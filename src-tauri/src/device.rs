#[cfg(test)]
use crate::hardware::board_by_runtime_usb;
use crate::{
    coordinator::{DeviceWorker, WorkerCommand, WorkerEvent, WorkerLauncher, WorkerStart},
    hardware::BoardProfile,
    metrics::{HomeMetricsSnapshot, MetricAttribution, MetricsStore},
    paste::{PasteHandle, PasteReply, PasteRequest},
    profile::DeviceProfile,
    protocol::{
        ActionSequence, DeviceMessage, HelloCapabilities, InputState, PhysicalInput, is_hello_line,
        parse_device, topology_commands, validate_hello,
    },
};
use serde::Serialize;
#[cfg(test)]
use serialport::{SerialPortInfo, SerialPortType};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::{BufRead, BufReader, ErrorKind, Write},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const ACTION_ACK_TIMEOUT: Duration = Duration::from_millis(1800);
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeActivity {
    pub code: String,
    pub params: BTreeMap<String, String>,
    pub detail: Option<String>,
    pub input: Option<PhysicalInput>,
    pub pressed: Option<bool>,
    #[serde(skip)]
    metric_press: Option<MetricPress>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MetricPress {
    attribution: MetricAttribution,
    button_id: String,
    occurred_at_ms: u64,
}

impl RuntimeActivity {
    pub(crate) fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            params: BTreeMap::new(),
            detail: None,
            input: None,
            pressed: None,
            metric_press: None,
        }
    }

    fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionOutput {
    pub lines: Vec<String>,
    pub activities: Vec<RuntimeActivity>,
    pub paste_requests: Vec<PendingPaste>,
    pub completed_receive_sequences: Vec<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingPaste {
    pub receive_sequence: u64,
    pub event_id: u64,
    pub step: u16,
    pub total: u16,
    pub text: String,
}

pub struct DeviceSession {
    profile: Option<RuntimeProfile>,
    candidate_board: &'static BoardProfile,
    hello: Option<HelloCapabilities>,
    revision: u32,
    configuring: Option<u32>,
    ready: bool,
    active: Option<ActionSequence>,
    queue: VecDeque<(u64, u64, PhysicalInput)>,
    active_receive_sequence: Option<u64>,
    pending_paste: Option<PendingPaste>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeProfile {
    pub profile: DeviceProfile,
    pub hardware_profile_id: String,
    pub metric_attribution: MetricAttribution,
}

impl DeviceSession {
    #[cfg(test)]
    pub fn new(profile: RuntimeProfile) -> Self {
        Self {
            profile: Some(profile),
            candidate_board: crate::hardware::board_by_id("luatos-esp32s3-aio").unwrap(),
            hello: None,
            revision: 0,
            configuring: None,
            ready: false,
            active: None,
            queue: VecDeque::new(),
            active_receive_sequence: None,
            pending_paste: None,
        }
    }

    pub fn without_model(candidate_board: &'static BoardProfile) -> Self {
        Self {
            profile: None,
            candidate_board,
            hello: None,
            revision: 0,
            configuring: None,
            ready: false,
            active: None,
            queue: VecDeque::new(),
            active_receive_sequence: None,
            pending_paste: None,
        }
    }

    pub fn replace_profile(&mut self, profile: Option<RuntimeProfile>) -> SessionOutput {
        let mut output = SessionOutput::default();
        if let Some(sequence) = self.active.take() {
            output.lines.push(format!("SKIP {}\n", sequence.event_id()));
        }
        if let Some(receive_sequence) = self.active_receive_sequence.take() {
            output.completed_receive_sequences.push(receive_sequence);
        }
        output
            .completed_receive_sequences
            .extend(self.queue.iter().map(|(sequence, _, _)| *sequence));
        self.queue.clear();
        self.pending_paste = None;
        self.ready = false;
        self.configuring = None;
        self.profile = profile;
        if let Some(hello) = self.hello.clone() {
            self.configure_for_hello(hello, &mut output);
        }
        output
    }

    pub fn is_awaiting_action(&self) -> bool {
        self.active.as_ref().is_some_and(ActionSequence::is_waiting)
    }

    pub fn fail_active_deferred(&mut self, code: &str, detail: Option<String>) -> SessionOutput {
        let mut output = SessionOutput::default();
        if let Some(mut sequence) = self.active.take() {
            let event_id = sequence.event_id();
            sequence.abort();
            output.lines.push(format!("SKIP {event_id}\n"));
            let mut activity = RuntimeActivity::new(code);
            activity.detail = detail;
            output.activities.push(activity);
            self.pending_paste = None;
            if let Some(receive_sequence) = self.active_receive_sequence.take() {
                output.completed_receive_sequences.push(receive_sequence);
            }
            self.start_next(&mut output);
        }
        output
    }

    #[cfg(test)]
    pub fn on_message(
        &mut self,
        message: DeviceMessage,
        copy: &mut impl FnMut(&str) -> Result<(), String>,
    ) -> SessionOutput {
        let receive_sequence = message_event_id(&message).unwrap_or(0);
        let output = self.on_message_deferred(message, receive_sequence, now_ms());
        self.resolve_immediate(output, copy)
    }

    #[cfg(test)]
    fn on_message_at(
        &mut self,
        message: DeviceMessage,
        occurred_at_ms: u64,
        copy: &mut impl FnMut(&str) -> Result<(), String>,
    ) -> SessionOutput {
        let receive_sequence = message_event_id(&message).unwrap_or(0);
        let output = self.on_message_deferred(message, receive_sequence, occurred_at_ms);
        self.resolve_immediate(output, copy)
    }

    pub fn on_message_deferred(
        &mut self,
        message: DeviceMessage,
        receive_sequence: u64,
        occurred_at_ms: u64,
    ) -> SessionOutput {
        let mut output = SessionOutput::default();
        match message {
            DeviceMessage::Hello(hello) => self.configure_for_hello(hello, &mut output),
            DeviceMessage::ConfigOk { revision } if self.configuring == Some(revision) => {
                self.configuring = None;
                self.ready = true;
                output.activities.push(
                    RuntimeActivity::new("topology_active")
                        .with_param("revision", revision.to_string()),
                );
            }
            DeviceMessage::ConfigError { revision, code } if self.configuring == Some(revision) => {
                self.configuring = None;
                self.ready = false;
                output.activities.push(
                    RuntimeActivity::new("topology_rejected")
                        .with_param("revision", revision.to_string())
                        .with_param("deviceCode", code),
                );
            }
            DeviceMessage::State {
                event_id,
                input,
                state,
            } => {
                let metric_press = (state == InputState::Down)
                    .then_some(self.profile.as_ref())
                    .flatten()
                    .and_then(|runtime| {
                        runtime
                            .profile
                            .button_for(&runtime.hardware_profile_id, &input)
                            .map(|button_id| MetricPress {
                                attribution: runtime.metric_attribution.clone(),
                                button_id: button_id.into(),
                                occurred_at_ms,
                            })
                    });
                output.activities.push(RuntimeActivity {
                    input: Some(input),
                    pressed: Some(state == InputState::Down),
                    metric_press,
                    ..RuntimeActivity::new("input_state")
                });
                if state == InputState::Down {
                    if self.ready {
                        self.queue.push_back((receive_sequence, event_id, input));
                        self.start_next(&mut output);
                    } else {
                        output.lines.push(format!("SKIP {event_id}\n"));
                        output.completed_receive_sequences.push(receive_sequence);
                        output
                            .activities
                            .push(RuntimeActivity::new("input_before_configuration"));
                    }
                } else {
                    output.completed_receive_sequences.push(receive_sequence);
                }
            }
            DeviceMessage::Done { event_id, step } => self.handle_done(event_id, step, &mut output),
            DeviceMessage::LearnOk { revision } => output.activities.push(
                RuntimeActivity::new("learning_ready").with_param("revision", revision.to_string()),
            ),
            DeviceMessage::LearnDirect { gpio, state } => {
                output.activities.push(RuntimeActivity {
                    input: Some(PhysicalInput::Direct { gpio }),
                    pressed: Some(state == InputState::Down),
                    ..RuntimeActivity::new("learning_input")
                });
            }
            DeviceMessage::LearnContact {
                pin_a,
                pin_b,
                state,
            } => output.activities.push(RuntimeActivity {
                input: Some(PhysicalInput::Contact {
                    source: 0,
                    pin_a,
                    pin_b,
                }),
                pressed: Some(state == InputState::Down),
                ..RuntimeActivity::new("learning_input")
            }),
            DeviceMessage::ConfigOk { .. } | DeviceMessage::ConfigError { .. } => {}
        }
        output
    }

    #[cfg(test)]
    pub fn on_line(
        &mut self,
        line: &str,
        copy: &mut impl FnMut(&str) -> Result<(), String>,
    ) -> SessionOutput {
        let output = self.on_line_deferred(line, 0, now_ms());
        self.resolve_immediate(output, copy)
    }

    #[cfg(test)]
    pub fn on_line_deferred(
        &mut self,
        line: &str,
        receive_sequence: u64,
        occurred_at_ms: u64,
    ) -> SessionOutput {
        match parse_device(line) {
            Some(message) => self.on_message_deferred(message, receive_sequence, occurred_at_ms),
            None if is_hello_line(line) => self.invalidate_hello(),
            None => SessionOutput::default(),
        }
    }

    fn configure_for_hello(&mut self, hello: HelloCapabilities, output: &mut SessionOutput) {
        self.clear_handshake();
        if let Err(error) = validate_hello(self.candidate_board, &hello) {
            output.activities.push(activity_from_error(error));
            return;
        }
        self.hello = Some(hello.clone());
        let Some(runtime) = self.profile.as_ref() else {
            output
                .activities
                .push(RuntimeActivity::new("no_runtime_assignment"));
            return;
        };
        if let Err(error) = runtime.profile.validate() {
            output
                .activities
                .push(RuntimeActivity::new("invalid_topology").with_detail(error.code));
            return;
        }
        let Some(hardware) = runtime
            .profile
            .hardware_profile(&runtime.hardware_profile_id)
        else {
            output
                .activities
                .push(RuntimeActivity::new("invalid_assignment"));
            return;
        };
        if hardware.board_profile_id != self.candidate_board.id {
            output
                .activities
                .push(RuntimeActivity::new("assignment_board_mismatch"));
            return;
        }
        let reported_pins = hello.pins.iter().copied().collect::<BTreeSet<_>>();
        self.revision = self.revision.wrapping_add(1).max(1);
        match topology_commands(hardware, self.revision, &reported_pins) {
            Ok(lines) => {
                self.configuring = Some(self.revision);
                output.lines = lines;
            }
            Err(error) => output.activities.push(activity_from_error(error)),
        }
    }

    #[cfg(test)]
    fn invalidate_hello(&mut self) -> SessionOutput {
        self.clear_handshake();
        SessionOutput {
            lines: Vec::new(),
            activities: vec![RuntimeActivity::new("protocol_mismatch")],
            ..SessionOutput::default()
        }
    }

    fn clear_handshake(&mut self) {
        self.hello = None;
        self.ready = false;
        self.configuring = None;
        self.active = None;
        self.active_receive_sequence = None;
        self.pending_paste = None;
        self.queue.clear();
    }

    fn handle_done(&mut self, event_id: u64, step: u16, output: &mut SessionOutput) {
        let Some(sequence) = self.active.as_mut() else {
            output
                .activities
                .push(RuntimeActivity::new("unexpected_action_acknowledgement"));
            return;
        };
        if let Err(error) = sequence.acknowledge(event_id, step) {
            let active_event = sequence.event_id();
            output.lines.push(format!("SKIP {active_event}\n"));
            output
                .activities
                .push(RuntimeActivity::new("invalid_action_acknowledgement").with_detail(error));
            self.active = None;
            self.pending_paste = None;
            if let Some(receive_sequence) = self.active_receive_sequence.take() {
                output.completed_receive_sequences.push(receive_sequence);
            }
            self.start_next(output);
            return;
        }
        if sequence.is_complete() {
            self.active = None;
            self.pending_paste = None;
            if let Some(receive_sequence) = self.active_receive_sequence.take() {
                output.completed_receive_sequences.push(receive_sequence);
            }
            self.start_next(output);
        } else {
            self.emit_active_step(output);
        }
    }

    fn start_next(&mut self, output: &mut SessionOutput) {
        while self.active.is_none() {
            let Some((receive_sequence, event_id, input)) = self.queue.pop_front() else {
                return;
            };
            let Some(runtime) = self.profile.as_ref() else {
                output.lines.push(format!("SKIP {event_id}\n"));
                output.completed_receive_sequences.push(receive_sequence);
                continue;
            };
            let Some(button) = runtime
                .profile
                .button_for(&runtime.hardware_profile_id, &input)
                .map(str::to_owned)
            else {
                output.lines.push(format!("SKIP {event_id}\n"));
                output.completed_receive_sequences.push(receive_sequence);
                output
                    .activities
                    .push(RuntimeActivity::new("unmapped_input"));
                continue;
            };
            let actions = runtime
                .profile
                .actions
                .get(&button)
                .cloned()
                .unwrap_or_default();
            if actions.is_empty() {
                output.lines.push(format!("SKIP {event_id}\n"));
                output.completed_receive_sequences.push(receive_sequence);
                output
                    .activities
                    .push(RuntimeActivity::new("empty_action_list").with_param("button", button));
                continue;
            }
            self.active = Some(ActionSequence::new(event_id, button, actions));
            self.active_receive_sequence = Some(receive_sequence);
            self.emit_active_step(output);
        }
    }

    fn emit_active_step(&mut self, output: &mut SessionOutput) {
        let Some(sequence) = self.active.as_mut() else {
            return;
        };
        let Some(step) = sequence.next_step() else {
            return;
        };
        match &step.action {
            crate::profile::ButtonAction::Paste { text } => {
                let request = PendingPaste {
                    receive_sequence: self.active_receive_sequence.unwrap_or_default(),
                    event_id: step.event_id,
                    step: step.step,
                    total: step.total,
                    text: text.clone(),
                };
                self.pending_paste = Some(request.clone());
                output.paste_requests.push(request);
            }
            crate::profile::ButtonAction::Hotkey { .. } => match step.command(|_| Ok(())) {
                Ok(line) => output.lines.push(line),
                Err(error) => {
                    sequence.abort();
                    output.lines.push(format!("SKIP {}\n", step.event_id));
                    output.activities.push(
                        RuntimeActivity::new("action_step_failed")
                            .with_param("button", step.button)
                            .with_param("step", step.step.to_string())
                            .with_detail(error),
                    );
                    self.active = None;
                    if let Some(receive_sequence) = self.active_receive_sequence.take() {
                        output.completed_receive_sequences.push(receive_sequence);
                    }
                    self.start_next(output);
                }
            },
        }
    }

    pub fn grant_paste(&mut self, event_id: u64, step: u16) -> SessionOutput {
        let mut output = SessionOutput::default();
        let Some(request) = self.pending_paste.take() else {
            output
                .activities
                .push(RuntimeActivity::new("unexpected_paste_grant"));
            return output;
        };
        if request.event_id == event_id && request.step == step {
            output.lines.push(format!(
                "PASTE {} {} {}\n",
                request.event_id, request.step, request.total
            ));
        } else {
            self.pending_paste = Some(request);
            output
                .activities
                .push(RuntimeActivity::new("paste_grant_mismatch"));
        }
        output
    }

    #[cfg(test)]
    fn resolve_immediate(
        &mut self,
        mut output: SessionOutput,
        copy: &mut impl FnMut(&str) -> Result<(), String>,
    ) -> SessionOutput {
        let requests = std::mem::take(&mut output.paste_requests);
        for request in requests {
            match copy(&request.text) {
                Ok(()) => merge_output(
                    &mut output,
                    self.grant_paste(request.event_id, request.step),
                ),
                Err(error) => {
                    merge_output(
                        &mut output,
                        self.fail_active_deferred("action_step_failed", Some(error)),
                    );
                }
            }
        }
        output
    }
}

#[cfg(test)]
fn message_event_id(message: &DeviceMessage) -> Option<u64> {
    match message {
        DeviceMessage::State { event_id, .. } | DeviceMessage::Done { event_id, .. } => {
            Some(*event_id)
        }
        _ => None,
    }
}

#[cfg(test)]
fn merge_output(target: &mut SessionOutput, mut source: SessionOutput) {
    target.lines.append(&mut source.lines);
    target.activities.append(&mut source.activities);
    target.paste_requests.append(&mut source.paste_requests);
    target
        .completed_receive_sequences
        .append(&mut source.completed_receive_sequences);
}

fn activity_from_error(error: crate::workspace::AppError) -> RuntimeActivity {
    let mut activity = RuntimeActivity::new(error.code);
    activity.params = error.params;
    activity.detail = error.detail;
    activity
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionState {
    Searching,
    Connected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub state: ConnectionState,
    pub port: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningSession {
    pub revision: u32,
    pub pins: Vec<u8>,
}

impl ConnectionStatus {
    pub fn searching() -> Self {
        Self {
            state: ConnectionState::Searching,
            port: None,
        }
    }
}

fn persist_metrics(
    metrics: &MetricsStore,
    activity: &RuntimeActivity,
    snapshot_at_ms: u64,
) -> Result<Option<HomeMetricsSnapshot>, rusqlite::Error> {
    let Some(metric_press) = activity.metric_press.as_ref() else {
        return Ok(None);
    };
    metrics.record_button_press(
        &metric_press.attribution,
        &metric_press.button_id,
        metric_press.occurred_at_ms,
    )?;
    metrics
        .home_snapshot(
            &metric_press.attribution.device_profile_id,
            None,
            snapshot_at_ms,
        )
        .map(Some)
}

#[cfg(test)]
pub fn is_target_port(port: &SerialPortInfo) -> bool {
    matches!(
        &port.port_type,
        SerialPortType::UsbPort(info) if board_by_runtime_usb(info.vid, info.pid).is_some()
    )
}

pub struct SystemWorkerLauncher {
    paste: PasteHandle,
    metrics: Option<Arc<MetricsStore>>,
    operation_barrier: Arc<RwLock<()>>,
}

impl SystemWorkerLauncher {
    pub fn new(
        paste: PasteHandle,
        metrics: Option<Arc<MetricsStore>>,
        operation_barrier: Arc<RwLock<()>>,
    ) -> Self {
        Self {
            paste,
            metrics,
            operation_barrier,
        }
    }
}

struct SystemDeviceWorker {
    commands: mpsc::Sender<WorkerCommand>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

struct PendingPasteReply {
    request: PendingPaste,
    replies: mpsc::Receiver<PasteReply>,
}

impl DeviceWorker for SystemDeviceWorker {
    fn send(&self, command: WorkerCommand) -> Result<(), String> {
        self.commands
            .send(command)
            .map_err(|_| "device_worker_stopped".into())
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.commands.send(WorkerCommand::Shutdown);
    }

    fn join(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl WorkerLauncher for SystemWorkerLauncher {
    fn start(
        &self,
        start: WorkerStart,
        events: mpsc::Sender<WorkerEvent>,
    ) -> Result<Box<dyn DeviceWorker>, String> {
        let (commands, command_receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let paste = self.paste.clone();
        let metrics = self.metrics.clone();
        let operation_barrier = Arc::clone(&self.operation_barrier);
        let join = thread::Builder::new()
            .name(format!("kivo-device-{}", start.device_id.as_str()))
            .spawn(move || {
                run_isolated_worker(
                    start,
                    command_receiver,
                    events,
                    paste,
                    metrics,
                    operation_barrier,
                    thread_stop,
                )
            })
            .map_err(|error| error.to_string())?;
        Ok(Box::new(SystemDeviceWorker {
            commands,
            stop,
            join: Some(join),
        }))
    }
}

fn run_isolated_worker(
    start: WorkerStart,
    commands: mpsc::Receiver<WorkerCommand>,
    events: mpsc::Sender<WorkerEvent>,
    paste: PasteHandle,
    metrics: Option<Arc<MetricsStore>>,
    operation_barrier: Arc<RwLock<()>>,
    stop: Arc<AtomicBool>,
) {
    let result = run_isolated_worker_inner(
        &start,
        &commands,
        &events,
        &paste,
        metrics.as_deref(),
        &operation_barrier,
        &stop,
    );
    let _ = paste.cancel_device(&start.device_id);
    if !stop.load(Ordering::Relaxed) {
        let _ = events.send(WorkerEvent::Disconnected {
            device_id: start.device_id,
            error: result.err(),
        });
    }
}

fn run_isolated_worker_inner(
    start: &WorkerStart,
    commands: &mpsc::Receiver<WorkerCommand>,
    events: &mpsc::Sender<WorkerEvent>,
    paste: &PasteHandle,
    metrics: Option<&MetricsStore>,
    operation_barrier: &RwLock<()>,
    stop: &AtomicBool,
) -> Result<(), String> {
    let board = crate::hardware::board_by_id(&start.board_profile_id)
        .ok_or_else(|| "unknown_board_profile".to_owned())?;
    let mut port = serialport::new(&start.port, 115_200)
        .timeout(Duration::from_millis(50))
        .open()
        .map_err(|error| format!("serial_open_failed: {error}"))?;
    port.write_data_terminal_ready(true)
        .and_then(|()| port.write_request_to_send(true))
        .map_err(|error| format!("serial_handshake_failed: {error}"))?;
    port.write_all(b"HELLO\n")
        .and_then(|()| port.flush())
        .map_err(|error| format!("serial_handshake_failed: {error}"))?;
    let mut device = BufReader::new(port);
    let hello = read_valid_hello(&mut device, board, stop)?;
    events
        .send(WorkerEvent::HelloValidated {
            device_id: start.device_id.clone(),
            capabilities: hello.clone(),
        })
        .map_err(|_| "coordinator_stopped".to_owned())?;
    let mut session = DeviceSession::without_model(board);
    let mut pending_paste: Option<PendingPasteReply> = None;
    let mut active_paste_ack = None;
    let mut action_deadline = None;
    let mut line = Vec::new();
    let initial = session.on_message_deferred(DeviceMessage::Hello(hello), 0, now_ms());
    write_isolated_output(
        start,
        events,
        paste,
        metrics,
        operation_barrier,
        device.get_mut(),
        initial,
        &mut pending_paste,
        &mut action_deadline,
    )?;

    while !stop.load(Ordering::Relaxed) {
        for command in commands.try_iter() {
            let output = match command {
                WorkerCommand::UpdateProfile(profile) => {
                    let _ = paste.cancel_device(&start.device_id);
                    pending_paste = None;
                    active_paste_ack = None;
                    action_deadline = None;
                    session.replace_profile(profile.as_deref().cloned())
                }
                WorkerCommand::Input {
                    receive_sequence,
                    event_id,
                    input,
                    state,
                    occurred_at_ms,
                } => session.on_message_deferred(
                    DeviceMessage::State {
                        event_id,
                        input,
                        state,
                    },
                    receive_sequence,
                    occurred_at_ms,
                ),
                WorkerCommand::Shutdown => return Ok(()),
            };
            write_isolated_output(
                start,
                events,
                paste,
                metrics,
                operation_barrier,
                device.get_mut(),
                output,
                &mut pending_paste,
                &mut action_deadline,
            )?;
        }

        if let Some(pending) = pending_paste.as_ref() {
            match pending.replies.try_recv() {
                Ok(PasteReply::Granted) => {
                    let request = pending.request.clone();
                    active_paste_ack = Some((request.event_id, request.step));
                    let output = session.grant_paste(request.event_id, request.step);
                    write_isolated_output(
                        start,
                        events,
                        paste,
                        metrics,
                        operation_barrier,
                        device.get_mut(),
                        output,
                        &mut pending_paste,
                        &mut action_deadline,
                    )?;
                }
                Ok(PasteReply::TimedOut) => {
                    pending_paste = None;
                    active_paste_ack = None;
                    let output = session.fail_active_deferred("action_ack_timeout", None);
                    write_isolated_output(
                        start,
                        events,
                        paste,
                        metrics,
                        operation_barrier,
                        device.get_mut(),
                        output,
                        &mut pending_paste,
                        &mut action_deadline,
                    )?;
                }
                Ok(PasteReply::Cancelled) => {
                    pending_paste = None;
                    active_paste_ack = None;
                    let output = session.fail_active_deferred("action_cancelled", None);
                    write_isolated_output(
                        start,
                        events,
                        paste,
                        metrics,
                        operation_barrier,
                        device.get_mut(),
                        output,
                        &mut pending_paste,
                        &mut action_deadline,
                    )?;
                }
                Ok(PasteReply::ClipboardError(error)) => {
                    pending_paste = None;
                    active_paste_ack = None;
                    let output = session.fail_active_deferred("action_step_failed", Some(error));
                    write_isolated_output(
                        start,
                        events,
                        paste,
                        metrics,
                        operation_barrier,
                        device.get_mut(),
                        output,
                        &mut pending_paste,
                        &mut action_deadline,
                    )?;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("paste_coordinator_stopped".into());
                }
            }
        }

        if action_deadline.is_some_and(|deadline| Instant::now() >= deadline)
            && pending_paste.is_none()
            && active_paste_ack.is_none()
        {
            let output = session.fail_active_deferred("action_ack_timeout", None);
            write_isolated_output(
                start,
                events,
                paste,
                metrics,
                operation_barrier,
                device.get_mut(),
                output,
                &mut pending_paste,
                &mut action_deadline,
            )?;
        }
        if !session.is_awaiting_action() && pending_paste.is_none() {
            action_deadline = None;
        }

        line.clear();
        match device.read_until(b'\n', &mut line) {
            Ok(0) => return Err("device_disconnected".into()),
            Ok(_) => {
                let received_at_ms = now_ms();
                let Ok(text) = std::str::from_utf8(&line) else {
                    continue;
                };
                let Some(message) = parse_device(text) else {
                    if is_hello_line(text) {
                        return Err("protocol_mismatch".into());
                    }
                    continue;
                };
                match message {
                    DeviceMessage::State {
                        event_id,
                        input,
                        state,
                    } => {
                        events
                            .send(WorkerEvent::Input {
                                device_id: start.device_id.clone(),
                                event_id,
                                input,
                                state,
                                occurred_at_ms: received_at_ms,
                            })
                            .map_err(|_| "coordinator_stopped".to_owned())?;
                    }
                    DeviceMessage::Done { event_id, step } => {
                        if active_paste_ack == Some((event_id, step)) {
                            if paste.complete(&start.device_id, event_id, step).is_err() {
                                continue;
                            }
                            active_paste_ack = None;
                            pending_paste = None;
                        }
                        let output = session.on_message_deferred(
                            DeviceMessage::Done { event_id, step },
                            0,
                            received_at_ms,
                        );
                        write_isolated_output(
                            start,
                            events,
                            paste,
                            metrics,
                            operation_barrier,
                            device.get_mut(),
                            output,
                            &mut pending_paste,
                            &mut action_deadline,
                        )?;
                    }
                    DeviceMessage::Hello(ref capability) => {
                        validate_hello(board, capability).map_err(|error| error.code.clone())?;
                        let output = session.on_message_deferred(message, 0, received_at_ms);
                        write_isolated_output(
                            start,
                            events,
                            paste,
                            metrics,
                            operation_barrier,
                            device.get_mut(),
                            output,
                            &mut pending_paste,
                            &mut action_deadline,
                        )?;
                    }
                    message => {
                        let output = session.on_message_deferred(message, 0, received_at_ms);
                        write_isolated_output(
                            start,
                            events,
                            paste,
                            metrics,
                            operation_barrier,
                            device.get_mut(),
                            output,
                            &mut pending_paste,
                            &mut action_deadline,
                        )?;
                    }
                }
            }
            Err(error) if error.kind() == ErrorKind::TimedOut => {}
            Err(error) => return Err(format!("serial_read_failed: {error}")),
        }
    }
    Ok(())
}

fn read_valid_hello<R: BufRead>(
    reader: &mut R,
    board: &BoardProfile,
    stop: &AtomicBool,
) -> Result<HelloCapabilities, String> {
    let deadline = Instant::now() + ACTION_ACK_TIMEOUT;
    let mut line = Vec::new();
    while !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => return Err("device_disconnected".into()),
            Ok(_) => {
                let Ok(text) = std::str::from_utf8(&line) else {
                    continue;
                };
                match parse_device(text) {
                    Some(DeviceMessage::Hello(hello)) => {
                        validate_hello(board, &hello).map_err(|error| error.code)?;
                        return Ok(hello);
                    }
                    _ if is_hello_line(text) => return Err("protocol_mismatch".into()),
                    _ => {}
                }
            }
            Err(error) if error.kind() == ErrorKind::TimedOut => {}
            Err(error) => return Err(format!("serial_handshake_failed: {error}")),
        }
    }
    Err("serial_handshake_timeout".into())
}

#[allow(clippy::too_many_arguments)]
fn write_isolated_output<W: Write + ?Sized>(
    start: &WorkerStart,
    events: &mpsc::Sender<WorkerEvent>,
    paste: &PasteHandle,
    metrics: Option<&MetricsStore>,
    operation_barrier: &RwLock<()>,
    writer: &mut W,
    mut output: SessionOutput,
    pending_paste: &mut Option<PendingPasteReply>,
    action_deadline: &mut Option<Instant>,
) -> Result<(), String> {
    for activity in output.activities.drain(..) {
        if let Some(metrics) = metrics {
            let _operation = operation_barrier
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            persist_metrics(metrics, &activity, now_ms())
                .map_err(|error| format!("metrics_write_failed: {error}"))?;
        }
        events
            .send(WorkerEvent::Activity {
                device_id: start.device_id.clone(),
                activity,
            })
            .map_err(|_| "coordinator_stopped".to_owned())?;
    }
    let completed_sequence = !output.completed_receive_sequences.is_empty();
    for receive_sequence in output.completed_receive_sequences.drain(..) {
        events
            .send(WorkerEvent::SequenceFinished {
                device_id: start.device_id.clone(),
                receive_sequence,
            })
            .map_err(|_| "coordinator_stopped".to_owned())?;
    }
    for request in output.paste_requests.drain(..) {
        if pending_paste.is_some() {
            return Err("multiple_pending_paste_requests".into());
        }
        let (reply, replies) = mpsc::channel();
        paste
            .submit(PasteRequest {
                receive_sequence: request.receive_sequence,
                device_id: start.device_id.clone(),
                event_id: request.event_id,
                step: request.step,
                text: request.text.clone(),
                reply,
            })
            .map_err(|error| format!("paste_submit_failed: {error}"))?;
        *pending_paste = Some(PendingPasteReply { request, replies });
    }
    let sent_action = output
        .lines
        .iter()
        .any(|line| line.starts_with("PASTE ") || line.starts_with("HOTKEY "));
    for line in output.lines {
        writer
            .write_all(line.as_bytes())
            .map_err(|error| format!("serial_write_failed: {error}"))?;
    }
    writer
        .flush()
        .map_err(|error| format!("serial_write_failed: {error}"))?;
    if sent_action {
        *action_deadline = Some(Instant::now() + ACTION_ACK_TIMEOUT);
    } else if completed_sequence {
        *action_deadline = None;
    }
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        hardware::DeviceId,
        metrics::{MetricAttribution, MetricsStore},
        model::{ButtonDefinition, ButtonGroup, ModelLayout},
        profile::{
            ButtonAction, DeviceProfile, HardwareProfile, InputSource, PROFILE_SCHEMA_VERSION,
        },
        protocol::{DeviceMessage, PhysicalInput},
    };
    use serialport::{SerialPortInfo, SerialPortType, UsbPortInfo};
    use std::{cell::RefCell, collections::BTreeMap};

    fn runtime_model() -> RuntimeProfile {
        RuntimeProfile {
            hardware_profile_id: "esp-primary".into(),
            metric_attribution: MetricAttribution {
                device_id: DeviceId::new("luatos-esp32s3-aio", "ABCDEF123456").unwrap(),
                device_name: "Desk".into(),
                device_profile_id: "phone".into(),
                hardware_profile_id: "esp-primary".into(),
            },
            profile: DeviceProfile {
                schema_version: PROFILE_SCHEMA_VERSION,
                profile: ModelLayout {
                    id: "phone".into(),
                    name: "电话".into(),
                    groups: vec![ButtonGroup {
                        id: "keys".into(),
                        columns: 1,
                        buttons: vec![ButtonDefinition {
                            id: "A".into(),
                            label: "甲".into(),
                        }],
                    }],
                },
                hardware_profiles: vec![HardwareProfile {
                    id: "esp-primary".into(),
                    name: "ESP primary".into(),
                    board_profile_id: "luatos-esp32s3-aio".into(),
                    debounce_ms: 30,
                    inputs: vec![InputSource::Direct {
                        id: "direct".into(),
                        keys: BTreeMap::from([("A".into(), 6)]),
                    }],
                }],
                actions: BTreeMap::from([(
                    "A".into(),
                    vec![
                        ButtonAction::Paste {
                            text: "第一步".into(),
                        },
                        ButtonAction::Paste {
                            text: "第二步".into(),
                        },
                    ],
                )]),
            },
        }
    }

    #[test]
    fn persists_a_mapped_button_press_as_metrics_and_activity() {
        let mut session = DeviceSession::new(runtime_model());
        session.ready = true;
        let path = std::env::temp_dir().join(format!(
            "kivo-device-metrics-{}-{}.sqlite3",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = MetricsStore::open(&path).unwrap();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let output = session.on_message(
            DeviceMessage::State {
                event_id: 1,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            &mut |_| Ok(()),
        );

        let update = persist_metrics(&store, &output.activities[0], timestamp)
            .unwrap()
            .unwrap();

        assert_eq!(update.today_presses, 1);
        assert_eq!(update.logs[0].message, "A pressed");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn persists_the_session_profile_that_interpreted_the_event() {
        let original = runtime_model();
        let original_device = original.metric_attribution.device_id.clone();
        let active_profile = RwLock::new(Some(original.clone()));
        let mut session = DeviceSession::new(original);
        session.ready = true;
        let output = session.on_message(
            DeviceMessage::State {
                event_id: 1,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            &mut |_| Ok(()),
        );
        let mut reassigned = runtime_model();
        reassigned.profile.profile.id = "console".into();
        reassigned.metric_attribution.device_name = "Renamed desk".into();
        reassigned.metric_attribution.device_profile_id = "console".into();
        reassigned.metric_attribution.hardware_profile_id = "esp-alternate".into();
        *active_profile.write().unwrap() = Some(reassigned);
        let path = std::env::temp_dir().join(format!(
            "kivo-device-attribution-{}-{}.sqlite3",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = MetricsStore::open(&path).unwrap();

        persist_metrics(&store, &output.activities[0], 1_720_086_400_000).unwrap();

        let original_snapshot = store
            .home_snapshot("phone", Some(&original_device), 1_720_086_400_000)
            .unwrap();
        assert_eq!(original_snapshot.total_presses, 1);
        assert_eq!(original_snapshot.logs[0].device_name, "Desk");
        assert_eq!(
            store
                .home_snapshot("console", Some(&original_device), 1_720_086_400_000)
                .unwrap()
                .total_presses,
            0
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn persists_receive_time_after_action_work_crosses_a_day_boundary() {
        let mut session = DeviceSession::new(runtime_model());
        session.ready = true;
        let received_at_ms = 1_720_000_000_000;
        let persisted_at_ms = received_at_ms + 86_400_000;
        let output = session.on_message_at(
            DeviceMessage::State {
                event_id: 1,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            received_at_ms,
            &mut |_| Ok(()),
        );
        let path = std::env::temp_dir().join(format!(
            "kivo-device-receive-time-{}-{}.sqlite3",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = MetricsStore::open(&path).unwrap();

        persist_metrics(&store, &output.activities[0], persisted_at_ms).unwrap();

        let backup = store.backup().unwrap();
        assert_eq!(backup.activity_logs[0].occurred_at_ms, received_at_ms);
        assert_eq!(
            store
                .home_snapshot("phone", None, received_at_ms)
                .unwrap()
                .today_presses,
            1
        );
        assert_eq!(
            store
                .home_snapshot("phone", None, persisted_at_ms)
                .unwrap()
                .today_presses,
            0
        );
        std::fs::remove_file(path).unwrap();
    }

    fn hello() -> DeviceMessage {
        DeviceMessage::Hello(HelloCapabilities {
            protocol: 3,
            controller_family_id: "esp32s3".into(),
            board_profile_id: "luatos-esp32s3-aio".into(),
            firmware_build_id: "test".into(),
            pins: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18],
        })
    }

    #[test]
    fn serializes_actions_and_queues_presses_until_done() {
        let mut session = DeviceSession::new(runtime_model());
        let copied = RefCell::new(Vec::new());
        let mut copy = |text: &str| {
            copied.borrow_mut().push(text.to_owned());
            Ok(())
        };

        let before_config = session.on_message(
            DeviceMessage::State {
                event_id: 8,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            &mut copy,
        );
        assert_eq!(before_config.lines, ["SKIP 8\n"]);

        let configuring = session.on_message(hello(), &mut copy);
        assert_eq!(configuring.lines[0], "CONFIG_BEGIN 1 30\n");
        assert_eq!(configuring.lines.last().unwrap(), "CONFIG_COMMIT 1\n");
        session.on_message(DeviceMessage::ConfigOk { revision: 1 }, &mut copy);

        let first = session.on_message(
            DeviceMessage::State {
                event_id: 9,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            &mut copy,
        );
        assert_eq!(copied.borrow().as_slice(), ["第一步"]);
        assert!(first.lines[0].contains(" 9 1 2"));
        let queued = session.on_message(
            DeviceMessage::State {
                event_id: 10,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            &mut copy,
        );
        assert!(queued.lines.is_empty());

        let second = session.on_message(
            DeviceMessage::Done {
                event_id: 9,
                step: 1,
            },
            &mut copy,
        );
        assert_eq!(copied.borrow().as_slice(), ["第一步", "第二步"]);
        assert!(second.lines[0].contains(" 9 2 2"));
        let next_press = session.on_message(
            DeviceMessage::Done {
                event_id: 9,
                step: 2,
            },
            &mut copy,
        );
        assert_eq!(copied.borrow().as_slice(), ["第一步", "第二步", "第一步"]);
        assert!(next_press.lines[0].contains(" 10 1 2"));
    }

    #[test]
    fn deferred_paste_waits_for_global_grant_before_emitting_device_command() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            vec![
                ButtonAction::Paste {
                    text: "global".into(),
                },
                ButtonAction::Hotkey {
                    keys: vec!["enter".into()],
                },
            ],
        );
        let mut session = DeviceSession::new(runtime);
        session.ready = true;

        let pending = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 41,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            7,
            123,
        );

        assert!(pending.lines.is_empty());
        assert_eq!(
            pending.paste_requests,
            vec![PendingPaste {
                receive_sequence: 7,
                event_id: 41,
                step: 1,
                total: 2,
                text: "global".into(),
            }]
        );

        let granted = session.grant_paste(41, 1);
        assert_eq!(granted.lines, ["PASTE 41 1 2\n"]);
        let next = session.on_message_deferred(
            DeviceMessage::Done {
                event_id: 41,
                step: 1,
            },
            0,
            124,
        );
        assert_eq!(next.lines, ["HOTKEY 41 2 2 0 40\n"]);
        assert!(next.paste_requests.is_empty());
    }

    #[test]
    fn hotkey_only_action_bypasses_global_paste_coordinator() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            vec![ButtonAction::Hotkey {
                keys: vec!["enter".into()],
            }],
        );
        let mut session = DeviceSession::new(runtime);
        session.ready = true;

        let output = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 42,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            8,
            125,
        );

        assert_eq!(output.lines, ["HOTKEY 42 1 1 0 40\n"]);
        assert!(output.paste_requests.is_empty());
    }

    #[test]
    fn replacing_profile_completes_in_flight_receive_sequence() {
        let mut session = DeviceSession::new(runtime_model());
        session.ready = true;
        let pending = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 43,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            9,
            126,
        );
        assert_eq!(pending.paste_requests.len(), 1);

        let replaced = session.replace_profile(None);

        assert_eq!(replaced.lines, ["SKIP 43\n"]);
        assert_eq!(replaced.completed_receive_sequences, [9]);
        assert_eq!(
            session.grant_paste(43, 1).activities[0].code,
            "unexpected_paste_grant"
        );
    }

    #[test]
    fn rejects_unsupported_hello_without_sending_topology() {
        let mut session = DeviceSession::new(runtime_model());
        let output = session.on_message(
            DeviceMessage::Hello(HelloCapabilities {
                protocol: 3,
                controller_family_id: "esp32s3".into(),
                board_profile_id: "luatos-esp32s3-aio".into(),
                firmware_build_id: "test".into(),
                pins: vec![1, 2],
            }),
            &mut |_| Ok(()),
        );

        assert!(output.lines.is_empty());
        assert_eq!(output.activities[0].code, "capability_mismatch");
        assert_eq!(output.activities[0].params["gpio"], "6");
    }

    #[test]
    fn invalid_hello_lines_clear_ready_session() {
        let mut session = DeviceSession::new(runtime_model());
        let DeviceMessage::Hello(hello) = hello() else {
            unreachable!();
        };
        let mut copy = |_: &str| -> Result<(), String> { Ok(()) };

        session.on_message(DeviceMessage::Hello(hello.clone()), &mut copy);
        session.on_message(DeviceMessage::ConfigOk { revision: 1 }, &mut copy);
        assert!(session.ready);
        session.on_message(
            DeviceMessage::State {
                event_id: 8,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            &mut copy,
        );
        assert!(session.active.is_some());

        session.on_line("HELLO 2 esp32s3", &mut copy);
        assert!(!session.ready);
        assert!(session.hello.is_none());
        assert!(session.configuring.is_none());
        assert!(session.active.is_none());
        assert!(session.queue.is_empty());
        let state = session.on_message(
            DeviceMessage::State {
                event_id: 9,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            &mut copy,
        );
        assert_eq!(state.lines, ["SKIP 9\n"]);

        session.on_message(DeviceMessage::Hello(hello.clone()), &mut copy);
        session.on_message(DeviceMessage::ConfigOk { revision: 2 }, &mut copy);
        assert!(session.ready);

        session.on_line("HELLO 3 esp32s3 luatos-esp32s3-aio build 2 0", &mut copy);
        assert!(!session.ready);
        assert!(session.hello.is_none());

        session.on_message(DeviceMessage::Hello(hello.clone()), &mut copy);
        session.on_message(DeviceMessage::ConfigOk { revision: 3 }, &mut copy);
        let _ = hello;
        session.on_line("HELLO 3 esp32s3 vccgnd-yd-rp2040 build 2 0 6", &mut copy);
        assert!(!session.ready);
        assert!(session.hello.is_none());
    }

    fn usb_port(vid: u16, pid: u16, product: Option<&str>) -> SerialPortInfo {
        SerialPortInfo {
            port_name: "/dev/cu.test".to_owned(),
            port_type: SerialPortType::UsbPort(UsbPortInfo {
                vid,
                pid,
                serial_number: None,
                manufacturer: None,
                product: product.map(str::to_owned),
            }),
        }
    }

    #[test]
    fn identifies_only_the_expected_usb_device() {
        assert!(is_target_port(&usb_port(
            0x303a,
            0x4002,
            Some("USB Serial Device (COM3)")
        )));
        assert!(is_target_port(&usb_port(0x303a, 0x4002, None)));
        assert!(is_target_port(&usb_port(
            0x2e8a,
            0x102e,
            Some("Kivo Keyboard")
        )));
        assert!(!is_target_port(&usb_port(
            0x303b,
            0x4002,
            Some("Kivo Keyboard")
        )));
        assert!(!is_target_port(&usb_port(
            0x303a,
            0x4003,
            Some("Kivo Keyboard")
        )));
        assert!(!is_target_port(&SerialPortInfo {
            port_name: "/dev/cu.Bluetooth".to_owned(),
            port_type: SerialPortType::BluetoothPort,
        }));
    }
}
