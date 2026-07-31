use crate::{
    hardware::{BoardProfile, board_by_runtime_usb},
    metrics::{HomeMetricsSnapshot, MetricAttribution, MetricsStore},
    profile::DeviceProfile,
    protocol::{
        ActionSequence, DeviceMessage, HelloCapabilities, InputState, PhysicalInput, is_hello_line,
        parse_device, topology_commands, validate_hello,
    },
};
use serde::Serialize;
use serialport::{SerialPortInfo, SerialPortType};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::{BufRead, BufReader, ErrorKind, Write},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};

const ACTION_ACK_TIMEOUT: Duration = Duration::from_millis(1800);
const CLIPBOARD_COMMAND: &str = if cfg!(target_os = "windows") {
    "clip.exe"
} else {
    "/usr/bin/pbcopy"
};

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
}

impl RuntimeActivity {
    fn new(code: impl Into<String>) -> Self {
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
}

pub struct DeviceSession {
    profile: Option<RuntimeProfile>,
    candidate_board: &'static BoardProfile,
    hello: Option<HelloCapabilities>,
    revision: u32,
    configuring: Option<u32>,
    ready: bool,
    active: Option<ActionSequence>,
    queue: VecDeque<(u64, PhysicalInput)>,
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
        }
    }

    pub fn replace_profile(&mut self, profile: Option<RuntimeProfile>) -> SessionOutput {
        let mut output = SessionOutput::default();
        if let Some(sequence) = self.active.take() {
            output.lines.push(format!("SKIP {}\n", sequence.event_id()));
        }
        self.queue.clear();
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

    pub fn fail_active(
        &mut self,
        code: &str,
        detail: Option<String>,
        copy: &mut impl FnMut(&str) -> Result<(), String>,
    ) -> SessionOutput {
        let mut output = SessionOutput::default();
        if let Some(mut sequence) = self.active.take() {
            let event_id = sequence.event_id();
            sequence.abort();
            output.lines.push(format!("SKIP {event_id}\n"));
            let mut activity = RuntimeActivity::new(code);
            activity.detail = detail;
            output.activities.push(activity);
            self.start_next(&mut output, copy);
        }
        output
    }

    pub fn on_message(
        &mut self,
        message: DeviceMessage,
        copy: &mut impl FnMut(&str) -> Result<(), String>,
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
                        self.queue.push_back((event_id, input));
                        self.start_next(&mut output, copy);
                    } else {
                        output.lines.push(format!("SKIP {event_id}\n"));
                        output
                            .activities
                            .push(RuntimeActivity::new("input_before_configuration"));
                    }
                }
            }
            DeviceMessage::Done { event_id, step } => {
                self.handle_done(event_id, step, &mut output, copy)
            }
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

    pub fn on_line(
        &mut self,
        line: &str,
        copy: &mut impl FnMut(&str) -> Result<(), String>,
    ) -> SessionOutput {
        match parse_device(line) {
            Some(message) => self.on_message(message, copy),
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

    fn invalidate_hello(&mut self) -> SessionOutput {
        self.clear_handshake();
        SessionOutput {
            lines: Vec::new(),
            activities: vec![RuntimeActivity::new("protocol_mismatch")],
        }
    }

    fn clear_handshake(&mut self) {
        self.hello = None;
        self.ready = false;
        self.configuring = None;
        self.active = None;
        self.queue.clear();
    }

    fn handle_done(
        &mut self,
        event_id: u64,
        step: u16,
        output: &mut SessionOutput,
        copy: &mut impl FnMut(&str) -> Result<(), String>,
    ) {
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
            self.start_next(output, copy);
            return;
        }
        if sequence.is_complete() {
            self.active = None;
            self.start_next(output, copy);
        } else {
            self.emit_active_step(output, copy);
        }
    }

    fn start_next(
        &mut self,
        output: &mut SessionOutput,
        copy: &mut impl FnMut(&str) -> Result<(), String>,
    ) {
        while self.active.is_none() {
            let Some((event_id, input)) = self.queue.pop_front() else {
                return;
            };
            let Some(runtime) = self.profile.as_ref() else {
                output.lines.push(format!("SKIP {event_id}\n"));
                continue;
            };
            let Some(button) = runtime
                .profile
                .button_for(&runtime.hardware_profile_id, &input)
                .map(str::to_owned)
            else {
                output.lines.push(format!("SKIP {event_id}\n"));
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
                output
                    .activities
                    .push(RuntimeActivity::new("empty_action_list").with_param("button", button));
                continue;
            }
            self.active = Some(ActionSequence::new(event_id, button, actions));
            self.emit_active_step(output, copy);
        }
    }

    fn emit_active_step(
        &mut self,
        output: &mut SessionOutput,
        copy: &mut impl FnMut(&str) -> Result<(), String>,
    ) {
        let Some(sequence) = self.active.as_mut() else {
            return;
        };
        let Some(step) = sequence.next_step() else {
            return;
        };
        match step.command(|text| copy(text)) {
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
                self.start_next(output, copy);
            }
        }
    }
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

