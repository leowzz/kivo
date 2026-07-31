#[cfg(test)]
use crate::hardware::board_by_runtime_usb;
use crate::{
    coordinator::{
        CapturedInput, DeviceWorker, RuntimeEventContext, WorkerCommand, WorkerEvent,
        WorkerLauncher, WorkerStart,
    },
    hardware::{BoardProfile, DeviceId},
    metrics::{HomeMetricsSnapshot, MetricAttribution, MetricsStore},
    paste::{Clock, PasteHandle, PasteReply, PasteRequest, SystemClock},
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
    io::{BufRead, BufReader, ErrorKind, Read, Write},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
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
    pub learning_target: Option<LearningTarget>,
    #[serde(skip)]
    context: Option<RuntimeEventContext>,
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
            learning_target: None,
            context: None,
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

    fn with_context(mut self, context: Option<RuntimeEventContext>) -> Self {
        self.context = context;
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
    context: Option<RuntimeEventContext>,
}

pub struct DeviceSession {
    profile: Option<Arc<RuntimeProfileSnapshot>>,
    candidate_board: &'static BoardProfile,
    hello: Option<HelloCapabilities>,
    revision: u32,
    configuring: Option<u32>,
    ready: bool,
    active: Option<ActionSequence>,
    active_snapshot: Option<Arc<RuntimeProfileSnapshot>>,
    active_context: Option<RuntimeEventContext>,
    queue: VecDeque<QueuedInput>,
    active_receive_sequence: Option<u64>,
    pending_paste: Option<PendingPaste>,
    pending_reconfiguration: Option<PendingReconfiguration>,
    pending_learning: Option<LearningTarget>,
    learning: Option<ActiveLearning>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeProfileSnapshot {
    pub profile: DeviceProfile,
    pub hardware_profile_id: String,
    pub metric_attribution: MetricAttribution,
}

#[derive(Clone, Debug)]
struct QueuedInput {
    receive_sequence: u64,
    event_id: u64,
    input: PhysicalInput,
    snapshot: Arc<RuntimeProfileSnapshot>,
    context: Option<RuntimeEventContext>,
}

struct PendingReconfiguration {
    snapshot: Option<Arc<RuntimeProfileSnapshot>>,
    revision: u32,
}

struct ActiveLearning {
    target: LearningTarget,
    acknowledged: bool,
}

impl DeviceSession {
    #[cfg(test)]
    pub fn new(profile: RuntimeProfileSnapshot) -> Self {
        Self {
            profile: Some(Arc::new(profile)),
            candidate_board: crate::hardware::board_by_id(
                crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID,
            )
            .unwrap(),
            hello: None,
            revision: 0,
            configuring: None,
            ready: false,
            active: None,
            active_snapshot: None,
            active_context: None,
            queue: VecDeque::new(),
            active_receive_sequence: None,
            pending_paste: None,
            pending_reconfiguration: None,
            pending_learning: None,
            learning: None,
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
            active_snapshot: None,
            active_context: None,
            queue: VecDeque::new(),
            active_receive_sequence: None,
            pending_paste: None,
            pending_reconfiguration: None,
            pending_learning: None,
            learning: None,
        }
    }

    pub fn update_snapshot(
        &mut self,
        snapshot: Option<Arc<RuntimeProfileSnapshot>>,
    ) -> SessionOutput {
        self.profile = snapshot.clone();
        if let Some(pending) = self.pending_reconfiguration.as_mut() {
            pending.snapshot = snapshot;
        }
        SessionOutput::default()
    }

    pub fn reconfigure(
        &mut self,
        snapshot: Option<Arc<RuntimeProfileSnapshot>>,
        revision: u32,
    ) -> SessionOutput {
        let mut output = SessionOutput::default();
        self.end_active_learning(&mut output);
        self.pending_learning = None;
        self.settle_queued(&mut output);
        self.ready = false;
        self.configuring = None;
        self.profile = snapshot.clone();
        self.pending_reconfiguration = Some(PendingReconfiguration {
            snapshot,
            revision: revision.max(1),
        });
        self.start_pending_control(&mut output);
        output
    }

    pub fn begin_learning(&mut self, target: LearningTarget) -> SessionOutput {
        let mut output = SessionOutput::default();
        if self.learning.is_some() || self.pending_learning.is_some() {
            output
                .activities
                .push(RuntimeActivity::new("learning_session_active"));
            return output;
        }
        let pins = target.pins.iter().copied().collect::<BTreeSet<_>>();
        let reported = self
            .hello
            .as_ref()
            .map(|hello| hello.pins.iter().copied().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        if target.firmware_revision == 0
            || target.device_id.board_profile_id() != self.candidate_board.id
            || target.pins.is_empty()
            || pins.len() != target.pins.len()
            || !pins.is_subset(&reported)
            || !pins
                .iter()
                .all(|pin| self.candidate_board.safe_pins.contains(pin))
        {
            output
                .activities
                .push(RuntimeActivity::new("invalid_learning_target"));
            return output;
        }
        self.pending_reconfiguration = None;
        self.configuring = None;
        self.ready = false;
        self.settle_queued(&mut output);
        self.pending_learning = Some(target);
        self.start_pending_control(&mut output);
        output
    }

    pub fn end_learning(
        &mut self,
        restore: Option<Arc<RuntimeProfileSnapshot>>,
        revision: u32,
    ) -> SessionOutput {
        let mut output = SessionOutput::default();
        self.end_active_learning(&mut output);
        self.pending_learning = None;
        let restored = self.reconfigure(restore, revision);
        merge_output(&mut output, restored);
        output
    }

    pub fn is_awaiting_action(&self) -> bool {
        self.active_snapshot.is_some()
            && self.active.as_ref().is_some_and(ActionSequence::is_waiting)
    }

    pub(crate) fn runtime_profile_for_input(&self) -> Option<Arc<RuntimeProfileSnapshot>> {
        (self.ready
            && self.pending_reconfiguration.is_none()
            && self.pending_learning.is_none()
            && self.learning.is_none())
        .then(|| self.profile.clone())
        .flatten()
    }

    pub fn fail_active_deferred(&mut self, code: &str, detail: Option<String>) -> SessionOutput {
        let mut output = SessionOutput::default();
        if let Some(mut sequence) = self.active.take() {
            let event_id = sequence.event_id();
            sequence.abort();
            output.lines.push(format!("SKIP {event_id}\n"));
            let mut activity = RuntimeActivity::new(code);
            activity.detail = detail;
            activity.context = self.active_context.clone();
            output.activities.push(activity);
            self.pending_paste = None;
            self.active_snapshot = None;
            self.active_context = None;
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
                self.start_next(&mut output);
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
                let snapshot = self.profile.clone();
                let action_snapshot = self.runtime_profile_for_input();
                self.handle_input(
                    event_id,
                    input,
                    state,
                    receive_sequence,
                    occurred_at_ms,
                    snapshot,
                    action_snapshot,
                    None,
                    &mut output,
                );
            }
            DeviceMessage::Done { event_id, step } => self.handle_done(event_id, step, &mut output),
            DeviceMessage::LearnOk { revision }
                if self
                    .learning
                    .as_ref()
                    .is_some_and(|learning| learning.target.firmware_revision == revision) =>
            {
                let learning = self.learning.as_mut().expect("matching learning exists");
                learning.acknowledged = true;
                output.activities.push(RuntimeActivity {
                    learning_target: Some(learning.target.clone()),
                    ..RuntimeActivity::new("learning_ready")
                        .with_param("revision", revision.to_string())
                });
            }
            DeviceMessage::LearnDirect { gpio, state } => {
                if let Some(learning) = self.learning.as_ref().filter(|learning| {
                    learning.acknowledged && learning.target.pins.contains(&gpio)
                }) {
                    output.activities.push(RuntimeActivity {
                        input: Some(PhysicalInput::Direct { gpio }),
                        pressed: Some(state == InputState::Down),
                        learning_target: Some(learning.target.clone()),
                        ..RuntimeActivity::new("learning_input")
                    });
                }
            }
            DeviceMessage::LearnContact {
                pin_a,
                pin_b,
                state,
            } => {
                if let Some(learning) = self.learning.as_ref().filter(|learning| {
                    learning.acknowledged
                        && learning.target.pins.contains(&pin_a)
                        && learning.target.pins.contains(&pin_b)
                }) {
                    output.activities.push(RuntimeActivity {
                        input: Some(PhysicalInput::Contact {
                            source: 0,
                            pin_a,
                            pin_b,
                        }),
                        pressed: Some(state == InputState::Down),
                        learning_target: Some(learning.target.clone()),
                        ..RuntimeActivity::new("learning_input")
                    });
                }
            }
            DeviceMessage::ConfigOk { .. }
            | DeviceMessage::ConfigError { .. }
            | DeviceMessage::LearnOk { .. } => {}
        }
        output
    }

    pub(crate) fn capture_input(
        &self,
        current_context: &RuntimeEventContext,
        received_at_ms: u64,
        event_id: u64,
        input: PhysicalInput,
        state: InputState,
    ) -> CapturedInput {
        CapturedInput {
            context: current_context.with_timestamp(received_at_ms),
            runtime_profile: self.runtime_profile_for_input(),
            event_id,
            input,
            state,
        }
    }

    pub(crate) fn on_captured_input(
        &mut self,
        captured: &CapturedInput,
        receive_sequence: u64,
    ) -> SessionOutput {
        let mut output = SessionOutput::default();
        let snapshot = captured.runtime_profile.clone();
        self.handle_input(
            captured.event_id,
            captured.input,
            captured.state,
            receive_sequence,
            captured.context.timestamp_ms,
            snapshot.clone(),
            snapshot,
            Some(captured.context.clone()),
            &mut output,
        );
        output
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_input(
        &mut self,
        event_id: u64,
        input: PhysicalInput,
        state: InputState,
        receive_sequence: u64,
        occurred_at_ms: u64,
        metric_snapshot: Option<Arc<RuntimeProfileSnapshot>>,
        action_snapshot: Option<Arc<RuntimeProfileSnapshot>>,
        context: Option<RuntimeEventContext>,
        output: &mut SessionOutput,
    ) {
        let metric_press = (state == InputState::Down)
            .then_some(metric_snapshot.as_ref())
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
            context: context.clone(),
            metric_press,
            ..RuntimeActivity::new("input_state")
        });
        if state == InputState::Down {
            if let Some(snapshot) = action_snapshot {
                self.queue.push_back(QueuedInput {
                    receive_sequence,
                    event_id,
                    input,
                    snapshot,
                    context: context.clone(),
                });
                self.start_next(output);
            } else {
                output.lines.push(format!("SKIP {event_id}\n"));
                output.completed_receive_sequences.push(receive_sequence);
                output
                    .activities
                    .push(RuntimeActivity::new("input_before_configuration").with_context(context));
            }
        } else {
            output.completed_receive_sequences.push(receive_sequence);
        }
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
        let snapshot = self.profile.clone();
        self.clear_handshake();
        if let Err(error) = validate_hello(self.candidate_board, &hello) {
            output.activities.push(activity_from_error(error));
            return;
        }
        self.hello = Some(hello);
        let revision = self.revision.wrapping_add(1).max(1);
        self.configure_snapshot(snapshot, revision, output);
    }

    fn configure_snapshot(
        &mut self,
        snapshot: Option<Arc<RuntimeProfileSnapshot>>,
        revision: u32,
        output: &mut SessionOutput,
    ) {
        self.profile = snapshot;
        self.ready = false;
        self.configuring = None;
        let Some(hello) = self.hello.as_ref() else {
            return;
        };
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
        self.revision = revision.max(1);
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
        self.active_snapshot = None;
        self.active_context = None;
        self.active_receive_sequence = None;
        self.pending_paste = None;
        self.queue.clear();
        self.pending_reconfiguration = None;
        self.pending_learning = None;
        self.learning = None;
    }

    fn settle_queued(&mut self, output: &mut SessionOutput) {
        for queued in self.queue.drain(..) {
            output.lines.push(format!("SKIP {}\n", queued.event_id));
            output
                .completed_receive_sequences
                .push(queued.receive_sequence);
        }
    }

    fn end_active_learning(&mut self, output: &mut SessionOutput) {
        if let Some(learning) = self.learning.take() {
            output
                .lines
                .push(format!("LEARN_END {}\n", learning.target.firmware_revision));
        }
    }

    fn start_pending_control(&mut self, output: &mut SessionOutput) {
        if self.active.is_some() {
            return;
        }
        if let Some(target) = self.pending_learning.take() {
            self.revision = target.firmware_revision;
            let pins = target
                .pins
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            output.lines.push(format!(
                "LEARN_BEGIN {} {} {}\n",
                target.firmware_revision,
                target.pins.len(),
                pins
            ));
            self.learning = Some(ActiveLearning {
                target,
                acknowledged: false,
            });
            return;
        }
        if let Some(pending) = self.pending_reconfiguration.take() {
            self.configure_snapshot(pending.snapshot, pending.revision, output);
        }
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
            output.activities.push(
                RuntimeActivity::new("invalid_action_acknowledgement")
                    .with_detail(error)
                    .with_context(self.active_context.clone()),
            );
            self.active = None;
            self.active_snapshot = None;
            self.active_context = None;
            self.pending_paste = None;
            if let Some(receive_sequence) = self.active_receive_sequence.take() {
                output.completed_receive_sequences.push(receive_sequence);
            }
            self.start_next(output);
            return;
        }
        if sequence.is_complete() {
            self.active = None;
            self.active_snapshot = None;
            self.active_context = None;
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
            self.start_pending_control(output);
            if self.pending_reconfiguration.is_some()
                || self.pending_learning.is_some()
                || self.learning.is_some()
                || self.configuring.is_some()
            {
                return;
            }
            let Some(queued) = self.queue.pop_front() else {
                return;
            };
            let runtime = &queued.snapshot;
            let Some(button) = runtime
                .profile
                .button_for(&runtime.hardware_profile_id, &queued.input)
                .map(str::to_owned)
            else {
                output.lines.push(format!("SKIP {}\n", queued.event_id));
                output
                    .completed_receive_sequences
                    .push(queued.receive_sequence);
                output
                    .activities
                    .push(RuntimeActivity::new("unmapped_input").with_context(queued.context));
                continue;
            };
            let actions = runtime
                .profile
                .actions
                .get(&button)
                .cloned()
                .unwrap_or_default();
            if actions.is_empty() {
                output.lines.push(format!("SKIP {}\n", queued.event_id));
                output
                    .completed_receive_sequences
                    .push(queued.receive_sequence);
                output.activities.push(
                    RuntimeActivity::new("empty_action_list")
                        .with_param("button", button)
                        .with_context(queued.context),
                );
                continue;
            }
            self.active = Some(ActionSequence::new(queued.event_id, button, actions));
            self.active_snapshot = Some(queued.snapshot);
            self.active_context = queued.context;
            self.active_receive_sequence = Some(queued.receive_sequence);
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
                    context: self.active_context.clone(),
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
                            .with_detail(error)
                            .with_context(self.active_context.clone()),
                    );
                    self.active = None;
                    self.active_snapshot = None;
                    self.active_context = None;
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
            output.activities.push(
                RuntimeActivity::new("paste_grant_mismatch")
                    .with_context(self.pending_paste.as_ref().unwrap().context.clone()),
            );
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
#[serde(rename_all = "camelCase")]
pub struct LearningTarget {
    pub device_id: DeviceId,
    pub device_profile_id: String,
    pub hardware_profile_id: String,
    pub editing_revision: u64,
    pub firmware_revision: u32,
    pub pins: Vec<u8>,
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
    transport_factory: Arc<dyn SerialTransportFactory>,
    clock: Arc<dyn Clock>,
}

impl SystemWorkerLauncher {
    pub fn new(
        paste: PasteHandle,
        metrics: Option<Arc<MetricsStore>>,
        operation_barrier: Arc<RwLock<()>>,
    ) -> Self {
        Self::with_runtime(
            paste,
            metrics,
            operation_barrier,
            Arc::new(SystemSerialTransportFactory),
            Arc::new(SystemClock::default()),
        )
    }

    pub fn with_runtime(
        paste: PasteHandle,
        metrics: Option<Arc<MetricsStore>>,
        operation_barrier: Arc<RwLock<()>>,
        transport_factory: Arc<dyn SerialTransportFactory>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            paste,
            metrics,
            operation_barrier,
            transport_factory,
            clock,
        }
    }
}

pub trait SerialTransport: Read + Write + Send {
    fn prepare(&mut self) -> Result<(), String>;
}

pub trait SerialTransportFactory: Send + Sync {
    fn open(&self, port: &str) -> Result<Box<dyn SerialTransport>, String>;
}

struct SystemSerialTransportFactory;

impl SerialTransportFactory for SystemSerialTransportFactory {
    fn open(&self, port: &str) -> Result<Box<dyn SerialTransport>, String> {
        let port = serialport::new(port, 115_200)
            .timeout(Duration::from_millis(50))
            .open()
            .map_err(|error| format!("serial_open_failed: {error}"))?;
        Ok(Box::new(SystemSerialTransport(port)))
    }
}

struct SystemSerialTransport(Box<dyn serialport::SerialPort>);

impl Read for SystemSerialTransport {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buffer)
    }
}