pub type DeviceCapabilities = HelloCapabilities;

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

    fn connected(port: String) -> Self {
        Self {
            state: ConnectionState::Connected,
            port: Some(port),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EventLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone)]
pub struct WorkerState {
    pub active_profile: Arc<RwLock<Option<RuntimeProfile>>>,
    pub connection: Arc<RwLock<ConnectionStatus>>,
    pub capabilities: Arc<RwLock<Option<DeviceCapabilities>>>,
    pub runtime_error: Arc<RwLock<Option<RuntimeActivity>>>,
    pub learning: Arc<RwLock<Option<LearningSession>>>,
    pub metrics: Option<Arc<MetricsStore>>,
    pub controls: Arc<Mutex<VecDeque<String>>>,
    pub stop: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEvent {
    pub timestamp_ms: u64,
    pub level: EventLevel,
    pub connection: ConnectionStatus,
    pub home_update: Option<HomeMetricsSnapshot>,
    #[serde(flatten)]
    pub activity: RuntimeActivity,
}

fn persist_metrics(
    metrics: &MetricsStore,
    activity: &RuntimeActivity,
    timestamp_ms: u64,
) -> Result<Option<HomeMetricsSnapshot>, rusqlite::Error> {
    let Some(metric_press) = activity.metric_press.as_ref() else {
        return Ok(None);
    };
    metrics.record_button_press(
        &metric_press.attribution,
        &metric_press.button_id,
        timestamp_ms,
    )?;
    metrics
        .home_snapshot(
            &metric_press.attribution.device_profile_id,
            None,
            timestamp_ms,
        )
        .map(Some)
}

pub fn is_target_port(port: &SerialPortInfo) -> bool {
    matches!(
        &port.port_type,
        SerialPortType::UsbPort(info) if board_by_runtime_usb(info.vid, info.pid).is_some()
    )
}

fn update_published_capabilities(
    capabilities: &RwLock<Option<DeviceCapabilities>>,
    candidate_board: &BoardProfile,
    line: &str,
) {
    if !is_hello_line(line) {
        return;
    }
    let capability = match parse_device(line) {
        Some(DeviceMessage::Hello(hello)) if validate_hello(candidate_board, &hello).is_ok() => {
            Some(hello)
        }
        _ => None,
    };
    *capabilities
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = capability;
}

pub fn run_worker(app: AppHandle, state: WorkerState) {
    let WorkerState {
        active_profile,
        connection,
        capabilities,
        runtime_error,
        learning,
        metrics,
        controls,
        stop,
    } = state;
    while !stop.load(Ordering::Relaxed) {
        let target = match serialport::available_ports() {
            Ok(ports) => ports.into_iter().find_map(|port| match &port.port_type {
                SerialPortType::UsbPort(info) => {
                    board_by_runtime_usb(info.vid, info.pid).map(|board| (port, board))
                }
                _ => None,
            }),
            Err(error) => {
                emit_activity(
                    &app,
                    &connection,
                    EventLevel::Warning,
                    RuntimeActivity::new("serial_scan_failed").with_detail(error.to_string()),
                );
                wait(&stop);
                continue;
            }
        };
        let Some((port, candidate_board)) = target else {
            clear_device_state(&capabilities, &learning, &controls);
            set_connection(
                &app,
                &connection,
                ConnectionStatus::searching(),
                Some("device_searching"),
            );
            wait(&stop);
            continue;
        };

        let mut device = match serialport::new(&port.port_name, 115_200)
            .timeout(Duration::from_millis(500))
            .open()
        {
            Ok(device) => device,
            Err(error) => {
                emit_activity(
                    &app,
                    &connection,
                    EventLevel::Warning,
                    RuntimeActivity::new("serial_open_failed")
                        .with_param("port", &port.port_name)
                        .with_detail(error.to_string()),
                );
                wait(&stop);
                continue;
            }
        };
        let handshake = device
            .write_data_terminal_ready(true)
            .and_then(|()| device.write_request_to_send(true))
            .map_err(|error| error.to_string())
            .and_then(|()| {
                device
                    .write_all(b"HELLO\n")
                    .and_then(|()| device.flush())
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = handshake {
            emit_activity(
                &app,
                &connection,
                EventLevel::Warning,
                RuntimeActivity::new("serial_handshake_failed")
                    .with_param("port", &port.port_name)
                    .with_detail(error),
            );
            wait(&stop);
            continue;
        }

        set_connection(
            &app,
            &connection,
            ConnectionStatus::connected(port.port_name.clone()),
            Some("device_connected"),
        );
        let mut device = BufReader::new(device);
        let mut session = DeviceSession::without_model(candidate_board);
        let mut loaded_model = None;
        let mut action_deadline = None;
        let mut line = Vec::new();
        while !stop.load(Ordering::Relaxed) {
            let next_profile = active_profile
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            if next_profile != loaded_model {
                loaded_model = next_profile.clone();
                let output = session.replace_profile(next_profile);
                match write_output(
                    &app,
                    &connection,
                    &runtime_error,
                    metrics.as_deref(),
                    device.get_mut(),
                    output,
                ) {
                    Ok(sent_action) => {
                        update_action_deadline(&mut action_deadline, &session, sent_action)
                    }
                    Err(error) => {
                        emit_serial_write_error(&app, &connection, error);
                        break;
                    }
                }
            }
            if action_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                let output =
                    session.fail_active("action_ack_timeout", None, &mut copy_to_clipboard);
                match write_output(
                    &app,
                    &connection,
                    &runtime_error,
                    metrics.as_deref(),
                    device.get_mut(),
                    output,
                ) {
                    Ok(sent_action) => {
                        update_action_deadline(&mut action_deadline, &session, sent_action)
                    }
                    Err(error) => {
                        emit_serial_write_error(&app, &connection, error);
                        break;
                    }
                }
            }
            let control_lines = controls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .drain(..)
                .collect::<Vec<_>>();
            if !control_lines.is_empty() {
                match write_output(
                    &app,
                    &connection,
                    &runtime_error,
                    metrics.as_deref(),
                    device.get_mut(),
                    SessionOutput {
                        lines: control_lines,
                        activities: Vec::new(),
                    },
                ) {
                    Ok(_) => {}
                    Err(error) => {
                        emit_serial_write_error(&app, &connection, error);
                        break;
                    }
                }
            }
            line.clear();
            match device.read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let Ok(text) = std::str::from_utf8(&line) else {
                        continue;
                    };
                    update_published_capabilities(&capabilities, candidate_board, text);
                    let output = session.on_line(text, &mut copy_to_clipboard);
                    match write_output(
                        &app,
                        &connection,
                        &runtime_error,
                        metrics.as_deref(),
                        device.get_mut(),
                        output,
                    ) {
                        Ok(sent_action) => {
                            update_action_deadline(&mut action_deadline, &session, sent_action)
                        }
                        Err(error) => {
                            emit_serial_write_error(&app, &connection, error);
                            break;
                        }
                    }
                }
                Err(error) if error.kind() == ErrorKind::TimedOut => continue,
                Err(error) => {
                    emit_activity(
                        &app,
                        &connection,
                        EventLevel::Warning,
                        RuntimeActivity::new("device_disconnected").with_detail(error.to_string()),
                    );
                    break;
                }
            }
        }
        clear_device_state(&capabilities, &learning, &controls);
        set_connection(
            &app,
            &connection,
            ConnectionStatus::searching(),
            Some("device_searching"),
        );
    }
}

fn write_output<W: Write + ?Sized>(
    app: &AppHandle,
    connection: &RwLock<ConnectionStatus>,
    runtime_error: &RwLock<Option<RuntimeActivity>>,
    metrics: Option<&MetricsStore>,
    writer: &mut W,
    output: SessionOutput,
) -> std::io::Result<bool> {
    for activity in output.activities {
        let level = activity_level(&activity.code);
        if level == EventLevel::Error {
            *runtime_error
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(activity.clone());
        } else if activity.code == "topology_active" {
            *runtime_error
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        }
        let timestamp_ms = now_ms();
        let home_update =
            metrics.and_then(
                |metrics| match persist_metrics(metrics, &activity, timestamp_ms) {
                    Ok(update) => update,
                    Err(error) => {
                        *runtime_error
                            .write()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(
                            RuntimeActivity::new("metrics_write_failed")
                                .with_detail(error.to_string()),
                        );
                        None
                    }
                },
            );
        emit_activity_with_home_update(app, connection, level, activity, home_update);
    }
    let sent_action = output
        .lines
        .iter()
        .any(|line| line.starts_with("PASTE ") || line.starts_with("HOTKEY "));
    for line in output.lines {
        writer.write_all(line.as_bytes())?;
    }
    writer.flush()?;
    Ok(sent_action)
}