impl Write for SystemSerialTransport {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl SerialTransport for SystemSerialTransport {
    fn prepare(&mut self) -> Result<(), String> {
        self.0
            .write_data_terminal_ready(true)
            .and_then(|()| self.0.write_request_to_send(true))
            .map_err(|error| format!("serial_handshake_failed: {error}"))
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

struct WorkerRuntime {
    paste: PasteHandle,
    metrics: Option<Arc<MetricsStore>>,
    operation_barrier: Arc<RwLock<()>>,
    transport_factory: Arc<dyn SerialTransportFactory>,
    clock: Arc<dyn Clock>,
}

pub(crate) fn apply_worker_context_update(
    current_port: &mut String,
    current_context: &mut RuntimeEventContext,
    command: &WorkerCommand,
) -> bool {
    let WorkerCommand::UpdatePort(port) = command else {
        return false;
    };
    *current_port = port.clone();
    current_context.port = Some(port.clone());
    true
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
        let runtime = WorkerRuntime {
            paste: self.paste.clone(),
            metrics: self.metrics.clone(),
            operation_barrier: Arc::clone(&self.operation_barrier),
            transport_factory: Arc::clone(&self.transport_factory),
            clock: Arc::clone(&self.clock),
        };
        let join = thread::Builder::new()
            .name(format!("kivo-device-{}", start.device_id.as_str()))
            .spawn(move || {
                run_isolated_worker(start, command_receiver, events, runtime, thread_stop)
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
    runtime: WorkerRuntime,
    stop: Arc<AtomicBool>,
) {
    let result = run_isolated_worker_inner(&start, &commands, &events, &runtime, &stop);
    let _ = runtime.paste.cancel_device(&start.device_id);
    if !stop.load(Ordering::Relaxed) {
        let _ = events.send(WorkerEvent::Disconnected {
            generation: start.generation,
            device_id: start.device_id,
            error: result.err(),
        });
    }
}

fn run_isolated_worker_inner(
    start: &WorkerStart,
    commands: &mpsc::Receiver<WorkerCommand>,
    events: &mpsc::Sender<WorkerEvent>,
    runtime: &WorkerRuntime,
    stop: &AtomicBool,
) -> Result<(), String> {
    let paste = &runtime.paste;
    let metrics = runtime.metrics.as_deref();
    let operation_barrier = runtime.operation_barrier.as_ref();
    let transport_factory = runtime.transport_factory.as_ref();
    let clock = runtime.clock.as_ref();
    let board = crate::hardware::board_by_id(&start.board_profile_id)
        .ok_or_else(|| "unknown_board_profile".to_owned())?;
    let mut port = transport_factory.open(&start.port)?;
    port.prepare()?;
    port.write_all(b"HELLO\n")
        .and_then(|()| port.flush())
        .map_err(|error| format!("serial_handshake_failed: {error}"))?;
    let mut device = BufReader::new(port);
    let hello = read_valid_hello(&mut device, board, clock, stop)?;
    events
        .send(WorkerEvent::HelloValidated {
            generation: start.generation,
            device_id: start.device_id.clone(),
            capabilities: hello.clone(),
        })
        .map_err(|_| "coordinator_stopped".to_owned())?;
    let mut session = DeviceSession::without_model(board);
    let mut pending_paste: Option<PendingPasteReply> = None;
    let mut active_paste_ack = None;
    let mut action_deadline = None;
    let mut line = Vec::new();
    let mut current_port = start.port.clone();
    let mut current_context =
        RuntimeEventContext::unassigned(clock.unix_time_ms()).with_port(current_port.clone());
    let initial = session.on_message_deferred(DeviceMessage::Hello(hello), 0, clock.unix_time_ms());
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
        &current_context,
        clock,
        stop,
    )?;

    while !stop.load(Ordering::Relaxed) {
        for command in commands.try_iter() {
            if apply_worker_context_update(&mut current_port, &mut current_context, &command) {
                continue;
            }
            let (output, context) = match command {
                WorkerCommand::UpdatePort(_) => unreachable!("port updates are handled above"),
                WorkerCommand::UpdateSnapshot(snapshot) => {
                    current_context = RuntimeEventContext::from_snapshot(
                        clock.unix_time_ms(),
                        snapshot.as_deref(),
                    )
                    .with_port(current_port.clone());
                    (session.update_snapshot(snapshot), current_context.clone())
                }
                WorkerCommand::Reconfigure { snapshot, revision } => {
                    current_context = RuntimeEventContext::from_snapshot(
                        clock.unix_time_ms(),
                        snapshot.as_deref(),
                    )
                    .with_port(current_port.clone());
                    (
                        session.reconfigure(snapshot, revision),
                        current_context.clone(),
                    )
                }
                WorkerCommand::BeginLearning(target) => {
                    current_context =
                        RuntimeEventContext::from_learning(clock.unix_time_ms(), &target)
                            .with_port(current_port.clone());
                    (session.begin_learning(target), current_context.clone())
                }
                WorkerCommand::EndLearning { snapshot, revision } => {
                    current_context = RuntimeEventContext::from_snapshot(
                        clock.unix_time_ms(),
                        snapshot.as_deref(),
                    )
                    .with_port(current_port.clone());
                    (
                        session.end_learning(snapshot, revision),
                        current_context.clone(),
                    )
                }
                WorkerCommand::Input {
                    receive_sequence,
                    captured,
                } => (
                    session.on_captured_input(&captured, receive_sequence),
                    captured.context,
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
                &context,
                clock,
                stop,
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
                        &current_context.with_timestamp(clock.unix_time_ms()),
                        clock,
                        stop,
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
                        &current_context.with_timestamp(clock.unix_time_ms()),
                        clock,
                        stop,
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
                        &current_context.with_timestamp(clock.unix_time_ms()),
                        clock,
                        stop,
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
                        &current_context.with_timestamp(clock.unix_time_ms()),
                        clock,
                        stop,
                    )?;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("paste_coordinator_stopped".into());
                }
            }
        }

        if action_deadline.is_some_and(|deadline| clock.monotonic_now() >= deadline)
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
                &current_context.with_timestamp(clock.unix_time_ms()),
                clock,
                stop,
            )?;
        }
        if !session.is_awaiting_action() && pending_paste.is_none() {
            action_deadline = None;
        }

        line.clear();
        match device.read_until(b'\n', &mut line) {
            Ok(0) => return Err("device_disconnected".into()),
            Ok(_) => {
                let received_at_ms = clock.unix_time_ms();
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
                        let captured = session.capture_input(
                            &current_context,
                            received_at_ms,
                            event_id,
                            input,
                            state,
                        );
                        events
                            .send(WorkerEvent::Input {
                                generation: start.generation,
                                device_id: start.device_id.clone(),
                                captured,
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
                            &current_context.with_timestamp(received_at_ms),
                            clock,
                            stop,
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
                            &current_context.with_timestamp(received_at_ms),
                            clock,
                            stop,
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
                            &current_context.with_timestamp(received_at_ms),
                            clock,
                            stop,
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
    clock: &dyn Clock,
    stop: &AtomicBool,
) -> Result<HelloCapabilities, String> {
    let deadline = clock.monotonic_now() + ACTION_ACK_TIMEOUT;
    let mut line = Vec::new();
    while !stop.load(Ordering::Relaxed) && clock.monotonic_now() < deadline {
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
    context: &RuntimeEventContext,
    clock: &dyn Clock,
    stop: &AtomicBool,
) -> Result<(), String> {
    emit_worker_activities(
        start,
        events,
        metrics,
        operation_barrier,
        &mut output.activities,
        context,
        clock,
        stop,
    )?;
    let completed_sequence = !output.completed_receive_sequences.is_empty();
    for receive_sequence in output.completed_receive_sequences.drain(..) {
        events
            .send(WorkerEvent::SequenceFinished {
                generation: start.generation,
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
        *action_deadline = Some(clock.monotonic_now() + ACTION_ACK_TIMEOUT);
    } else if completed_sequence {
        *action_deadline = None;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_worker_activities(
    start: &WorkerStart,
    events: &mpsc::Sender<WorkerEvent>,
    metrics: Option<&MetricsStore>,
    operation_barrier: &RwLock<()>,
    activities: &mut Vec<RuntimeActivity>,
    context: &RuntimeEventContext,
    clock: &dyn Clock,
    stop: &AtomicBool,
) -> Result<(), String> {
    for activity in activities.drain(..) {
        let event_context = activity.context.clone().unwrap_or_else(|| context.clone());
        if let Some(metrics) = metrics {
            let _operation = operation_barrier
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            persist_metrics(metrics, &activity, clock.unix_time_ms())
                .map_err(|error| format!("metrics_write_failed: {error}"))?;
        }
        events
            .send(WorkerEvent::Activity {
                generation: start.generation,
                device_id: start.device_id.clone(),
                context: event_context,
                activity,
            })
            .map_err(|_| "coordinator_stopped".to_owned())?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn emit_worker_activities_for_test(
    start: &WorkerStart,
    events: &mpsc::Sender<WorkerEvent>,
    mut output: SessionOutput,
    context: &RuntimeEventContext,
) {
    emit_worker_activities(
        start,
        events,
        None,
        &RwLock::new(()),
        &mut output.activities,
        context,
        &SystemClock::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
}

#[cfg(test)]
fn now_ms() -> u64 {
    SystemClock::default().unix_time_ms()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        hardware::DeviceId,
        metrics::{MetricAttribution, MetricsStore},
        model::{ButtonDefinition, ButtonGroup, ModelLayout},
        paste::{ClipboardWriter, PasteCoordinator},
        profile::{
            ButtonAction, DeviceProfile, HardwareProfile, InputSource, PROFILE_SCHEMA_VERSION,
        },
        protocol::{DeviceMessage, PhysicalInput},
    };
    use serialport::{SerialPortInfo, SerialPortType, UsbPortInfo};
    use std::{
        cell::RefCell,
        collections::BTreeMap,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn runtime_model() -> RuntimeProfileSnapshot {
        RuntimeProfileSnapshot {
            hardware_profile_id: "esp-primary".into(),
            metric_attribution: MetricAttribution {
                device_id: DeviceId::new(
                    crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID,
                    "ABCDEF123456",
                )
                .unwrap(),
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
                    board_profile_id: crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID.into(),
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
            controller_family_id: crate::hardware::ESP32S3_FAMILY_ID.into(),
            board_profile_id: crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID.into(),
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
                context: None,
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

    struct FailingClipboard;

    impl ClipboardWriter for FailingClipboard {
        fn write(&self, _text: &str) -> Result<(), String> {
            Err("clipboard unavailable".into())
        }
    }

    #[test]
    fn deferred_paste_failure_keeps_captured_context_after_overtaking_reconfigure() {
        let old = Arc::new(runtime_model());
        let device_id = old.metric_attribution.device_id.clone();
        let old_context = RuntimeEventContext::from_snapshot(10, Some(old.as_ref()))
            .with_port("/dev/old-captured");
        let mut session = DeviceSession::new((*old).clone());
        let DeviceMessage::Hello(hello) = hello() else {
            unreachable!();
        };
        session.on_message_deferred(DeviceMessage::Hello(hello), 0, 1);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 2);
        let captured = session.capture_input(
            &old_context,
            10,
            70,
            PhysicalInput::Direct { gpio: 6 },
            InputState::Down,
        );

        let mut new = runtime_model();
        new.profile.profile.id = "console".into();
        new.profile.profile.name = "Console".into();
        new.profile.hardware_profiles[0].id = "esp-new".into();
        new.hardware_profile_id = "esp-new".into();
        new.metric_attribution.device_profile_id = "console".into();
        new.metric_attribution.hardware_profile_id = "esp-new".into();
        let new = Arc::new(new);
        let new_context = RuntimeEventContext::from_snapshot(20, Some(new.as_ref()))
            .with_port("/dev/new-current");
        session.reconfigure(Some(new), 2);
        let queued = session.on_captured_input(&captured, 77);
        assert!(queued.paste_requests.is_empty());
        let configured =
            session.on_message_deferred(DeviceMessage::ConfigOk { revision: 2 }, 0, 21);
        assert_eq!(configured.paste_requests[0].text, "第一步");

        let paste = PasteCoordinator::with_timeout(FailingClipboard, Duration::from_millis(100));
        paste.handle().register_sequence(77).unwrap();
        let start = WorkerStart {
            generation: 9,
            device_id: device_id.clone(),
            port: "/dev/new-current".into(),
            board_profile_id: device_id.board_profile_id().into(),
        };
        let (events, received_events) = mpsc::channel();
        let mut writer = Vec::new();
        let mut pending_paste = None;
        let mut action_deadline = None;
        let stop = AtomicBool::new(false);
        let barrier = RwLock::new(());
        write_isolated_output(
            &start,
            &events,
            &paste.handle(),
            None,
            &barrier,
            &mut writer,
            configured,
            &mut pending_paste,
            &mut action_deadline,
            &new_context,
            &SystemClock::default(),
            &stop,
        )
        .unwrap();
        let reply = pending_paste
            .as_ref()
            .unwrap()
            .replies
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        assert_eq!(
            reply,
            PasteReply::ClipboardError("clipboard unavailable".into())
        );
        pending_paste = None;
        let failed = session
            .fail_active_deferred("action_step_failed", Some("clipboard unavailable".into()));
        write_isolated_output(
            &start,
            &events,
            &paste.handle(),
            None,
            &barrier,
            &mut writer,
            failed,
            &mut pending_paste,
            &mut action_deadline,
            &new_context,
            &SystemClock::default(),
            &stop,
        )
        .unwrap();
        let event = received_events
            .try_iter()
            .find(|event| {
                matches!(
                    event,
                    WorkerEvent::Activity { activity, .. }
                        if activity.code == "action_step_failed"
                )
            })
            .unwrap();
        let WorkerEvent::Activity {
            device_id: actual_device,
            context,
            ..
        } = event
        else {
            unreachable!();
        };

        assert_eq!(actual_device, device_id);
        assert_eq!(context.device_profile_id.as_deref(), Some("phone"));
        assert_eq!(context.hardware_profile_id.as_deref(), Some("esp-primary"));
        assert_eq!(context.port.as_deref(), Some("/dev/old-captured"));
        paste.shutdown();
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
    fn live_update_keeps_the_in_flight_action_snapshot_and_uses_the_new_one_next() {
        let mut session = DeviceSession::new(runtime_model());
        session.ready = true;
        let first = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 50,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            50,
            100,
        );
        assert_eq!(first.paste_requests[0].text, "第一步");

        let mut updated = runtime_model();
        updated.profile.actions.insert(
            "A".into(),
            vec![ButtonAction::Paste {
                text: "新动作".into(),
            }],
        );
        let swapped = session.update_snapshot(Some(Arc::new(updated)));
        assert!(swapped.lines.is_empty());

        session.grant_paste(50, 1);
        let second = session.on_message_deferred(
            DeviceMessage::Done {
                event_id: 50,
                step: 1,
            },
            0,
            101,
        );
        assert_eq!(second.paste_requests[0].text, "第二步");
        session.grant_paste(50, 2);
        session.on_message_deferred(
            DeviceMessage::Done {
                event_id: 50,
                step: 2,
            },
            0,
            102,
        );

        let next = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 51,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            51,
            103,
        );
        assert_eq!(next.paste_requests[0].text, "新动作");
    }

    #[test]
    fn live_update_reconfiguration_settles_the_current_action_and_ignores_stale_config_ok() {
        let mut session = DeviceSession::new(runtime_model());
        let DeviceMessage::Hello(hello) = hello() else {
            unreachable!();
        };
        session.on_message_deferred(DeviceMessage::Hello(hello), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);
        let active = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 60,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            60,
            102,
        );
        assert_eq!(active.paste_requests[0].text, "第一步");

        let mut updated = runtime_model();
        updated.profile.hardware_profiles[0].debounce_ms = 45;
        let pending = session.reconfigure(Some(Arc::new(updated)), 2);
        assert!(pending.lines.is_empty());
        let rejected = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 61,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            61,
            103,
        );
        assert_eq!(rejected.lines, ["SKIP 61\n"]);

        session.grant_paste(60, 1);
        let old_second = session.on_message_deferred(
            DeviceMessage::Done {
                event_id: 60,
                step: 1,
            },
            0,
            104,
        );
        assert_eq!(old_second.paste_requests[0].text, "第二步");
        session.grant_paste(60, 2);
        let configured = session.on_message_deferred(
            DeviceMessage::Done {
                event_id: 60,
                step: 2,
            },
            0,
            105,
        );
        assert_eq!(configured.lines[0], "CONFIG_BEGIN 2 45\n");

        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 106);
        assert!(!session.ready);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 2 }, 0, 107);
        assert!(session.ready);
    }

    #[test]
    fn learning_emits_only_matching_revision_captures_with_the_complete_target() {
        let mut session = DeviceSession::new(runtime_model());
        let DeviceMessage::Hello(hello) = hello() else {
            unreachable!();
        };
        session.on_message_deferred(DeviceMessage::Hello(hello), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);
        let target = LearningTarget {
            device_id: DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "ABCDEF123456")
                .unwrap(),
            device_profile_id: "phone".into(),
            hardware_profile_id: "esp-primary".into(),
            editing_revision: 9,
            firmware_revision: 2,
            pins: vec![6, 7],
        };

        let begin = session.begin_learning(target.clone());
        assert_eq!(begin.lines, ["LEARN_BEGIN 2 2 6 7\n"]);
        assert!(
            session
                .on_message_deferred(DeviceMessage::LearnOk { revision: 1 }, 0, 102)
                .activities
                .is_empty()
        );
        assert!(
            session
                .on_message_deferred(
                    DeviceMessage::LearnDirect {
                        gpio: 6,
                        state: InputState::Down,
                    },
                    0,
                    103,
                )
                .activities
                .is_empty()
        );
        let ready = session.on_message_deferred(DeviceMessage::LearnOk { revision: 2 }, 0, 104);
        assert_eq!(ready.activities[0].learning_target.as_ref(), Some(&target));
        let capture = session.on_message_deferred(
            DeviceMessage::LearnDirect {
                gpio: 6,
                state: InputState::Down,
            },
            0,
            105,
        );
        assert_eq!(
            capture.activities[0].learning_target.as_ref(),
            Some(&target)
        );

        let restored = session.end_learning(Some(Arc::new(runtime_model())), 3);
        assert_eq!(restored.lines[0], "LEARN_END 2\n");
        assert_eq!(restored.lines[1], "CONFIG_BEGIN 3 30\n");
    }

    #[test]
    fn rejects_unsupported_hello_without_sending_topology() {
        let mut session = DeviceSession::new(runtime_model());
        let output = session.on_message(
            DeviceMessage::Hello(HelloCapabilities {
                protocol: 3,
                controller_family_id: crate::hardware::ESP32S3_FAMILY_ID.into(),
                board_profile_id: crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID.into(),
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