fn activity_level(code: &str) -> EventLevel {
    match code {
        "topology_active" | "input_state" | "learning_ready" | "learning_input" => EventLevel::Info,
        "input_before_configuration"
        | "unexpected_action_acknowledgement"
        | "unmapped_input"
        | "empty_action_list"
        | "no_runtime_assignment" => EventLevel::Warning,
        _ => EventLevel::Error,
    }
}

fn clear_device_state(
    capabilities: &RwLock<Option<DeviceCapabilities>>,
    learning: &RwLock<Option<LearningSession>>,
    controls: &Mutex<VecDeque<String>>,
) {
    *capabilities
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    *learning
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    controls
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}

fn update_action_deadline(
    deadline: &mut Option<Instant>,
    session: &DeviceSession,
    sent_action: bool,
) {
    if sent_action {
        *deadline = Some(Instant::now() + ACTION_ACK_TIMEOUT);
    } else if !session.is_awaiting_action() {
        *deadline = None;
    }
}

fn emit_serial_write_error(
    app: &AppHandle,
    connection: &RwLock<ConnectionStatus>,
    error: std::io::Error,
) {
    emit_activity(
        app,
        connection,
        EventLevel::Error,
        RuntimeActivity::new("serial_write_failed").with_detail(error.to_string()),
    );
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut child = clipboard_command()
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("start clipboard command: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "open clipboard command stdin".to_owned())?
        .write_all(text.as_bytes())
        .map_err(|error| format!("write clipboard command: {error}"))?;
    let status = child
        .wait()
        .map_err(|error| format!("wait for clipboard command: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("clipboard command exited {status}"))
    }
}

fn clipboard_command() -> Command {
    let command = Command::new(CLIPBOARD_COMMAND);
    #[cfg(target_os = "macos")]
    let command = {
        let mut command = command;
        command.env("LC_CTYPE", "UTF-8");
        command
    };
    command
}

fn set_connection(
    app: &AppHandle,
    connection: &RwLock<ConnectionStatus>,
    next: ConnectionStatus,
    code: Option<&str>,
) {
    let changed = {
        let mut current = connection
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *current == next {
            false
        } else {
            *current = next.clone();
            true
        }
    };
    if changed {
        #[cfg(target_os = "macos")]
        crate::tray::update_connection(app, &next);
        emit_activity(
            app,
            connection,
            EventLevel::Info,
            RuntimeActivity::new(code.unwrap_or("connection_changed")),
        );
    }
}

fn emit_activity(
    app: &AppHandle,
    connection: &RwLock<ConnectionStatus>,
    level: EventLevel,
    activity: RuntimeActivity,
) {
    emit_activity_with_home_update(app, connection, level, activity, None);
}

fn emit_activity_with_home_update(
    app: &AppHandle,
    connection: &RwLock<ConnectionStatus>,
    level: EventLevel,
    activity: RuntimeActivity,
    home_update: Option<HomeMetricsSnapshot>,
) {
    let payload = RuntimeEvent {
        timestamp_ms: now_ms(),
        level,
        connection: connection
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone(),
        home_update,
        activity,
    };
    let _ = app.emit("runtime-event", payload);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn wait(stop: &AtomicBool) {
    for _ in 0..10 {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
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
    fn invalid_hello_lines_clear_capabilities_and_ready_session() {
        let mut session = DeviceSession::new(runtime_model());
        let capabilities = RwLock::new(None);
        let DeviceMessage::Hello(hello) = hello() else {
            unreachable!();
        };
        let mut copy = |_: &str| -> Result<(), String> { Ok(()) };

        session.on_message(DeviceMessage::Hello(hello.clone()), &mut copy);
        session.on_message(DeviceMessage::ConfigOk { revision: 1 }, &mut copy);
        *capabilities.write().unwrap() = Some(hello.clone());
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

        update_published_capabilities(&capabilities, session.candidate_board, "HELLO 2 esp32s3");
        session.on_line("HELLO 2 esp32s3", &mut copy);
        assert!(capabilities.read().unwrap().is_none());
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
        *capabilities.write().unwrap() = Some(hello.clone());
        assert!(session.ready);

        update_published_capabilities(
            &capabilities,
            session.candidate_board,
            "HELLO 3 esp32s3 luatos-esp32s3-aio build 2 0",
        );
        session.on_line("HELLO 3 esp32s3 luatos-esp32s3-aio build 2 0", &mut copy);
        assert!(capabilities.read().unwrap().is_none());
        assert!(!session.ready);
        assert!(session.hello.is_none());

        session.on_message(DeviceMessage::Hello(hello.clone()), &mut copy);
        session.on_message(DeviceMessage::ConfigOk { revision: 3 }, &mut copy);
        *capabilities.write().unwrap() = Some(hello);
        update_published_capabilities(
            &capabilities,
            session.candidate_board,
            "HELLO 3 esp32s3 vccgnd-yd-rp2040 build 2 0 6",
        );
        session.on_line("HELLO 3 esp32s3 vccgnd-yd-rp2040 build 2 0 6", &mut copy);
        assert!(capabilities.read().unwrap().is_none());
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

    #[test]
    fn runtime_events_serialize_input_state_or_null() {
        let event = RuntimeEvent {
            timestamp_ms: 1,
            level: EventLevel::Info,
            connection: ConnectionStatus::searching(),
            home_update: None,
            activity: RuntimeActivity::new("device_searching"),
        };
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["code"], "device_searching");
        assert!(value["input"].is_null());
        assert!(value["pressed"].is_null());

        let down = RuntimeEvent {
            activity: RuntimeActivity {
                input: Some(PhysicalInput::Direct { gpio: 6 }),
                pressed: Some(true),
                ..RuntimeActivity::new("input_state")
            },
            ..event.clone()
        };
        let value = serde_json::to_value(down).unwrap();
        assert_eq!(value["pressed"], true);
        assert_eq!(value["input"]["gpio"], 6);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn clipboard_command_uses_utf8_locale() {
        let command = clipboard_command();

        assert!(command.get_envs().any(|(key, value)| {
            key == "LC_CTYPE" && value == Some(std::ffi::OsStr::new("UTF-8"))
        }));
    }
}
