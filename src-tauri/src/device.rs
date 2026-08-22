#[cfg(test)]
use crate::hardware::board_by_runtime_usb;
#[cfg(test)]
use crate::profile::{TriggerActions, TriggerSettings};
use crate::{
    coordinator::{
        CapturedInput, DeviceWorker, RuntimeEventContext, WorkerCommand, WorkerEvent,
        WorkerLauncher, WorkerRendererRegistry, WorkerStart,
    },
    display::{
        DisplayRenderer, DisplaySnapshot, RenderedScene, RendererRegistry, SH1106_PANEL_ID,
        SSD1306_PANEL_ID, SceneTracker, SceneUpdate, built_in_renderer_registry,
    },
    hardware::{BoardProfile, DeviceId},
    metrics::{HomeMetricsSnapshot, MetricAttribution, MetricsStore},
    paste::{Clock, PasteHandle, PasteReply, PasteRequest, SystemClock},
    product::{PRODUCT_DEFINITION_SCHEMA_VERSION, ProductDefinition, ProductDefinitionCache},
    profile::{ActionTrigger, DeviceProfile, InputSource, SwitchState},
    protocol::{
        ACTION_RUN_PROTOCOL_VERSION, ActionSequence, DISPLAY_LARGE_FONT_PROTOCOL_VERSION,
        DISPLAY_PROTOCOL_VERSION, DeviceMessage, HelloCapabilities, InputState,
        OLED_PROTOCOL_VERSION, PhysicalInput, ProductDefinitionTransfer, SH1106_PROTOCOL_VERSION,
        display_commands, format_paste_command, is_hello_line, parse_device, topology_commands,
        validate_hello,
    },
    trigger::{TriggerEdge, TriggerOccurrence, TriggerTracker},
};
use serde::Serialize;
#[cfg(test)]
use serialport::{SerialPortInfo, SerialPortType};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::{BufRead, BufReader, ErrorKind, Read, Write},
    path::Path,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
use std::process::Command;

const ACTION_ACK_TIMEOUT: Duration = Duration::from_millis(1800);
const DISPLAY_ACK_TIMEOUT: Duration = Duration::from_secs(2);
const PRODUCT_READ_TIMEOUT: Duration = Duration::from_secs(15);
const EMPTY_TOPOLOGY_DEBOUNCE_MS: u16 = 30;
pub(crate) const SERIAL_COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(10);
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
    #[serde(skip)]
    feature_disabled_log: Option<FeatureDisabledLog>,
    #[serde(skip)]
    action_result_log: Option<ActionResultLog>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MetricPress {
    attribution: MetricAttribution,
    button_id: String,
    occurred_at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FeatureDisabledLog {
    attribution: MetricAttribution,
    button_id: String,
    occurred_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActionResultLog {
    attribution: MetricAttribution,
    button_id: String,
    action_kind: String,
    succeeded: bool,
    occurred_at_ms: Option<u64>,
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
            feature_disabled_log: None,
            action_result_log: None,
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
    action_timeout: Option<Duration>,
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
    configuring: Option<ConfigurationInFlight>,
    ready: bool,
    active: Option<ActionSequence>,
    active_snapshot: Option<Arc<RuntimeProfileSnapshot>>,
    active_context: Option<RuntimeEventContext>,
    active_release_placeholder: Option<u64>,
    queue: VecDeque<QueuedOccurrence>,
    triggers: TriggerTracker,
    next_run_id: u64,
    pending_receive_sequences: BTreeMap<u64, usize>,
    gesture_placeholders: BTreeMap<u64, usize>,
    trigger_metadata: BTreeMap<(PhysicalInput, u64), TriggerMetadata>,
    feature_switch_states: BTreeMap<String, SwitchState>,
    active_receive_sequence: Option<u64>,
    pending_paste: Option<PendingPaste>,
    pending_reconfiguration: Option<PendingReconfiguration>,
    pending_learning: Option<LearningTarget>,
    learning: Option<ActiveLearning>,
    target_opener: Arc<dyn TargetOpener>,
}

trait TargetOpener: Send + Sync {
    fn open(&self, target: &str) -> Result<(), String>;
}

struct SystemTargetOpener;

impl TargetOpener for SystemTargetOpener {
    fn open(&self, target: &str) -> Result<(), String> {
        open_target(target)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeProfileSnapshot {
    pub profile: DeviceProfile,
    pub hardware_profile_id: String,
    pub metric_attribution: MetricAttribution,
}

fn format_switch_state(state: SwitchState) -> &'static str {
    match state {
        SwitchState::Open => "open",
        SwitchState::Closed => "closed",
    }
}

#[derive(Clone, Debug)]
struct TriggerMetadata {
    receive_sequence: u64,
    event_id: u64,
    placeholder_receive_sequence: Option<u64>,
}

#[derive(Clone, Debug)]
struct QueuedOccurrence {
    occurrence: TriggerOccurrence,
    receive_sequence: u64,
    event_id: u64,
    release_placeholder: Option<u64>,
}

struct PendingReconfiguration {
    snapshot: Option<Arc<RuntimeProfileSnapshot>>,
    revision: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigurationKind {
    Activate,
    Clear,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ConfigurationInFlight {
    revision: u32,
    kind: ConfigurationKind,
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
            candidate_board: crate::hardware::board_by_id(crate::hardware::YD_ESP32_S3_BOARD_ID)
                .unwrap(),
            hello: None,
            revision: 0,
            configuring: None,
            ready: false,
            active: None,
            active_snapshot: None,
            active_context: None,
            active_release_placeholder: None,
            queue: VecDeque::new(),
            triggers: TriggerTracker::default(),
            next_run_id: 1,
            pending_receive_sequences: BTreeMap::new(),
            gesture_placeholders: BTreeMap::new(),
            trigger_metadata: BTreeMap::new(),
            feature_switch_states: BTreeMap::new(),
            active_receive_sequence: None,
            pending_paste: None,
            pending_reconfiguration: None,
            pending_learning: None,
            learning: None,
            target_opener: Arc::new(SystemTargetOpener),
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
            active_release_placeholder: None,
            queue: VecDeque::new(),
            triggers: TriggerTracker::default(),
            next_run_id: 1,
            pending_receive_sequences: BTreeMap::new(),
            gesture_placeholders: BTreeMap::new(),
            trigger_metadata: BTreeMap::new(),
            feature_switch_states: BTreeMap::new(),
            active_receive_sequence: None,
            pending_paste: None,
            pending_reconfiguration: None,
            pending_learning: None,
            learning: None,
            target_opener: Arc::new(SystemTargetOpener),
        }
    }

    pub fn update_snapshot(
        &mut self,
        snapshot: Option<Arc<RuntimeProfileSnapshot>>,
    ) -> SessionOutput {
        let switch_ids = snapshot
            .as_deref()
            .and_then(|runtime| {
                runtime
                    .profile
                    .hardware_profile(&runtime.hardware_profile_id)
            })
            .into_iter()
            .flat_map(|hardware| &hardware.inputs)
            .filter_map(|source| match source {
                InputSource::FeatureSwitch { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        self.feature_switch_states
            .retain(|id, _| switch_ids.contains(id.as_str()));
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
        self.reset_gestures();
        self.settle_placeholders(&mut output);
        self.ready = false;
        self.configuring = None;
        self.feature_switch_states.clear();
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
        self.reset_gestures();
        self.settle_placeholders(&mut output);
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
            let run_id = sequence.run_id();
            let pending_step = sequence.awaiting_step();
            let detail = if code == "action_step_failed" && sequence.is_awaiting_paste() {
                Some("clipboard_write_failed".into())
            } else {
                detail
            };
            sequence.abort();
            output.lines.push(format!("SKIP {run_id}\n"));
            let mut activity = pending_step
                .as_ref()
                .map(|step| action_activity(code, step))
                .unwrap_or_else(|| RuntimeActivity::new(code));
            activity.detail = detail;
            activity.context = self.active_context.clone();
            if let Some(step) = pending_step.as_ref() {
                activity =
                    with_action_result_log(activity, self.active_snapshot.as_deref(), step, false);
            }
            output.activities.push(activity);
            self.pending_paste = None;
            self.active_snapshot = None;
            self.active_context = None;
            if let Some(receive_sequence) = self.active_receive_sequence.take() {
                let release_placeholder = self.active_release_placeholder.take();
                self.finish_queued_occurrence(receive_sequence, release_placeholder, &mut output);
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
            DeviceMessage::ConfigOk { revision }
                if self
                    .configuring
                    .as_ref()
                    .is_some_and(|configuring| configuring.revision == revision) =>
            {
                let configuring = self
                    .configuring
                    .take()
                    .expect("matching configuration exists");
                self.ready = configuring.kind == ConfigurationKind::Activate;
                let code = match configuring.kind {
                    ConfigurationKind::Activate => "topology_active",
                    ConfigurationKind::Clear => "topology_cleared",
                };
                output
                    .activities
                    .push(RuntimeActivity::new(code).with_param("revision", revision.to_string()));
                self.start_next(&mut output);
            }
            DeviceMessage::ConfigError { revision, code }
                if self
                    .configuring
                    .as_ref()
                    .is_some_and(|configuring| configuring.revision == revision) =>
            {
                self.configuring = None;
                self.ready = false;
                output.activities.push(
                    RuntimeActivity::new("topology_rejected")
                        .with_param("revision", revision.to_string())
                        .with_param("deviceCode", configuration_error_code(&code)),
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
                    occurred_at_ms,
                    snapshot,
                    action_snapshot,
                    None,
                    &mut output,
                );
            }
            DeviceMessage::Done { run_id, step } => self.handle_done(run_id, step, &mut output),
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
            | DeviceMessage::LearnOk { .. }
            | DeviceMessage::DisplayOk { .. }
            | DeviceMessage::DisplayResync { .. }
            | DeviceMessage::DisplayError { .. }
            | DeviceMessage::ProductInfo { .. }
            | DeviceMessage::ProductBegin { .. }
            | DeviceMessage::ProductChunk { .. }
            | DeviceMessage::ProductEnd { .. }
            | DeviceMessage::ProductError { .. } => {}
        }
        output
    }

    #[cfg(test)]
    pub(crate) fn capture_input(
        &self,
        current_context: &RuntimeEventContext,
        received_at_ms: u64,
        event_id: u64,
        input: PhysicalInput,
        state: InputState,
    ) -> CapturedInput {
        self.capture_input_with_monotonic(
            current_context,
            received_at_ms,
            received_at_ms,
            event_id,
            input,
            state,
        )
    }

    pub(crate) fn capture_input_with_monotonic(
        &self,
        current_context: &RuntimeEventContext,
        received_at_ms: u64,
        monotonic_ms: u64,
        event_id: u64,
        input: PhysicalInput,
        state: InputState,
    ) -> CapturedInput {
        CapturedInput {
            context: current_context.with_timestamp(received_at_ms),
            runtime_profile: self.runtime_profile_for_input(),
            monotonic_ms,
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
            captured.monotonic_ms,
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
        monotonic_ms: u64,
        metric_snapshot: Option<Arc<RuntimeProfileSnapshot>>,
        action_snapshot: Option<Arc<RuntimeProfileSnapshot>>,
        context: Option<RuntimeEventContext>,
        output: &mut SessionOutput,
    ) {
        let button = metric_snapshot.as_ref().and_then(|runtime| {
            runtime
                .profile
                .button_for(&runtime.hardware_profile_id, &input)
                .map(str::to_owned)
        });
        let feature_switch = metric_snapshot.as_ref().and_then(|runtime| {
            runtime
                .profile
                .feature_switch_for(&runtime.hardware_profile_id, &input)
                .and_then(|source| match source {
                    InputSource::FeatureSwitch { id, .. } => Some(id.clone()),
                    _ => None,
                })
        });
        let metric_press = if state == InputState::Down
            && button.as_deref().is_some_and(|button_id| {
                metric_snapshot.as_ref().is_some_and(|runtime| {
                    runtime.profile.button_is_enabled(
                        &runtime.hardware_profile_id,
                        button_id,
                        &self.feature_switch_states,
                    )
                })
            }) {
            metric_snapshot
                .as_ref()
                .zip(button.as_deref())
                .map(|(runtime, button_id)| MetricPress {
                    attribution: runtime.metric_attribution.clone(),
                    button_id: button_id.into(),
                    occurred_at_ms,
                })
        } else {
            None
        };
        let mut activity = RuntimeActivity {
            input: Some(input),
            pressed: Some(state == InputState::Down),
            context: context.clone(),
            metric_press,
            ..RuntimeActivity::new("input_state")
        };
        if let Some(button) = button {
            activity.params.insert("button".into(), button);
        }
        output.activities.push(activity);
        if let Some(id) = feature_switch {
            let switch_state = match state {
                InputState::Down => SwitchState::Closed,
                InputState::Up => SwitchState::Open,
            };
            self.feature_switch_states.insert(id.clone(), switch_state);
            if !self.is_v6() && state == InputState::Down {
                output.lines.push(format!("SKIP {event_id}\n"));
            }
            output.activities.push(
                RuntimeActivity::new("feature_switch_changed")
                    .with_param("switch", id)
                    .with_param("state", format_switch_state(switch_state))
                    .with_context(context),
            );
            output.completed_receive_sequences.push(receive_sequence);
            return;
        }
        let Some(snapshot) = action_snapshot else {
            if state == InputState::Down {
                output.lines.push(format!("SKIP {event_id}\n"));
            }
            output.completed_receive_sequences.push(receive_sequence);
            if state == InputState::Down {
                output
                    .activities
                    .push(RuntimeActivity::new("input_before_configuration").with_context(context));
            }
            return;
        };

        if self.is_v6() {
            let mut metadata = TriggerMetadata {
                receive_sequence,
                event_id,
                placeholder_receive_sequence: None,
            };
            let edge = TriggerEdge {
                input,
                state,
                monotonic_ms,
                snapshot,
                context,
            };
            let occurrences = self.triggers.edge(edge);
            if state == InputState::Down && !occurrences.is_empty() {
                metadata.placeholder_receive_sequence = Some(receive_sequence);
                *self
                    .pending_receive_sequences
                    .entry(receive_sequence)
                    .or_default() += 1;
                *self
                    .gesture_placeholders
                    .entry(receive_sequence)
                    .or_default() += 1;
                self.trigger_metadata.insert(
                    (input, occurrences[0].origin_monotonic_ms),
                    metadata.clone(),
                );
            }
            let release_origins = occurrences
                .iter()
                .filter(|occurrence| occurrence.trigger == ActionTrigger::Release)
                .map(|occurrence| occurrence.origin_monotonic_ms)
                .collect::<Vec<_>>();
            self.enqueue_occurrences(occurrences, metadata, state == InputState::Up, output);
            for origin in release_origins {
                self.trigger_metadata.remove(&(input, origin));
            }
            self.start_next(output);
        } else if state == InputState::Down {
            let occurrence = TriggerOccurrence {
                sequence: 0,
                input,
                trigger: ActionTrigger::Press,
                origin_monotonic_ms: monotonic_ms,
                snapshot,
                context,
            };
            self.queue.push_back(QueuedOccurrence {
                occurrence,
                receive_sequence,
                event_id,
                release_placeholder: None,
            });
            self.start_next(output);
        } else {
            output.completed_receive_sequences.push(receive_sequence);
        }
    }

    fn is_v6(&self) -> bool {
        self.hello
            .as_ref()
            .is_some_and(|hello| hello.protocol >= ACTION_RUN_PROTOCOL_VERSION)
    }

    fn enqueue_occurrences(
        &mut self,
        occurrences: Vec<TriggerOccurrence>,
        fallback_metadata: TriggerMetadata,
        prefer_fallback: bool,
        output: &mut SessionOutput,
    ) {
        if occurrences.is_empty() {
            if fallback_metadata.receive_sequence != 0 {
                self.finish_receive_sequence(fallback_metadata.receive_sequence, output);
            }
            return;
        }
        for occurrence in occurrences {
            let key = (occurrence.input, occurrence.origin_monotonic_ms);
            let origin_metadata = self.trigger_metadata.get(&key).cloned();
            let metadata = if prefer_fallback {
                fallback_metadata.clone()
            } else {
                origin_metadata
                    .clone()
                    .unwrap_or_else(|| fallback_metadata.clone())
            };
            let release_placeholder = (occurrence.trigger == ActionTrigger::Release)
                .then(|| origin_metadata.and_then(|metadata| metadata.placeholder_receive_sequence))
                .flatten();
            *self
                .pending_receive_sequences
                .entry(metadata.receive_sequence)
                .or_default() += 1;
            let mut activity = RuntimeActivity::new("trigger_occurred")
                .with_param("trigger", trigger_name(occurrence.trigger))
                .with_param(
                    "originMonotonicMs",
                    occurrence.origin_monotonic_ms.to_string(),
                )
                .with_context(occurrence.context.clone());
            activity.input = Some(occurrence.input);
            activity.pressed = Some(matches!(occurrence.trigger, ActionTrigger::Press));
            if let Some(button) = occurrence
                .snapshot
                .profile
                .button_for(&occurrence.snapshot.hardware_profile_id, &occurrence.input)
            {
                activity.params.insert("button".into(), button.into());
            }
            output.activities.push(activity);
            self.queue.push_back(QueuedOccurrence {
                occurrence,
                receive_sequence: metadata.receive_sequence,
                event_id: metadata.event_id,
                release_placeholder,
            });
        }
    }

    fn finish_receive_sequence(&mut self, receive_sequence: u64, output: &mut SessionOutput) {
        if let Some(pending) = self.pending_receive_sequences.get_mut(&receive_sequence) {
            if *pending > 1 {
                *pending -= 1;
            } else {
                self.pending_receive_sequences.remove(&receive_sequence);
                output.completed_receive_sequences.push(receive_sequence);
            }
        } else {
            output.completed_receive_sequences.push(receive_sequence);
        }
    }

    fn finish_queued_occurrence(
        &mut self,
        receive_sequence: u64,
        release_placeholder: Option<u64>,
        output: &mut SessionOutput,
    ) {
        self.finish_receive_sequence(receive_sequence, output);
        if let Some(placeholder) = release_placeholder {
            self.finish_placeholder(placeholder, output);
        }
    }

    fn finish_placeholder(&mut self, receive_sequence: u64, output: &mut SessionOutput) {
        let present = if let Some(count) = self.gesture_placeholders.get_mut(&receive_sequence) {
            if *count > 1 {
                *count -= 1;
            } else {
                self.gesture_placeholders.remove(&receive_sequence);
            }
            true
        } else {
            false
        };
        if present {
            self.finish_receive_sequence(receive_sequence, output);
        }
    }

    fn settle_placeholders(&mut self, output: &mut SessionOutput) {
        let placeholders = std::mem::take(&mut self.gesture_placeholders);
        for (receive_sequence, count) in placeholders {
            for _ in 0..count {
                self.finish_receive_sequence(receive_sequence, output);
            }
        }
    }

    fn next_host_run_id(&mut self) -> u64 {
        let run_id = self.next_run_id.max(1);
        self.next_run_id = self.next_run_id.wrapping_add(1).max(1);
        run_id
    }

    pub fn poll_triggers(&mut self, monotonic_ms: u64) -> SessionOutput {
        let mut output = SessionOutput::default();
        if !self.is_v6() {
            return output;
        }
        let occurrences = self.triggers.poll(monotonic_ms);
        self.enqueue_occurrences(
            occurrences,
            TriggerMetadata {
                receive_sequence: 0,
                event_id: 0,
                placeholder_receive_sequence: None,
            },
            false,
            &mut output,
        );
        self.start_next(&mut output);
        output
    }

    pub fn next_trigger_deadline_ms(&self) -> Option<u64> {
        self.is_v6()
            .then(|| self.triggers.next_deadline_ms())
            .flatten()
    }

    pub fn on_action_timeout(&mut self, run_id: u64, _monotonic_ms: u64) -> SessionOutput {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.run_id() == run_id)
        {
            self.fail_active_deferred("action_timeout", None)
        } else {
            SessionOutput::default()
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
        if self.hello.is_some() {
            self.settle_for_handshake(output);
        }
        self.clear_handshake();
        if let Err(error) = validate_hello(self.candidate_board, &hello) {
            output.activities.push(activity_from_error(error));
            return;
        }
        self.hello = Some(hello);
        let Some(snapshot) = snapshot else {
            return;
        };
        let revision = self.revision.wrapping_add(1).max(1);
        self.configure_snapshot(Some(snapshot), revision, output);
    }

    fn configure_snapshot(
        &mut self,
        snapshot: Option<Arc<RuntimeProfileSnapshot>>,
        revision: u32,
        output: &mut SessionOutput,
    ) {
        self.profile = snapshot;
        self.feature_switch_states.clear();
        self.ready = false;
        self.configuring = None;
        if self.hello.is_none() {
            return;
        }
        if self.profile.is_none() {
            self.clear_topology(revision, output);
            return;
        }
        let hello = self.hello.as_ref().expect("HELLO was checked above");
        let runtime = self.profile.as_ref().expect("profile was checked above");
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
        if !crate::hardware::board_profile_ids_match(
            &hardware.board_profile_id,
            self.candidate_board.id,
        ) {
            output
                .activities
                .push(RuntimeActivity::new("assignment_board_mismatch"));
            return;
        }
        let required_oled_protocol = if hardware.sh1106.is_some() {
            Some(SH1106_PROTOCOL_VERSION)
        } else if hardware.ssd1306.is_some() {
            Some(OLED_PROTOCOL_VERSION)
        } else {
            None
        };
        if let Some(required) = required_oled_protocol.filter(|required| hello.protocol < *required)
        {
            output.activities.push(activity_from_error(
                crate::workspace::AppError::new("protocol_mismatch")
                    .with_param("expected", required.to_string())
                    .with_param("actual", hello.protocol.to_string()),
            ));
            return;
        }
        let minimum_protocol = runtime.profile.minimum_protocol_version();
        if hello.protocol < minimum_protocol {
            output.activities.push(activity_from_error(
                crate::workspace::AppError::new("firmware_update_required")
                    .with_param("expected", minimum_protocol.to_string())
                    .with_param("actual", hello.protocol.to_string()),
            ));
            return;
        }
        let reported_pins = hello.pins.iter().copied().collect::<BTreeSet<_>>();
        self.revision = revision.max(1);
        match topology_commands(hardware, self.revision, &reported_pins) {
            Ok(lines) => {
                self.configuring = Some(ConfigurationInFlight {
                    revision: self.revision,
                    kind: ConfigurationKind::Activate,
                });
                output.lines.extend(lines);
            }
            Err(error) => output.activities.push(activity_from_error(error)),
        }
    }

    fn clear_topology(&mut self, revision: u32, output: &mut SessionOutput) {
        if self.hello.is_none() {
            return;
        }
        self.revision = revision.max(1);
        self.configuring = Some(ConfigurationInFlight {
            revision: self.revision,
            kind: ConfigurationKind::Clear,
        });
        output.lines.extend([
            format!(
                "CONFIG_BEGIN {} {EMPTY_TOPOLOGY_DEBOUNCE_MS}\n",
                self.revision
            ),
            format!("CONFIG_COMMIT {}\n", self.revision),
        ]);
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
        self.active_release_placeholder = None;
        self.feature_switch_states.clear();
        self.active_receive_sequence = None;
        self.pending_paste = None;
        self.queue.clear();
        self.pending_reconfiguration = None;
        self.pending_learning = None;
        self.learning = None;
        self.reset_gestures();
        self.gesture_placeholders.clear();
        self.pending_receive_sequences.clear();
    }

    fn settle_for_handshake(&mut self, output: &mut SessionOutput) {
        if let Some(mut sequence) = self.active.take() {
            let run_id = sequence.run_id();
            sequence.abort();
            output.lines.push(format!("SKIP {run_id}\n"));
            self.active_snapshot = None;
            self.active_context = None;
            self.pending_paste = None;
            if let Some(receive_sequence) = self.active_receive_sequence.take() {
                let release_placeholder = self.active_release_placeholder.take();
                self.finish_queued_occurrence(receive_sequence, release_placeholder, output);
            }
        }
        let queued_items = std::mem::take(&mut self.queue);
        let is_v6 = self.is_v6();
        for queued in queued_items {
            if !is_v6 {
                output.lines.push(format!("SKIP {}\n", queued.event_id));
            }
            self.finish_queued_occurrence(
                queued.receive_sequence,
                queued.release_placeholder,
                output,
            );
        }
        for receive_sequence in self
            .pending_receive_sequences
            .keys()
            .copied()
            .collect::<Vec<_>>()
        {
            if receive_sequence != 0 {
                output.completed_receive_sequences.push(receive_sequence);
            }
        }
        self.pending_receive_sequences.clear();
        self.active_receive_sequence = None;
        self.active_release_placeholder = None;
        self.active_snapshot = None;
        self.active_context = None;
        self.pending_paste = None;
    }

    fn reset_gestures(&mut self) {
        self.triggers.reset();
        self.trigger_metadata.clear();
    }

    fn settle_queued(&mut self, output: &mut SessionOutput) {
        let queued_items = std::mem::take(&mut self.queue);
        let is_v6 = self.is_v6();
        for queued in queued_items {
            if !is_v6 {
                output.lines.push(format!("SKIP {}\n", queued.event_id));
            }
            self.finish_queued_occurrence(
                queued.receive_sequence,
                queued.release_placeholder,
                output,
            );
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

    fn handle_done(&mut self, run_id: u64, step: u16, output: &mut SessionOutput) {
        let Some(sequence) = self.active.as_mut() else {
            output
                .activities
                .push(RuntimeActivity::new("unexpected_action_acknowledgement"));
            return;
        };
        let completed = match sequence.acknowledge(run_id, step) {
            Ok(completed) => completed,
            Err(error) => {
                let active_run = sequence.run_id();
                output.lines.push(format!("SKIP {active_run}\n"));
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
                    let release_placeholder = self.active_release_placeholder.take();
                    self.finish_queued_occurrence(receive_sequence, release_placeholder, output);
                }
                self.start_next(output);
                return;
            }
        };
        let activity = action_activity("action_step_completed", &completed)
            .with_context(self.active_context.clone());
        output.activities.push(with_action_result_log(
            activity,
            self.active_snapshot.as_deref(),
            &completed,
            true,
        ));
        if sequence.is_complete() {
            self.active = None;
            self.active_snapshot = None;
            self.active_context = None;
            self.pending_paste = None;
            if let Some(receive_sequence) = self.active_receive_sequence.take() {
                let release_placeholder = self.active_release_placeholder.take();
                self.finish_queued_occurrence(receive_sequence, release_placeholder, output);
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
            let occurrence = queued.occurrence;
            let runtime = &occurrence.snapshot;
            let Some(button) = runtime
                .profile
                .button_for(&runtime.hardware_profile_id, &occurrence.input)
                .map(str::to_owned)
            else {
                if !self.is_v6() {
                    output.lines.push(format!("SKIP {}\n", queued.event_id));
                }
                self.finish_queued_occurrence(
                    queued.receive_sequence,
                    queued.release_placeholder,
                    output,
                );
                output
                    .activities
                    .push(RuntimeActivity::new("unmapped_input").with_context(occurrence.context));
                continue;
            };
            let actions = runtime
                .profile
                .actions
                .get(&button)
                .map(|triggers| triggers.actions_for(occurrence.trigger).to_vec())
                .unwrap_or_default();
            if !runtime.profile.button_is_enabled(
                &runtime.hardware_profile_id,
                &button,
                &self.feature_switch_states,
            ) {
                if !self.is_v6() {
                    output.lines.push(format!("SKIP {}\n", queued.event_id));
                }
                self.finish_queued_occurrence(
                    queued.receive_sequence,
                    queued.release_placeholder,
                    output,
                );
                let feature_disabled_log = FeatureDisabledLog {
                    attribution: runtime.metric_attribution.clone(),
                    button_id: button.clone(),
                    occurred_at_ms: occurrence
                        .context
                        .as_ref()
                        .map(|context| context.timestamp_ms),
                };
                output.activities.push(RuntimeActivity {
                    feature_disabled_log: Some(feature_disabled_log),
                    ..RuntimeActivity::new("feature_disabled")
                        .with_param("button", button)
                        .with_context(occurrence.context)
                });
                continue;
            }
            if actions.is_empty() {
                if !self.is_v6() {
                    output.lines.push(format!("SKIP {}\n", queued.event_id));
                }
                self.finish_queued_occurrence(
                    queued.receive_sequence,
                    queued.release_placeholder,
                    output,
                );
                output.activities.push(
                    RuntimeActivity::new("empty_action_list")
                        .with_param("button", button)
                        .with_param("trigger", trigger_name(occurrence.trigger))
                        .with_context(occurrence.context),
                );
                continue;
            }
            let run_id = if self.is_v6() {
                self.next_host_run_id()
            } else {
                queued.event_id
            };
            self.active = Some(ActionSequence::new(
                run_id,
                button,
                occurrence.trigger,
                actions,
            ));
            self.active_snapshot = Some(Arc::clone(&occurrence.snapshot));
            self.active_context = occurrence.context;
            self.active_receive_sequence = Some(queued.receive_sequence);
            self.active_release_placeholder = queued.release_placeholder;
            self.emit_active_step(output);
        }
    }

    fn emit_active_step(&mut self, output: &mut SessionOutput) {
        let target_opener = Arc::clone(&self.target_opener);
        let Some(step) = self.active.as_mut().and_then(ActionSequence::next_step) else {
            return;
        };
        output.activities.push(
            action_activity("action_step_started", &step).with_context(self.active_context.clone()),
        );
        let result = match &step.action {
            crate::profile::ButtonAction::Paste { text } => {
                let request = PendingPaste {
                    receive_sequence: self.active_receive_sequence.unwrap_or_default(),
                    event_id: step.run_id,
                    step: step.step,
                    total: step.total,
                    text: text.clone(),
                    context: self.active_context.clone(),
                };
                self.pending_paste = Some(request.clone());
                output.paste_requests.push(request);
                return;
            }
            crate::profile::ButtonAction::Hotkey { .. }
            | crate::profile::ButtonAction::Media { .. } => self.command_step(&step),
            crate::profile::ButtonAction::Delay { duration_ms } => {
                output.action_timeout =
                    Some(ACTION_ACK_TIMEOUT + Duration::from_millis(u64::from(*duration_ms)));
                self.command_step(&step)
            }
            crate::profile::ButtonAction::Open { target } => target_opener
                .open(target)
                .map_err(|_| "open_target_failed".to_owned())
                .and_then(|()| self.command_step(&step)),
        };
        match result {
            Ok(line) => output.lines.push(line),
            Err(error) => self.fail_action_step(output, &step, error),
        }
    }

    fn command_step(&self, step: &crate::protocol::ActionStep) -> Result<String, String> {
        if self.is_v6() {
            step.command_v6(|_| Ok(()))
        } else {
            step.command_legacy(|_| Ok(()))
        }
    }

    fn fail_action_step(
        &mut self,
        output: &mut SessionOutput,
        step: &crate::protocol::ActionStep,
        error: String,
    ) {
        if let Some(sequence) = self.active.as_mut() {
            sequence.abort();
        }
        output.lines.push(format!("SKIP {}\n", step.run_id));
        let detail = if matches!(&step.action, crate::profile::ButtonAction::Open { .. }) {
            "open_target_failed".into()
        } else {
            error
        };
        let activity = action_activity("action_step_failed", step)
            .with_detail(detail)
            .with_context(self.active_context.clone());
        output.activities.push(with_action_result_log(
            activity,
            self.active_snapshot.as_deref(),
            step,
            false,
        ));
        self.active = None;
        self.active_snapshot = None;
        self.active_context = None;
        self.pending_paste = None;
        if let Some(receive_sequence) = self.active_receive_sequence.take() {
            let release_placeholder = self.active_release_placeholder.take();
            self.finish_queued_occurrence(receive_sequence, release_placeholder, output);
        }
        self.start_next(output);
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
            output.lines.push(format_paste_command(
                request.event_id,
                request.step,
                request.total,
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
                Err(_error) => {
                    merge_output(
                        &mut output,
                        self.fail_active_deferred(
                            "action_step_failed",
                            Some("clipboard_write_failed".into()),
                        ),
                    );
                }
            }
        }
        output
    }
}

fn action_activity(code: &str, step: &crate::protocol::ActionStep) -> RuntimeActivity {
    let mut activity = RuntimeActivity::new(code)
        .with_param("runId", step.run_id.to_string())
        .with_param("button", &step.button)
        .with_param("step", step.step.to_string())
        .with_param("total", step.total.to_string());
    match &step.action {
        crate::profile::ButtonAction::Paste { text } => {
            activity.params.insert("actionKind".into(), "paste".into());
            activity
                .params
                .insert("characterCount".into(), text.chars().count().to_string());
        }
        crate::profile::ButtonAction::Hotkey { keys } => {
            activity.params.insert("actionKind".into(), "hotkey".into());
            activity.params.insert("keys".into(), keys.join("+"));
        }
        crate::profile::ButtonAction::Delay { duration_ms } => {
            activity.params.insert("actionKind".into(), "delay".into());
            activity
                .params
                .insert("durationMs".into(), duration_ms.to_string());
        }
        crate::profile::ButtonAction::Media { command } => {
            activity.params.insert("actionKind".into(), "media".into());
            let command = match command {
                crate::profile::MediaCommand::PlayPause => "play_pause",
                crate::profile::MediaCommand::PreviousTrack => "previous_track",
                crate::profile::MediaCommand::NextTrack => "next_track",
                crate::profile::MediaCommand::Stop => "stop",
                crate::profile::MediaCommand::VolumeUp => "volume_up",
                crate::profile::MediaCommand::VolumeDown => "volume_down",
                crate::profile::MediaCommand::Mute => "mute",
            };
            activity.params.insert("command".into(), command.into());
        }
        crate::profile::ButtonAction::Open { target } => {
            activity.params.insert("actionKind".into(), "open".into());
            activity.params.insert(
                "targetKind".into(),
                if target.contains("://") {
                    "url"
                } else {
                    "path"
                }
                .into(),
            );
            activity
                .params
                .insert("characterCount".into(), target.chars().count().to_string());
        }
    }
    activity
}

fn with_action_result_log(
    mut activity: RuntimeActivity,
    snapshot: Option<&RuntimeProfileSnapshot>,
    step: &crate::protocol::ActionStep,
    succeeded: bool,
) -> RuntimeActivity {
    if let (Some(snapshot), Some(action_kind)) =
        (snapshot, activity.params.get("actionKind").cloned())
    {
        activity.action_result_log = Some(ActionResultLog {
            attribution: snapshot.metric_attribution.clone(),
            button_id: step.button.clone(),
            action_kind,
            succeeded,
            occurred_at_ms: activity
                .context
                .as_ref()
                .map(|context| context.timestamp_ms),
        });
    }
    activity
}

fn trigger_name(trigger: ActionTrigger) -> &'static str {
    match trigger {
        ActionTrigger::Press => "press",
        ActionTrigger::Release => "release",
        ActionTrigger::LongPress => "long_press",
        ActionTrigger::DoublePress => "double_press",
    }
}

#[cfg(test)]
fn message_event_id(message: &DeviceMessage) -> Option<u64> {
    match message {
        DeviceMessage::State { event_id, .. } => Some(*event_id),
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
    if source.action_timeout.is_some() {
        target.action_timeout = source.action_timeout;
    }
}

fn configuration_error_code(code: &str) -> &str {
    match code {
        "invalid_begin"
        | "invalid_direct"
        | "invalid_matrix"
        | "invalid_oled"
        | "invalid_commit"
        | "invalid_learning"
        | "invalid_learning_revision" => code,
        _ => "device_configuration_error",
    }
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
    let attribution = if let Some(metric_press) = activity.metric_press.as_ref() {
        metrics.record_button_press(
            &metric_press.attribution,
            &metric_press.button_id,
            metric_press.occurred_at_ms,
        )?;
        &metric_press.attribution
    } else if let Some(log) = activity.feature_disabled_log.as_ref() {
        metrics.record_feature_disabled(
            &log.attribution,
            &log.button_id,
            log.occurred_at_ms.unwrap_or(snapshot_at_ms),
        )?;
        &log.attribution
    } else if let Some(log) = activity.action_result_log.as_ref() {
        metrics.record_action_result(
            &log.attribution,
            &log.button_id,
            &log.action_kind,
            log.succeeded,
            activity.detail.as_deref(),
            log.occurred_at_ms.unwrap_or(snapshot_at_ms),
        )?;
        &log.attribution
    } else {
        return Ok(None);
    };
    metrics
        .home_snapshot(&attribution.device_profile_id, None, snapshot_at_ms)
        .map(Some)
}

fn monotonic_ms_since(origin: Instant, now: Instant) -> u64 {
    now.saturating_duration_since(origin)
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn monotonic_deadline_reached(now_ms: u64, deadline_ms: u64) -> bool {
    now_ms.wrapping_sub(deadline_ms) < (1_u64 << 63)
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
    product_cache: Option<Arc<ProductDefinitionCache>>,
}

impl SystemWorkerLauncher {
    pub fn new(
        paste: PasteHandle,
        metrics: Option<Arc<MetricsStore>>,
        operation_barrier: Arc<RwLock<()>>,
        config_directory: &Path,
    ) -> Self {
        let mut launcher = Self::with_runtime(
            paste,
            metrics,
            operation_barrier,
            Arc::new(SystemSerialTransportFactory),
            Arc::new(SystemClock::default()),
        );
        launcher.product_cache = Some(Arc::new(ProductDefinitionCache::new(
            config_directory.join("product-definitions"),
        )));
        launcher
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
            product_cache: None,
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
            .timeout(SERIAL_COMMAND_POLL_INTERVAL)
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

#[derive(Default)]
pub struct DeviceDisplayLink {
    tracker: SceneTracker,
    enabled: bool,
    renderer: Option<Arc<dyn DisplayRenderer>>,
    max_font_id: u8,
    pending_since: Option<Instant>,
    queued_update: Option<SceneUpdate>,
    desired_scene: Option<RenderedScene>,
    latest_snapshot: Option<Arc<DisplaySnapshot>>,
    needs_resync: bool,
}

impl DeviceDisplayLink {
    pub(crate) fn configure(
        &mut self,
        protocol: u16,
        profile: Option<&RuntimeProfileSnapshot>,
        registry: &RendererRegistry,
    ) {
        let panel_id = profile
            .and_then(|runtime| {
                runtime
                    .profile
                    .hardware_profile(&runtime.hardware_profile_id)
            })
            .and_then(|hardware| {
                if hardware.sh1106.is_some() && protocol >= SH1106_PROTOCOL_VERSION {
                    Some(SH1106_PANEL_ID)
                } else if hardware.ssd1306.is_some() && protocol >= OLED_PROTOCOL_VERSION {
                    Some(SSD1306_PANEL_ID)
                } else {
                    None
                }
            });
        self.configure_panel(protocol, panel_id, registry);
    }

    fn configure_panel(
        &mut self,
        protocol: u16,
        panel_id: Option<&str>,
        registry: &RendererRegistry,
    ) {
        let renderer = (protocol >= DISPLAY_PROTOCOL_VERSION)
            .then(|| panel_id.and_then(|id| registry.renderer(id).ok()))
            .flatten();
        let selected_panel = renderer.as_ref().map(|renderer| renderer.panel_id());
        let current_panel = self.renderer.as_ref().map(|renderer| renderer.panel_id());
        let max_font_id = renderer.as_ref().map_or(0, |renderer| {
            if protocol >= DISPLAY_LARGE_FONT_PROTOCOL_VERSION {
                renderer.capabilities().max_font_id
            } else {
                renderer.capabilities().ascii_font_id
            }
        });
        if self.enabled == renderer.is_some()
            && current_panel == selected_panel
            && self.max_font_id == max_font_id
        {
            return;
        }

        self.tracker = SceneTracker::default();
        self.pending_since = None;
        self.queued_update = None;
        self.desired_scene = None;
        self.needs_resync = false;
        self.enabled = renderer.is_some();
        self.renderer = renderer;
        self.max_font_id = max_font_id;
        let _ = self.render_latest();
    }

    fn reset_connection(
        &mut self,
        protocol: u16,
        profile: Option<&RuntimeProfileSnapshot>,
        registry: &RendererRegistry,
    ) {
        self.tracker = SceneTracker::default();
        self.enabled = false;
        self.renderer = None;
        self.max_font_id = 0;
        self.pending_since = None;
        self.queued_update = None;
        self.desired_scene = None;
        self.needs_resync = false;
        self.configure(protocol, profile, registry);
    }

    pub(crate) fn update_desired(&mut self, snapshot: Arc<DisplaySnapshot>) -> Result<(), String> {
        self.latest_snapshot = Some(snapshot);
        self.render_latest()
    }

    fn render_latest(&mut self) -> Result<(), String> {
        let (Some(renderer), Some(snapshot)) =
            (self.renderer.as_ref(), self.latest_snapshot.as_ref())
        else {
            self.desired_scene = None;
            return Ok(());
        };
        self.desired_scene = Some(
            renderer
                .render_with_font_limit(snapshot, self.max_font_id)
                .map_err(str::to_owned)?,
        );
        Ok(())
    }

    pub(crate) fn next_lines(&mut self, now: Instant) -> Result<Vec<String>, String> {
        if !self.enabled {
            return Ok(Vec::new());
        }
        if self.queued_update.is_some() {
            return Ok(Vec::new());
        }
        if self
            .pending_since
            .is_some_and(|sent_at| now.saturating_duration_since(sent_at) >= DISPLAY_ACK_TIMEOUT)
        {
            eprintln!("display acknowledgement timeout; retrying latest full scene");
            self.pending_since = None;
            self.needs_resync = true;
        }
        if self.pending_since.is_some() {
            return Ok(Vec::new());
        }
        let update = if self.needs_resync {
            self.needs_resync = false;
            if let Some(scene) = self.desired_scene.clone() {
                let _ = self.tracker.prepare(scene);
            }
            self.tracker.resync()
        } else {
            self.desired_scene
                .clone()
                .and_then(|scene| self.tracker.prepare(scene))
        };
        let Some(update) = update else {
            return Ok(Vec::new());
        };
        let lines = display_commands(&update)?;
        self.queued_update = Some(update);
        Ok(lines)
    }

    pub(crate) fn mark_transmitted(&mut self, now: Instant) {
        if self.queued_update.take().is_some() {
            self.pending_since = Some(now);
        }
    }

    pub(crate) fn on_message(&mut self, message: &DeviceMessage) -> bool {
        match message {
            DeviceMessage::DisplayOk { revision } => {
                if self.pending_since.is_none() {
                    return true;
                }
                self.pending_since = None;
                let _ = self.tracker.ack(*revision);
                true
            }
            DeviceMessage::DisplayResync { .. } | DeviceMessage::DisplayError { .. } => {
                self.pending_since = None;
                self.queued_update = None;
                self.needs_resync = true;
                true
            }
            _ => false,
        }
    }
}

struct WorkerRuntime {
    paste: PasteHandle,
    metrics: Option<Arc<MetricsStore>>,
    operation_barrier: Arc<RwLock<()>>,
    transport_factory: Arc<dyn SerialTransportFactory>,
    clock: Arc<dyn Clock>,
    renderers: Arc<RendererRegistry>,
    product_cache: Option<Arc<ProductDefinitionCache>>,
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
        self.start_with_renderers(
            start,
            events,
            WorkerRendererRegistry::new(Arc::new(built_in_renderer_registry())),
        )
    }

    fn start_with_renderers(
        &self,
        start: WorkerStart,
        events: mpsc::Sender<WorkerEvent>,
        renderers: WorkerRendererRegistry,
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
            renderers: renderers.into_inner(),
            product_cache: self.product_cache.clone(),
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
    let renderers = runtime.renderers.as_ref();
    let board = crate::hardware::board_by_id(&start.board_profile_id)
        .ok_or_else(|| "unknown_board_profile".to_owned())?;
    let mut port = transport_factory.open(&start.port)?;
    port.prepare()?;
    port.write_all(b"HELLO\n")
        .and_then(|()| port.flush())
        .map_err(|error| format!("serial_handshake_failed: {error}"))?;
    let mut device = BufReader::new(port);
    let hello = read_valid_hello(&mut device, board, clock, stop)?;
    let product_definition = read_embedded_product_definition(
        &mut device,
        &hello,
        board,
        clock,
        stop,
        runtime.product_cache.as_deref(),
    )?;
    events
        .send(WorkerEvent::HelloValidated {
            generation: start.generation,
            device_id: start.device_id.clone(),
            capabilities: hello.clone(),
            product_definition,
        })
        .map_err(|_| "coordinator_stopped".to_owned())?;
    let mut session = DeviceSession::without_model(board);
    let mut display_protocol = hello.protocol;
    let mut display_link = DeviceDisplayLink::default();
    display_link.reset_connection(display_protocol, None, renderers);
    let monotonic_origin = clock.monotonic_now();
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
                    display_link.configure(display_protocol, snapshot.as_deref(), renderers);
                    current_context = RuntimeEventContext::from_snapshot(
                        clock.unix_time_ms(),
                        snapshot.as_deref(),
                    )
                    .with_port(current_port.clone());
                    (session.update_snapshot(snapshot), current_context.clone())
                }
                WorkerCommand::Reconfigure { snapshot, revision } => {
                    display_link.configure(display_protocol, snapshot.as_deref(), renderers);
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
                    display_link.configure(display_protocol, snapshot.as_deref(), renderers);
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
                WorkerCommand::UpdateDisplay(snapshot) => {
                    display_link.update_desired(snapshot)?;
                    (SessionOutput::default(), current_context.clone())
                }
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

        let display_lines = display_link.next_lines(clock.monotonic_now())?;
        if !display_lines.is_empty() {
            write_display_lines(device.get_mut(), display_lines)?;
            display_link.mark_transmitted(clock.monotonic_now());
        }

        let monotonic_now_ms = monotonic_ms_since(monotonic_origin, clock.monotonic_now());
        let trigger_output = session
            .next_trigger_deadline_ms()
            .filter(|deadline| monotonic_deadline_reached(monotonic_now_ms, *deadline))
            .map(|_| session.poll_triggers(monotonic_now_ms))
            .unwrap_or_default();
        if !trigger_output.lines.is_empty()
            || !trigger_output.activities.is_empty()
            || !trigger_output.paste_requests.is_empty()
        {
            write_isolated_output(
                start,
                events,
                paste,
                metrics,
                operation_barrier,
                device.get_mut(),
                trigger_output,
                &mut pending_paste,
                &mut action_deadline,
                &current_context.with_timestamp(clock.unix_time_ms()),
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
                Ok(PasteReply::ClipboardError(_error)) => {
                    pending_paste = None;
                    active_paste_ack = None;
                    let output = session.fail_active_deferred(
                        "action_step_failed",
                        Some("clipboard_write_failed".into()),
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
            let output = session
                .active
                .as_ref()
                .map(ActionSequence::run_id)
                .map(|run_id| {
                    session.on_action_timeout(
                        run_id,
                        monotonic_ms_since(monotonic_origin, clock.monotonic_now()),
                    )
                })
                .unwrap_or_default();
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
                    message @ (DeviceMessage::DisplayOk { .. }
                    | DeviceMessage::DisplayResync { .. }
                    | DeviceMessage::DisplayError { .. }) => {
                        display_link.on_message(&message);
                    }
                    DeviceMessage::State {
                        event_id,
                        input,
                        state,
                    } => {
                        let captured = session.capture_input_with_monotonic(
                            &current_context,
                            received_at_ms,
                            monotonic_ms_since(monotonic_origin, clock.monotonic_now()),
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
                    DeviceMessage::Done { run_id, step } => {
                        if active_paste_ack == Some((run_id, step)) {
                            if paste.complete(&start.device_id, run_id, step).is_err() {
                                continue;
                            }
                            active_paste_ack = None;
                            pending_paste = None;
                        }
                        let output = session.on_message_deferred(
                            DeviceMessage::Done { run_id, step },
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
                        if session.hello.is_some() {
                            pending_paste = None;
                            active_paste_ack = None;
                            action_deadline = None;
                            let _ = paste.cancel_device(&start.device_id);
                        }
                        validate_hello(board, capability).map_err(|error| error.code.clone())?;
                        display_protocol = capability.protocol;
                        display_link.reset_connection(
                            display_protocol,
                            session.profile.as_deref(),
                            renderers,
                        );
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

fn read_embedded_product_definition<T: Read + Write>(
    device: &mut BufReader<T>,
    hello: &HelloCapabilities,
    board: &BoardProfile,
    clock: &dyn Clock,
    stop: &AtomicBool,
    cache: Option<&ProductDefinitionCache>,
) -> Result<Option<ProductDefinition>, String> {
    let Some(expected_product_id) = hello.product_version_id.as_deref() else {
        return Ok(None);
    };
    device
        .get_mut()
        .write_all(b"PRODUCT_INFO\n")
        .and_then(|()| device.get_mut().flush())
        .map_err(|error| format!("product_info_write_failed: {error}"))?;
    let deadline = clock.monotonic_now() + PRODUCT_READ_TIMEOUT;
    let mut line = Vec::new();
    let (length, sha256) = loop {
        if stop.load(Ordering::Relaxed) || clock.monotonic_now() >= deadline {
            return Err("product_read_timeout".into());
        }
        line.clear();
        match device.read_until(b'\n', &mut line) {
            Ok(0) => return Err("device_disconnected".into()),
            Ok(_) => {
                let text =
                    std::str::from_utf8(&line).map_err(|_| "invalid_product_info".to_owned())?;
                match parse_device(text) {
                    Some(DeviceMessage::ProductInfo {
                        product_version_id: Some(product_version_id),
                        schema_version,
                        length,
                        sha256: Some(sha256),
                    }) if product_version_id == expected_product_id
                        && schema_version == PRODUCT_DEFINITION_SCHEMA_VERSION =>
                    {
                        break (length, sha256);
                    }
                    Some(DeviceMessage::ProductError { code }) => {
                        return Err(format!("product_info_failed:{code}"));
                    }
                    _ => return Err("invalid_product_info".into()),
                }
            }
            Err(error) if error.kind() == ErrorKind::TimedOut => {}
            Err(error) => return Err(format!("product_info_read_failed: {error}")),
        }
    };

    if let Some(definition) =
        cache.and_then(|cache| cache.load(&sha256, length, expected_product_id, board.id))
    {
        return Ok(Some(definition));
    }

    device
        .get_mut()
        .write_all(b"PRODUCT_READ\n")
        .and_then(|()| device.get_mut().flush())
        .map_err(|error| format!("product_read_write_failed: {error}"))?;
    let mut transfer =
        ProductDefinitionTransfer::new(length, sha256.clone()).map_err(|error| error.code)?;
    let bytes = loop {
        if stop.load(Ordering::Relaxed) || clock.monotonic_now() >= deadline {
            return Err("product_read_timeout".into());
        }
        line.clear();
        match device.read_until(b'\n', &mut line) {
            Ok(0) => return Err("device_disconnected".into()),
            Ok(_) => {
                let text = std::str::from_utf8(&line)
                    .map_err(|_| "invalid_product_transfer_sequence".to_owned())?;
                let message = parse_device(text)
                    .ok_or_else(|| "invalid_product_transfer_sequence".to_owned())?;
                if let Some(bytes) = transfer.push(message).map_err(|error| error.code)? {
                    break bytes;
                }
            }
            Err(error) if error.kind() == ErrorKind::TimedOut => {}
            Err(error) => return Err(format!("product_read_failed: {error}")),
        }
    };
    let definition = ProductDefinition::parse_json(&bytes).map_err(|error| error.code)?;
    if definition.product.product_version_id != expected_product_id
        || !crate::hardware::board_profile_ids_match(
            &definition.hardware_profile.board_profile_id,
            board.id,
        )
    {
        return Err("product_definition_identity_mismatch".into());
    }
    let normalized = definition.normalize().map_err(|error| error.code)?;
    if normalized.sha256 != sha256 || normalized.json.as_bytes() != bytes {
        return Err("product_definition_not_canonical".into());
    }
    if let Some(cache) = cache {
        let _ = cache.store(&normalized);
    }
    Ok(Some(definition))
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
    let action_timeout = output.action_timeout.take().unwrap_or(ACTION_ACK_TIMEOUT);
    let sent_action = output.lines.iter().any(|line| {
        ["PASTE ", "HOTKEY ", "CHORD ", "DELAY ", "MEDIA ", "HOST "]
            .iter()
            .any(|prefix| line.starts_with(prefix))
    });
    for line in output.lines {
        writer
            .write_all(line.as_bytes())
            .map_err(|error| format!("serial_write_failed: {error}"))?;
    }
    writer
        .flush()
        .map_err(|error| format!("serial_write_failed: {error}"))?;
    if sent_action {
        *action_deadline = Some(clock.monotonic_now() + action_timeout);
    }
    Ok(())
}

fn write_display_lines<W: Write + ?Sized>(
    writer: &mut W,
    lines: Vec<String>,
) -> Result<(), String> {
    if lines.is_empty() {
        return Ok(());
    }
    for line in lines {
        writer
            .write_all(line.as_bytes())
            .map_err(|error| format!("serial_write_failed: {error}"))?;
    }
    writer
        .flush()
        .map_err(|error| format!("serial_write_failed: {error}"))
}

#[cfg(target_os = "macos")]
fn open_target(target: &str) -> Result<(), String> {
    let status = Command::new("/usr/bin/open")
        .arg("--")
        .arg(target)
        .status()
        .map_err(|error| format!("open target: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("open target exited {status}"))
}

#[cfg(target_os = "windows")]
fn open_target(target: &str) -> Result<(), String> {
    use std::{iter, ptr};
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

    let operation = "open\0".encode_utf16().collect::<Vec<_>>();
    let target = target
        .encode_utf16()
        .chain(iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        Ok(())
    } else {
        Err(format!(
            "open target failed with shell code {}",
            result as isize
        ))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn open_target(_: &str) -> Result<(), String> {
    Err("opening targets is unsupported on this platform".into())
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
        coordinator::{EventLevel, RuntimeEvent},
        display::{
            DisplayCapabilities, DisplayItem, DisplayPriority, DisplayRegion, DisplayRenderer,
            DisplaySnapshot, DisplayState, DrawOperation, Rect, RenderedScene, RendererRegistry,
            SourceHealth, built_in_renderer_registry,
        },
        hardware::DeviceId,
        metrics::{MetricAttribution, MetricsStore},
        model::{ButtonDefinition, ButtonGroup, ModelLayout},
        paste::{ClipboardWriter, PasteCoordinator},
        product::{PRODUCT_DEFINITION_SCHEMA_VERSION, ProductDefinition, ProductIdentity},
        profile::{
            ButtonAction, DeviceProfile, HardwareProfile, InputSource, PROFILE_SCHEMA_VERSION,
            Sh1106Config, Ssd1306Config,
        },
        protocol::{
            DISPLAY_LARGE_FONT_PROTOCOL_VERSION, DISPLAY_PROTOCOL_VERSION, DeviceMessage,
            PhysicalInput,
        },
    };
    use serialport::{SerialPortInfo, SerialPortType, UsbPortInfo};
    use std::{
        cell::RefCell,
        collections::{BTreeMap, BTreeSet},
        io::Cursor,
        sync::Mutex,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    struct RecordingTargetOpener {
        targets: Arc<Mutex<Vec<String>>>,
        error: Option<String>,
    }

    struct MemoryTransport {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl Read for MemoryTransport {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buffer)
        }
    }

    impl Write for MemoryTransport {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.output.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn embedded_product_definition() -> ProductDefinition {
        ProductDefinition {
            schema_version: PRODUCT_DEFINITION_SCHEMA_VERSION,
            product: ProductIdentity {
                display_name: "Kivo Key 1".into(),
                family_id: "key".into(),
                variant_id: "key-rp-k1".into(),
                hardware_revision: 1,
                product_version_id: "key-rp-k1-r01".into(),
                capabilities: Vec::new(),
            },
            layout: ModelLayout {
                id: "key-rp-k1".into(),
                name: "Kivo Key 1".into(),
                groups: vec![ButtonGroup {
                    id: "keys".into(),
                    columns: 1,
                    buttons: vec![ButtonDefinition {
                        id: "K1".into(),
                        label: "K1".into(),
                    }],
                }],
            },
            hardware_profile: HardwareProfile {
                id: "hardware".into(),
                name: "Hardware".into(),
                board_profile_id: crate::hardware::YD_RP2040_BOARD_ID.into(),
                debounce_ms: 30,
                ssd1306: None,
                sh1106: None,
                inputs: vec![InputSource::Direct {
                    id: "direct".into(),
                    keys: BTreeMap::from([("K1".into(), 0)]),
                }],
            },
        }
    }

    #[test]
    fn embedded_product_cache_hit_skips_product_read_transfer() {
        let directory = tempfile::tempdir().unwrap();
        let cache = ProductDefinitionCache::new(directory.path().into());
        let normalized = embedded_product_definition().normalize().unwrap();
        cache.store(&normalized).unwrap();
        let response = format!(
            "PRODUCT_INFO key-rp-k1-r01 1 {} {}\n",
            normalized.byte_length, normalized.sha256
        );
        let mut device = BufReader::new(MemoryTransport {
            input: Cursor::new(response.into_bytes()),
            output: Vec::new(),
        });
        let hello = HelloCapabilities {
            protocol: 9,
            controller_family_id: "rp2040".into(),
            board_profile_id: crate::hardware::YD_RP2040_BOARD_ID.into(),
            firmware_build_id: "test".into(),
            product_version_id: Some("key-rp-k1-r01".into()),
            pins: vec![0],
        };

        let loaded = read_embedded_product_definition(
            &mut device,
            &hello,
            crate::hardware::board_by_id(crate::hardware::YD_RP2040_BOARD_ID).unwrap(),
            &SystemClock::default(),
            &AtomicBool::new(false),
            Some(&cache),
        )
        .unwrap();

        assert_eq!(loaded, Some(normalized.definition));
        assert_eq!(device.into_inner().output, b"PRODUCT_INFO\n");
    }

    impl TargetOpener for RecordingTargetOpener {
        fn open(&self, target: &str) -> Result<(), String> {
            self.targets.lock().unwrap().push(target.into());
            self.error.clone().map_or(Ok(()), Err)
        }
    }

    fn runtime_model() -> RuntimeProfileSnapshot {
        RuntimeProfileSnapshot {
            hardware_profile_id: "esp-primary".into(),
            metric_attribution: MetricAttribution {
                device_id: DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456")
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
                snapshot_metadata: None,
                trigger_settings: TriggerSettings::default(),
                hardware_profiles: vec![HardwareProfile {
                    id: "esp-primary".into(),
                    name: "ESP primary".into(),
                    board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
                    debounce_ms: 30,
                    ssd1306: None,
                    sh1106: None,
                    inputs: vec![InputSource::Direct {
                        id: "direct".into(),
                        keys: BTreeMap::from([("A".into(), 6)]),
                    }],
                }],
                actions: BTreeMap::from([(
                    "A".into(),
                    TriggerActions::press(vec![
                        ButtonAction::Paste {
                            text: "第一步".into(),
                        },
                        ButtonAction::Paste {
                            text: "第二步".into(),
                        },
                    ]),
                )]),
            },
        }
    }

    fn oled_runtime_model() -> RuntimeProfileSnapshot {
        let mut runtime = runtime_model();
        let hardware = &mut runtime.profile.hardware_profiles[0];
        hardware.board_profile_id = crate::hardware::YD_RP2040_BOARD_ID.into();
        hardware.ssd1306 = Some(Ssd1306Config {
            sda: 4,
            scl: 5,
            control_panel: None,
        });
        runtime.metric_attribution.device_id =
            DeviceId::new(crate::hardware::YD_RP2040_BOARD_ID, "ABCDEF123456").unwrap();
        runtime
    }

    fn sh1106_runtime_model() -> RuntimeProfileSnapshot {
        let mut runtime = oled_runtime_model();
        let hardware = &mut runtime.profile.hardware_profiles[0];
        let ssd1306 = hardware.ssd1306.take().unwrap();
        hardware.sh1106 = Some(Sh1106Config {
            sda: ssd1306.sda,
            scl: ssd1306.scl,
            control_panel: None,
        });
        runtime
    }

    fn runtime_model_with_feature_switch() -> RuntimeProfileSnapshot {
        let mut runtime = runtime_model();
        runtime.profile.hardware_profiles[0]
            .inputs
            .push(InputSource::FeatureSwitch {
                id: "mode".into(),
                name: "Mode switch".into(),
                gpio: 7,
                buttons: BTreeSet::from(["A".into()]),
            });
        runtime.profile.validate().unwrap();
        runtime
    }

    fn display_snapshot(running: u32) -> Arc<DisplaySnapshot> {
        Arc::new(DisplaySnapshot {
            items: vec![
                DisplayItem::new(
                    "codex.summary",
                    "codex",
                    DisplayPriority::Ambient,
                    DisplayState::Running,
                    "Codex",
                )
                .unwrap()
                .with_metric("running", running),
            ],
            health: BTreeMap::from([("codex".into(), SourceHealth::Healthy)]),
        })
    }

    struct TestDisplayRenderer {
        panel_id: &'static str,
        text: &'static str,
    }

    struct FlushFailingDisplayWriter;

    impl std::io::Write for FlushFailingDisplayWriter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("flush failed"))
        }
    }

    impl DisplayRenderer for TestDisplayRenderer {
        fn panel_id(&self) -> &'static str {
            self.panel_id
        }

        fn capabilities(&self) -> &DisplayCapabilities {
            static CAPABILITIES: DisplayCapabilities = DisplayCapabilities::ssd1306_128x32_mono();
            &CAPABILITIES
        }

        fn render(&self, _snapshot: &DisplaySnapshot) -> Result<RenderedScene, &'static str> {
            Ok(RenderedScene {
                regions: vec![DisplayRegion::new(
                    0,
                    "test",
                    Rect::new(0, 0, 64, 16),
                    vec![
                        DrawOperation::ClearRegion,
                        DrawOperation::Text {
                            x: 0,
                            baseline_y: 12,
                            font_id: 0,
                            text: self.text.into(),
                        },
                    ],
                )],
            })
        }
    }

    #[test]
    fn display_link_is_silent_for_protocol_six_or_a_profile_without_oled() {
        let registry = built_in_renderer_registry();
        let now = Instant::now();
        let mut legacy = DeviceDisplayLink::default();
        legacy.configure(6, Some(&oled_runtime_model()), &registry);
        legacy.update_desired(display_snapshot(3)).unwrap();
        assert!(legacy.next_lines(now).unwrap().is_empty());

        let mut no_panel = DeviceDisplayLink::default();
        no_panel.configure(DISPLAY_PROTOCOL_VERSION, Some(&runtime_model()), &registry);
        no_panel.update_desired(display_snapshot(3)).unwrap();
        assert!(no_panel.next_lines(now).unwrap().is_empty());
    }

    #[test]
    fn protocol_seven_ssd1306_starts_with_a_full_scene_at_base_zero() {
        let registry = built_in_renderer_registry();
        let mut link = DeviceDisplayLink::default();
        link.configure(
            DISPLAY_PROTOCOL_VERSION,
            Some(&oled_runtime_model()),
            &registry,
        );

        link.update_desired(display_snapshot(3)).unwrap();
        let lines = link.next_lines(Instant::now()).unwrap();

        assert_eq!(lines.first().unwrap(), "DISPLAY_BEGIN 1 0 full\n");
        assert_eq!(lines.last().unwrap(), "DISPLAY_COMMIT 1\n");
    }

    #[test]
    fn protocol_seven_keeps_compact_layout_and_protocol_eight_uses_large_font() {
        let registry = built_in_renderer_registry();
        let now = Instant::now();
        let mut version_seven = DeviceDisplayLink::default();
        version_seven.configure(
            DISPLAY_PROTOCOL_VERSION,
            Some(&oled_runtime_model()),
            &registry,
        );
        version_seven.update_desired(display_snapshot(3)).unwrap();
        let version_seven_lines = version_seven.next_lines(now).unwrap();

        assert!(
            version_seven_lines
                .iter()
                .any(|line| line == "DISPLAY_REGION 0 0 0 64 16\n")
        );
        assert!(
            version_seven_lines
                .iter()
                .all(|line| !line.starts_with("DISPLAY_TEXT 0 9 22 2 "))
        );

        let mut version_eight = DeviceDisplayLink::default();
        version_eight.configure(
            DISPLAY_LARGE_FONT_PROTOCOL_VERSION,
            Some(&oled_runtime_model()),
            &registry,
        );
        version_eight.update_desired(display_snapshot(3)).unwrap();
        let version_eight_lines = version_eight.next_lines(now).unwrap();

        assert!(
            version_eight_lines
                .iter()
                .any(|line| line == "DISPLAY_REGION 0 0 0 128 32\n")
        );
        assert!(
            version_eight_lines
                .iter()
                .any(|line| line.starts_with("DISPLAY_TEXT 0 9 22 2 "))
        );
    }

    #[test]
    fn sh1106_requires_protocol_eleven_and_uses_the_full_panel() {
        let registry = built_in_renderer_registry();
        let now = Instant::now();
        let mut version_ten = DeviceDisplayLink::default();
        version_ten.configure(
            SH1106_PROTOCOL_VERSION - 1,
            Some(&sh1106_runtime_model()),
            &registry,
        );
        version_ten.update_desired(display_snapshot(3)).unwrap();
        assert!(version_ten.next_lines(now).unwrap().is_empty());

        let mut version_eleven = DeviceDisplayLink::default();
        version_eleven.configure(
            SH1106_PROTOCOL_VERSION,
            Some(&sh1106_runtime_model()),
            &registry,
        );
        version_eleven.update_desired(display_snapshot(3)).unwrap();
        let version_eleven_lines = version_eleven.next_lines(now).unwrap();

        assert!(
            version_eleven_lines
                .iter()
                .any(|line| line == "DISPLAY_REGION 0 0 0 128 64\n")
        );
        assert!(
            version_eleven_lines
                .iter()
                .any(|line| line.starts_with("DISPLAY_TEXT 0 9 38 2 "))
        );
    }

    #[test]
    fn display_updates_coalesce_before_the_first_transaction_is_generated() {
        let registry = built_in_renderer_registry();
        let mut link = DeviceDisplayLink::default();
        link.configure(
            DISPLAY_PROTOCOL_VERSION,
            Some(&oled_runtime_model()),
            &registry,
        );

        link.update_desired(display_snapshot(3)).unwrap();
        link.update_desired(display_snapshot(4)).unwrap();
        let lines = link.next_lines(Instant::now()).unwrap();

        assert_eq!(lines.first().unwrap(), "DISPLAY_BEGIN 1 0 full\n");
        assert!(lines.iter().any(|line| line.contains("NCBSVU4=")));
        assert!(!lines.iter().any(|line| line.contains("MyBSVU4=")));
    }

    #[test]
    fn each_display_link_uses_its_selected_renderer_for_the_same_snapshot() {
        let mut registry = RendererRegistry::default();
        registry
            .register(Arc::new(TestDisplayRenderer {
                panel_id: "test_alpha",
                text: "ALPHA",
            }))
            .unwrap();
        registry
            .register(Arc::new(TestDisplayRenderer {
                panel_id: "test_beta",
                text: "BETA",
            }))
            .unwrap();
        let snapshot = display_snapshot(3);
        let now = Instant::now();
        let mut alpha = DeviceDisplayLink::default();
        alpha.configure_panel(DISPLAY_PROTOCOL_VERSION, Some("test_alpha"), &registry);
        alpha.update_desired(Arc::clone(&snapshot)).unwrap();
        let mut beta = DeviceDisplayLink::default();
        beta.configure_panel(DISPLAY_PROTOCOL_VERSION, Some("test_beta"), &registry);
        beta.update_desired(snapshot).unwrap();

        let alpha_lines = alpha.next_lines(now).unwrap();
        let beta_lines = beta.next_lines(now).unwrap();

        assert_ne!(alpha_lines, beta_lines);
        assert!(alpha_lines.iter().any(|line| line.contains("QUxQSEE=")));
        assert!(beta_lines.iter().any(|line| line.contains("QkVUQQ==")));
    }

    #[test]
    fn pending_display_update_coalesces_until_the_exact_ack() {
        let registry = built_in_renderer_registry();
        let now = Instant::now();
        let mut link = DeviceDisplayLink::default();
        link.configure(
            DISPLAY_PROTOCOL_VERSION,
            Some(&oled_runtime_model()),
            &registry,
        );
        link.update_desired(display_snapshot(3)).unwrap();
        assert_eq!(
            link.next_lines(now).unwrap().first().unwrap(),
            "DISPLAY_BEGIN 1 0 full\n"
        );
        link.mark_transmitted(now);

        link.update_desired(display_snapshot(4)).unwrap();
        assert!(link.next_lines(now).unwrap().is_empty());
        assert!(link.on_message(&DeviceMessage::DisplayOk { revision: 1 }));
        let delta = link.next_lines(now).unwrap();

        assert_eq!(delta.first().unwrap(), "DISPLAY_BEGIN 2 1 delta\n");
        assert!(delta.iter().any(|line| line.contains("NCBSVU4=")));
    }

    #[test]
    fn mismatched_display_ack_recovers_with_the_latest_full_scene() {
        let registry = built_in_renderer_registry();
        let now = Instant::now();
        let mut link = DeviceDisplayLink::default();
        link.configure(
            DISPLAY_PROTOCOL_VERSION,
            Some(&oled_runtime_model()),
            &registry,
        );
        link.update_desired(display_snapshot(3)).unwrap();
        link.next_lines(now).unwrap();
        link.mark_transmitted(now);
        link.update_desired(display_snapshot(4)).unwrap();

        assert!(link.on_message(&DeviceMessage::DisplayOk { revision: 9 }));
        let recovered = link.next_lines(now).unwrap();

        assert_eq!(recovered.first().unwrap(), "DISPLAY_BEGIN 2 0 full\n");
        assert!(recovered.iter().any(|line| line.contains("NCBSVU4=")));
    }

    #[test]
    fn display_ack_before_the_transaction_is_written_is_ignored() {
        let registry = built_in_renderer_registry();
        let now = Instant::now();
        let mut link = DeviceDisplayLink::default();
        link.configure(
            DISPLAY_PROTOCOL_VERSION,
            Some(&oled_runtime_model()),
            &registry,
        );
        link.update_desired(display_snapshot(3)).unwrap();

        assert!(link.on_message(&DeviceMessage::DisplayOk { revision: 1 }));

        assert_eq!(
            link.next_lines(now).unwrap().first().unwrap(),
            "DISPLAY_BEGIN 1 0 full\n"
        );
    }

    #[test]
    fn display_ack_after_generation_is_ignored_until_marked_transmitted() {
        let registry = built_in_renderer_registry();
        let now = Instant::now();
        let mut link = DeviceDisplayLink::default();
        link.configure(
            DISPLAY_PROTOCOL_VERSION,
            Some(&oled_runtime_model()),
            &registry,
        );
        link.update_desired(display_snapshot(3)).unwrap();
        assert_eq!(
            link.next_lines(now).unwrap().first().unwrap(),
            "DISPLAY_BEGIN 1 0 full\n"
        );

        assert!(link.on_message(&DeviceMessage::DisplayOk { revision: 1 }));
        link.mark_transmitted(now);
        link.update_desired(display_snapshot(4)).unwrap();
        assert!(
            link.next_lines(now + Duration::from_secs(1))
                .unwrap()
                .is_empty()
        );

        assert!(link.on_message(&DeviceMessage::DisplayOk { revision: 1 }));
        let delta = link.next_lines(now + Duration::from_secs(1)).unwrap();
        assert_eq!(delta.first().unwrap(), "DISPLAY_BEGIN 2 1 delta\n");
        assert!(delta.iter().any(|line| line.contains("NCBSVU4=")));
    }

    #[test]
    fn failed_display_flush_does_not_mark_or_acknowledge_the_generated_revision() {
        let registry = built_in_renderer_registry();
        let now = Instant::now();
        let mut link = DeviceDisplayLink::default();
        link.configure(
            DISPLAY_PROTOCOL_VERSION,
            Some(&oled_runtime_model()),
            &registry,
        );
        link.update_desired(display_snapshot(3)).unwrap();
        let lines = link.next_lines(now).unwrap();

        let error = write_display_lines(&mut FlushFailingDisplayWriter, lines).unwrap_err();
        assert!(error.starts_with("serial_write_failed:"));
        assert!(link.on_message(&DeviceMessage::DisplayOk { revision: 1 }));

        assert!(link.pending_since.is_none());
        assert!(link.queued_update.is_some());
    }

    #[test]
    fn display_ack_timeout_retries_the_latest_scene_full_once_per_deadline() {
        let registry = built_in_renderer_registry();
        let now = Instant::now();
        let mut link = DeviceDisplayLink::default();
        link.configure(
            DISPLAY_PROTOCOL_VERSION,
            Some(&oled_runtime_model()),
            &registry,
        );
        link.update_desired(display_snapshot(3)).unwrap();
        link.next_lines(now).unwrap();
        link.mark_transmitted(now);
        link.update_desired(display_snapshot(4)).unwrap();

        assert!(
            link.next_lines(now + Duration::from_millis(1_999))
                .unwrap()
                .is_empty()
        );
        let retried = link.next_lines(now + Duration::from_secs(2)).unwrap();
        assert_eq!(retried.first().unwrap(), "DISPLAY_BEGIN 2 0 full\n");
        assert!(retried.iter().any(|line| line.contains("NCBSVU4=")));
        assert!(
            link.next_lines(now + Duration::from_secs(2))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn display_resync_and_error_keep_the_link_alive_and_force_full() {
        for reply in [
            DeviceMessage::DisplayResync {
                current_revision: 0,
            },
            DeviceMessage::DisplayError {
                revision: 1,
                code: "invalid_text".into(),
            },
        ] {
            let registry = built_in_renderer_registry();
            let now = Instant::now();
            let mut link = DeviceDisplayLink::default();
            link.configure(
                DISPLAY_PROTOCOL_VERSION,
                Some(&oled_runtime_model()),
                &registry,
            );
            link.update_desired(display_snapshot(3)).unwrap();
            link.next_lines(now).unwrap();
            link.mark_transmitted(now);

            assert!(link.on_message(&reply));
            assert_eq!(
                link.next_lines(now).unwrap().first().unwrap(),
                "DISPLAY_BEGIN 2 0 full\n"
            );
        }
    }

    #[test]
    fn a_fresh_display_link_reconnects_with_a_full_scene() {
        let registry = built_in_renderer_registry();
        let snapshot = display_snapshot(3);
        let now = Instant::now();
        let mut original = DeviceDisplayLink::default();
        original.configure(
            DISPLAY_PROTOCOL_VERSION,
            Some(&oled_runtime_model()),
            &registry,
        );
        original.update_desired(Arc::clone(&snapshot)).unwrap();
        original.next_lines(now).unwrap();
        original.mark_transmitted(now);
        original.on_message(&DeviceMessage::DisplayOk { revision: 1 });

        let mut reconnected = DeviceDisplayLink::default();
        reconnected.configure(
            DISPLAY_PROTOCOL_VERSION,
            Some(&oled_runtime_model()),
            &registry,
        );
        reconnected.update_desired(snapshot).unwrap();

        assert_eq!(
            reconnected.next_lines(now).unwrap().first().unwrap(),
            "DISPLAY_BEGIN 1 0 full\n"
        );
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
        let release = session.on_message(
            DeviceMessage::State {
                event_id: 1,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Up,
            },
            &mut |_| Ok(()),
        );

        let update = persist_metrics(&store, &output.activities[0], timestamp)
            .unwrap()
            .unwrap();

        assert_eq!(update.today_presses, 1);
        assert_eq!(update.logs[0].message, "A pressed");
        assert_eq!(output.activities[0].params["button"], "A");
        assert_eq!(release.activities[0].params["button"], "A");
        drop(store);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn persists_completed_and_failed_action_steps_as_device_activity() {
        let timestamp = 1_720_086_400_000;
        let path = std::env::temp_dir().join(format!(
            "kivo-action-result-metrics-{}-{}.sqlite3",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = MetricsStore::open(&path).unwrap();
        let mut completed_session = DeviceSession::new(runtime_model());
        completed_session.ready = true;
        completed_session.on_message(
            DeviceMessage::State {
                event_id: 1,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            &mut |_| Ok(()),
        );
        let completed = completed_session
            .on_message(DeviceMessage::Done { run_id: 1, step: 1 }, &mut |_| Ok(()));
        let completed_activity = completed
            .activities
            .iter()
            .find(|activity| activity.code == "action_step_completed")
            .unwrap();

        persist_metrics(&store, completed_activity, timestamp).unwrap();

        let mut failed_session = DeviceSession::new(runtime_model());
        failed_session.ready = true;
        failed_session.on_message(
            DeviceMessage::State {
                event_id: 2,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            &mut |_| Ok(()),
        );
        let failed = failed_session
            .fail_active_deferred("action_ack_timeout", Some("device_timeout".into()));
        let failed_activity = failed
            .activities
            .iter()
            .find(|activity| activity.code == "action_ack_timeout")
            .unwrap();

        persist_metrics(&store, failed_activity, timestamp + 1).unwrap();

        let snapshot = store
            .device_snapshot(
                &DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456").unwrap(),
                timestamp + 1,
            )
            .unwrap();
        assert_eq!(snapshot.logs.len(), 2);
        assert_eq!(snapshot.logs[0].kind, "action_failed");
        assert_eq!(snapshot.logs[0].action_kind.as_deref(), Some("paste"));
        assert_eq!(snapshot.logs[0].detail.as_deref(), Some("device_timeout"));
        assert_eq!(snapshot.logs[1].kind, "action_success");
        assert_eq!(snapshot.logs[1].button_id.as_deref(), Some("A"));
        assert_eq!(snapshot.logs[1].action_kind.as_deref(), Some("paste"));
        drop(store);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn feature_switch_gates_buttons_without_becoming_a_button_trigger() {
        let mut session = DeviceSession::new(runtime_model_with_feature_switch());
        session.ready = true;
        session.hello = Some(HelloCapabilities {
            protocol: ACTION_RUN_PROTOCOL_VERSION,
            controller_family_id: crate::hardware::ESP32S3_FAMILY_ID.into(),
            board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
            firmware_build_id: "test".into(),
            product_version_id: None,
            pins: vec![6, 7],
        });

        let blocked = session.on_message(
            DeviceMessage::State {
                event_id: 1,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            &mut |_| Ok(()),
        );
        let blocked_activity = blocked
            .activities
            .iter()
            .find(|activity| activity.code == "feature_disabled")
            .unwrap();
        assert_eq!(
            blocked_activity
                .feature_disabled_log
                .as_ref()
                .map(|log| log.button_id.as_str()),
            Some("A")
        );
        let path = std::env::temp_dir().join(format!(
            "kivo-feature-disabled-{}-{}.sqlite3",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = MetricsStore::open(&path).unwrap();
        let update = persist_metrics(&store, blocked_activity, 1_720_086_400_000)
            .unwrap()
            .unwrap();
        assert_eq!(update.total_presses, 0);
        assert_eq!(update.logs[0].kind, "feature_disabled");
        assert_eq!(update.logs[0].button_id.as_deref(), Some("A"));
        assert!(blocked.paste_requests.is_empty());

        session.on_message(
            DeviceMessage::State {
                event_id: 2,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Up,
            },
            &mut |_| Ok(()),
        );

        session.on_message(
            DeviceMessage::State {
                event_id: 3,
                input: PhysicalInput::Direct { gpio: 7 },
                state: InputState::Down,
            },
            &mut |_| Ok(()),
        );
        let enabled = session.on_message(
            DeviceMessage::State {
                event_id: 4,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            &mut |_| Ok(()),
        );
        assert!(
            enabled
                .activities
                .iter()
                .any(|activity| activity.code == "action_step_started")
        );
        drop(store);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn feature_switch_state_survives_snapshot_updates_and_clears_on_reconfigure() {
        let runtime = runtime_model_with_feature_switch();
        let mut session = DeviceSession::new(runtime.clone());
        session.ready = true;

        session.on_message(
            DeviceMessage::State {
                event_id: 1,
                input: PhysicalInput::Direct { gpio: 7 },
                state: InputState::Down,
            },
            &mut |_| Ok(()),
        );
        assert_eq!(
            session.feature_switch_states.get("mode"),
            Some(&SwitchState::Closed)
        );

        let mut updated = runtime.clone();
        updated.profile.profile.name = "Updated".into();
        session.update_snapshot(Some(Arc::new(updated)));
        assert_eq!(
            session.feature_switch_states.get("mode"),
            Some(&SwitchState::Closed)
        );

        session.reconfigure(Some(Arc::new(runtime)), 2);
        assert!(session.feature_switch_states.is_empty());
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
        drop(store);
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
        drop(store);
        std::fs::remove_file(path).unwrap();
    }

    fn hello() -> DeviceMessage {
        DeviceMessage::Hello(HelloCapabilities {
            protocol: 4,
            controller_family_id: crate::hardware::ESP32S3_FAMILY_ID.into(),
            board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
            firmware_build_id: "test".into(),
            product_version_id: None,
            pins: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18],
        })
    }

    #[test]
    fn protocol_v3_non_oled_profile_still_configures() {
        let mut session = DeviceSession::new(runtime_model());
        let DeviceMessage::Hello(mut legacy_hello) = hello() else {
            unreachable!();
        };
        legacy_hello.protocol = 3;

        let configuring = session.on_message_deferred(DeviceMessage::Hello(legacy_hello), 0, 100);

        assert_eq!(configuring.lines.first().unwrap(), "CONFIG_BEGIN 1 30\n");
        assert_eq!(configuring.lines.last().unwrap(), "CONFIG_COMMIT 1\n");
        assert!(configuring.activities.is_empty());
        let configured =
            session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);
        assert_eq!(configured.activities[0].code, "topology_active");
        assert!(session.ready);
    }

    fn configuration_rejection(device_code: &str) -> RuntimeActivity {
        let mut session = DeviceSession::new(runtime_model());
        let configuring = session.on_message_deferred(hello(), 0, 100);
        assert_eq!(configuring.lines.first().unwrap(), "CONFIG_BEGIN 1 30\n");

        let rejected = session.on_message_deferred(
            DeviceMessage::ConfigError {
                revision: 1,
                code: device_code.into(),
            },
            0,
            101,
        );

        assert_eq!(rejected.activities.len(), 1);
        rejected.activities.into_iter().next().unwrap()
    }

    #[test]
    fn known_firmware_configuration_error_codes_are_preserved() {
        for code in [
            "invalid_begin",
            "invalid_direct",
            "invalid_matrix",
            "invalid_oled",
            "invalid_commit",
            "invalid_learning",
            "invalid_learning_revision",
        ] {
            let activity = configuration_rejection(code);
            assert_eq!(
                activity.params.get("deviceCode").map(String::as_str),
                Some(code)
            );
        }
    }

    #[test]
    fn unknown_firmware_configuration_error_is_sanitized_before_runtime_activity() {
        let secret = "/Users/alice/private/firmware.txt?token=secret-config-123";

        let activity = configuration_rejection(secret);

        assert_eq!(activity.code, "topology_rejected");
        assert_eq!(
            activity.params.get("deviceCode").map(String::as_str),
            Some("device_configuration_error")
        );
        let activity_fields = format!("{activity:?}");
        let event = RuntimeEvent {
            timestamp_ms: 101,
            level: EventLevel::Error,
            device_id: DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456")
                .unwrap(),
            raw_serial: "ABCDEF123456".into(),
            controller_family_id: crate::hardware::ESP32S3_FAMILY_ID.into(),
            board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
            port: None,
            device_profile_id: Some("phone".into()),
            hardware_profile_id: Some("esp-primary".into()),
            home_update: None,
            activity,
        };
        let entry = crate::runtime_log::runtime_event_entry(&event).unwrap();
        let serialized = crate::runtime_log::serialize_entry(&entry).unwrap();

        for private_value in [secret, "/Users/alice", "secret-config-123"] {
            assert!(!activity_fields.contains(private_value));
            assert!(!serialized.contains(private_value));
        }
    }

    #[test]
    fn protocol_v4_rejects_profiles_that_use_advanced_actions() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![ButtonAction::Delay { duration_ms: 200 }]),
        );
        let mut session = DeviceSession::new(runtime);

        let rejected = session.on_message_deferred(hello(), 0, 100);

        assert!(rejected.lines.is_empty());
        assert_eq!(rejected.activities[0].code, "firmware_update_required");
        assert_eq!(
            rejected.activities[0]
                .params
                .get("expected")
                .map(String::as_str),
            Some("5")
        );
    }

    #[test]
    fn protocol_v3_oled_profile_is_rejected_before_configuration() {
        let board = crate::hardware::board_by_id(crate::hardware::YD_RP2040_BOARD_ID).unwrap();
        let mut session = DeviceSession::without_model(board);
        session.update_snapshot(Some(Arc::new(oled_runtime_model())));
        let legacy_hello = HelloCapabilities {
            protocol: 3,
            controller_family_id: board.family_id.into(),
            board_profile_id: board.id.into(),
            firmware_build_id: "legacy".into(),
            product_version_id: None,
            pins: board.safe_pins.to_vec(),
        };

        let rejected = session.on_message_deferred(DeviceMessage::Hello(legacy_hello), 0, 100);

        assert!(rejected.lines.is_empty());
        assert_eq!(session.hello.as_ref().map(|hello| hello.protocol), Some(3));
        assert!(!session.ready);
        assert_eq!(rejected.activities[0].code, "protocol_mismatch");
        assert_eq!(
            rejected.activities[0]
                .params
                .get("expected")
                .map(String::as_str),
            Some("4")
        );
        assert_eq!(
            rejected.activities[0]
                .params
                .get("actual")
                .map(String::as_str),
            Some("3")
        );
    }

    #[test]
    fn unassignment_clears_topology_and_stays_inactive_after_ack() {
        let mut session = DeviceSession::new(runtime_model());
        session.on_message_deferred(hello(), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);
        assert!(session.ready);

        let clearing = session.reconfigure(None, 2);

        assert_eq!(clearing.lines, ["CONFIG_BEGIN 2 30\n", "CONFIG_COMMIT 2\n"]);
        assert!(!session.ready);
        let cleared = session.on_message_deferred(DeviceMessage::ConfigOk { revision: 2 }, 0, 102);
        assert_eq!(cleared.activities[0].code, "topology_cleared");
        assert_eq!(
            cleared.activities[0]
                .params
                .get("revision")
                .map(String::as_str),
            Some("2")
        );
        assert!(!session.ready);
        assert!(session.profile.is_none());
    }

    #[test]
    fn unassigned_hello_waits_for_coordinator_revision_before_clearing_topology() {
        let board = crate::hardware::board_by_id(crate::hardware::YD_ESP32_S3_BOARD_ID).unwrap();
        let mut session = DeviceSession::without_model(board);

        let hello = session.on_message_deferred(hello(), 0, 100);

        assert!(hello.lines.is_empty());
        assert!(hello.activities.is_empty());
        assert!(session.hello.is_some());
        assert!(session.configuring.is_none());
        assert!(!session.ready);
        assert!(session.profile.is_none());

        let clearing = session.reconfigure(None, 1);
        assert_eq!(clearing.lines, ["CONFIG_BEGIN 1 30\n", "CONFIG_COMMIT 1\n"]);
        assert!(clearing.activities.is_empty());
        let cleared = session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);
        assert_eq!(cleared.activities[0].code, "topology_cleared");
        assert!(!session.ready);
        assert!(session.profile.is_none());
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

        let second = session.on_message(DeviceMessage::Done { run_id: 9, step: 1 }, &mut copy);
        assert_eq!(copied.borrow().as_slice(), ["第一步", "第二步"]);
        assert!(second.lines[0].contains(" 9 2 2"));
        let next_press = session.on_message(DeviceMessage::Done { run_id: 9, step: 2 }, &mut copy);
        assert_eq!(copied.borrow().as_slice(), ["第一步", "第二步", "第一步"]);
        assert!(next_press.lines[0].contains(" 10 1 2"));
    }

    #[test]
    fn protocol_v6_dispatches_host_runs_with_chord_commands() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![ButtonAction::Hotkey {
                keys: vec!["ctrl".into(), "a".into()],
            }]),
        );
        let mut session = DeviceSession::new(runtime);
        let DeviceMessage::Hello(mut capabilities) = hello() else {
            unreachable!();
        };
        capabilities.protocol = 6;
        session.on_message_deferred(DeviceMessage::Hello(capabilities), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);

        let first = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 41,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            41,
            102,
        );
        assert_eq!(first.lines, ["CHORD 1 1 1 1 1 4\n"]);
        assert_eq!(first.activities[2].params["runId"], "1");

        session.on_message_deferred(DeviceMessage::Done { run_id: 1, step: 1 }, 0, 103);
        session.on_message_deferred(
            DeviceMessage::State {
                event_id: 42,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Up,
            },
            42,
            104,
        );
        let second = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 99,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            99,
            105,
        );
        assert_eq!(second.lines, ["CHORD 2 1 1 1 1 4\n"]);
        assert_eq!(second.activities[3].params["runId"], "2");
    }

    fn protocol6_hello() -> DeviceMessage {
        let DeviceMessage::Hello(mut capabilities) = hello() else {
            unreachable!();
        };
        capabilities.protocol = 6;
        DeviceMessage::Hello(capabilities)
    }

    #[test]
    fn v6_pickup_and_hangup_queue_distinct_host_runs() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions {
                press: vec![ButtonAction::Open {
                    target: "pickup".into(),
                }],
                release: vec![ButtonAction::Media {
                    command: crate::profile::MediaCommand::PlayPause,
                }],
                ..TriggerActions::default()
            },
        );
        let mut session = DeviceSession::new(runtime);
        session.on_message_deferred(protocol6_hello(), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);

        session.target_opener = Arc::new(RecordingTargetOpener {
            targets: Arc::new(Mutex::new(Vec::new())),
            error: None,
        });
        let down = session.on_line_deferred("STATE 91 DIRECT 6 DOWN\n", 1, 100);
        assert!(down.lines[0].starts_with("HOST 1 1 1"));
        let done = session.on_line_deferred("DONE 1 1\n", 2, 101);
        assert!(done.activities.iter().any(|activity| {
            activity.code == "action_step_completed"
                && activity.params.get("runId").map(String::as_str) == Some("1")
        }));

        let up = session.on_line_deferred("STATE 92 DIRECT 6 UP\n", 3, 200);
        assert!(up.lines[0].starts_with("MEDIA 2 1 1"));
    }

    #[test]
    fn duplicate_hello_aborts_active_and_queued_runs_once() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions {
                press: vec![ButtonAction::Hotkey {
                    keys: vec!["a".into()],
                }],
                double_press: vec![ButtonAction::Hotkey {
                    keys: vec!["b".into()],
                }],
                ..TriggerActions::default()
            },
        );
        let mut session = DeviceSession::new(runtime);
        session.on_message_deferred(protocol6_hello(), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);
        session.on_line_deferred("STATE 1 DIRECT 6 DOWN\n", 1, 0);
        session.on_line_deferred("STATE 2 DIRECT 6 UP\n", 2, 10);
        session.on_line_deferred("STATE 3 DIRECT 6 DOWN\n", 3, 20);

        let restarted = session.on_message_deferred(protocol6_hello(), 4, 21);
        assert!(restarted.lines.iter().any(|line| line == "SKIP 1\n"));
        let completed = restarted
            .completed_receive_sequences
            .iter()
            .copied()
            .filter(|sequence| *sequence != 0)
            .collect::<Vec<_>>();
        let unique = completed.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(completed.len(), unique.len());
        assert_eq!(unique, BTreeSet::from([1, 2, 3]));
        assert!(session.active.is_none());
        assert!(session.queue.is_empty());
    }

    #[test]
    fn timer_poll_fires_long_press_without_serial_input() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions {
                long_press: vec![ButtonAction::Open {
                    target: "hold".into(),
                }],
                ..TriggerActions::default()
            },
        );
        let mut session = DeviceSession::new(runtime);
        session.on_message_deferred(protocol6_hello(), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);
        session.target_opener = Arc::new(RecordingTargetOpener {
            targets: Arc::new(Mutex::new(Vec::new())),
            error: None,
        });

        let down = session.on_line_deferred("STATE 5 DIRECT 6 DOWN\n", 1, 0);
        assert!(down.lines.is_empty());
        assert!(session.poll_triggers(499).lines.is_empty());
        let long = session.poll_triggers(500);
        assert!(long.activities.iter().any(|activity| {
            activity.code == "trigger_occurred"
                && activity.params.get("trigger").map(String::as_str) == Some("long_press")
        }));
        assert!(long.lines[0].starts_with("HOST 1 1 1"));
    }

    #[test]
    fn long_press_only_paste_keeps_down_sequence_until_release() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions {
                long_press: vec![ButtonAction::Paste {
                    text: "hold".into(),
                }],
                ..TriggerActions::default()
            },
        );
        let mut session = DeviceSession::new(runtime);
        session.on_message_deferred(protocol6_hello(), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);

        let down = session.on_line_deferred("STATE 1 DIRECT 6 DOWN\n", 1, 0);
        assert!(down.completed_receive_sequences.is_empty());
        let long = session.poll_triggers(500);
        assert_eq!(long.paste_requests[0].receive_sequence, 1);
        assert!(long.completed_receive_sequences.is_empty());
        session.grant_paste(1, 1);
        let completed_action = session.on_line_deferred("DONE 1 1\n", 2, 501);
        assert!(completed_action.completed_receive_sequences.is_empty());

        let release = session.on_line_deferred("STATE 2 DIRECT 6 UP\n", 3, 600);
        assert_eq!(
            release
                .completed_receive_sequences
                .iter()
                .filter(|sequence| **sequence == 1)
                .count(),
            1
        );
    }

    #[test]
    fn legacy_v5_uses_event_id_for_down_and_ignores_up_actions() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![ButtonAction::Hotkey {
                keys: vec!["a".into()],
            }]),
        );
        let mut session = DeviceSession::new(runtime);
        let DeviceMessage::Hello(mut capabilities) = hello() else {
            unreachable!();
        };
        capabilities.protocol = 5;
        session.on_message_deferred(DeviceMessage::Hello(capabilities), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);

        let down = session.on_line_deferred("STATE 77 DIRECT 6 DOWN\n", 1, 102);
        assert_eq!(down.lines, ["HOTKEY 77 1 1 0 4\n"]);
        let up = session.on_line_deferred("STATE 78 DIRECT 6 UP\n", 2, 103);
        assert!(up.lines.is_empty());
        session.on_line_deferred("DONE 77 1\n", 3, 104);
    }

    #[test]
    fn malformed_ack_aborts_only_active_v6_run_and_starts_queued_occurrence() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![ButtonAction::Hotkey {
                keys: vec!["a".into()],
            }]),
        );
        let mut session = DeviceSession::new(runtime);
        session.on_message_deferred(protocol6_hello(), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);
        session.on_line_deferred("STATE 1 DIRECT 6 DOWN\n", 1, 0);
        session.on_line_deferred("STATE 2 DIRECT 6 UP\n", 2, 10);
        session.on_line_deferred("STATE 3 DIRECT 6 DOWN\n", 3, 20);

        let recovered = session.on_line_deferred("DONE 999 1\n", 4, 21);
        assert!(recovered.lines.iter().any(|line| line == "SKIP 1\n"));
        assert!(
            recovered
                .lines
                .iter()
                .any(|line| line.starts_with("CHORD 2 1 1"))
        );
        assert!(
            recovered
                .activities
                .iter()
                .any(|activity| activity.code == "invalid_action_acknowledgement")
        );
    }

    #[test]
    fn timeout_aborts_active_run_and_preserves_the_queued_trigger() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![ButtonAction::Hotkey {
                keys: vec!["a".into()],
            }]),
        );
        let mut session = DeviceSession::new(runtime);
        session.on_message_deferred(protocol6_hello(), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);
        session.on_line_deferred("STATE 1 DIRECT 6 DOWN\n", 1, 0);
        session.on_line_deferred("STATE 2 DIRECT 6 UP\n", 2, 10);
        session.on_line_deferred("STATE 3 DIRECT 6 DOWN\n", 3, 20);

        let recovered = session.on_action_timeout(1, 2_000);
        assert!(recovered.lines.iter().any(|line| line == "SKIP 1\n"));
        assert!(
            recovered
                .lines
                .iter()
                .any(|line| line.starts_with("CHORD 2 1 1"))
        );
        assert!(
            recovered
                .activities
                .iter()
                .any(|activity| activity.code == "action_timeout")
        );
    }

    #[test]
    fn reconfigure_resets_held_v6_input_before_long_press_deadline() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions {
                long_press: vec![ButtonAction::Hotkey {
                    keys: vec!["a".into()],
                }],
                ..TriggerActions::default()
            },
        );
        let mut session = DeviceSession::new(runtime.clone());
        session.on_message_deferred(protocol6_hello(), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);
        session.on_line_deferred("STATE 1 DIRECT 6 DOWN\n", 1, 0);
        let reset = session.reconfigure(Some(Arc::new(runtime)), 2);
        assert_eq!(reset.completed_receive_sequences, [1]);
        assert!(session.poll_triggers(500).lines.is_empty());
        assert!(
            session
                .poll_triggers(500)
                .activities
                .iter()
                .all(|activity| activity.code != "trigger_occurred")
        );
    }

    #[test]
    fn snapshot_update_preserves_pending_press_and_double_sequence_bookkeeping() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions {
                press: vec![ButtonAction::Hotkey {
                    keys: vec!["a".into()],
                }],
                double_press: vec![ButtonAction::Hotkey {
                    keys: vec!["b".into()],
                }],
                ..TriggerActions::default()
            },
        );
        let mut updated = runtime.clone();
        updated.profile.profile.name = "Updated".into();
        let mut session = DeviceSession::new(runtime);
        session.on_message_deferred(protocol6_hello(), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);

        session.on_line_deferred("STATE 1 DIRECT 6 DOWN\n", 1, 0);
        session.on_line_deferred("STATE 2 DIRECT 6 UP\n", 2, 10);
        session.on_line_deferred("DONE 1 1\n", 3, 11);
        let second = session.on_line_deferred("STATE 3 DIRECT 6 DOWN\n", 4, 20);
        assert!(
            second
                .lines
                .iter()
                .any(|line| line.starts_with("CHORD 2 1 1"))
        );

        session.update_snapshot(Some(Arc::new(updated)));
        let double = session.on_line_deferred("DONE 2 1\n", 5, 21);
        assert!(
            double
                .lines
                .iter()
                .any(|line| line.starts_with("CHORD 3 1 1"))
        );
        assert!(double.completed_receive_sequences.is_empty());
        let completed = session.on_line_deferred("DONE 3 1\n", 6, 22);
        assert!(completed.completed_receive_sequences.is_empty());
        let release = session.on_line_deferred("STATE 4 DIRECT 6 UP\n", 7, 23);
        assert_eq!(
            release
                .completed_receive_sequences
                .iter()
                .filter(|sequence| **sequence == 4)
                .count(),
            1
        );
    }

    #[test]
    fn deferred_paste_action_activity_is_sanitized_and_advances_in_order() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![
                ButtonAction::Paste {
                    text: "甲乙丙".into(),
                },
                ButtonAction::Hotkey {
                    keys: vec!["primary".into(), "enter".into()],
                },
            ]),
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
        let started = &pending.activities[1];
        assert_eq!(started.code, "action_step_started");
        assert_eq!(
            started.params,
            BTreeMap::from([
                ("runId".into(), "41".into()),
                ("button".into(), "A".into()),
                ("step".into(), "1".into()),
                ("total".into(), "2".into()),
                ("actionKind".into(), "paste".into()),
                ("characterCount".into(), "3".into()),
            ])
        );
        let activity_debug = format!("{started:?}");
        let activity_json = serde_json::to_string(started).unwrap();
        assert!(!activity_debug.contains("甲乙丙"));
        assert!(!activity_json.contains("甲乙丙"));
        assert_eq!(
            pending.paste_requests,
            vec![PendingPaste {
                receive_sequence: 7,
                event_id: 41,
                step: 1,
                total: 2,
                text: "甲乙丙".into(),
                context: None,
            }]
        );

        let granted = session.grant_paste(41, 1);
        assert_eq!(granted.lines, [format_paste_command(41, 1, 2)]);
        let next = session.on_message_deferred(
            DeviceMessage::Done {
                run_id: 41,
                step: 1,
            },
            0,
            124,
        );
        let primary_modifier = if cfg!(target_os = "macos") { 8 } else { 1 };
        assert_eq!(
            next.lines,
            [format!("HOTKEY 41 2 2 {primary_modifier} 40\n")]
        );
        assert!(next.paste_requests.is_empty());
        assert_eq!(next.activities.len(), 2);
        assert_eq!(next.activities[0].code, "action_step_completed");
        assert_eq!(next.activities[0].params["actionKind"], "paste");
        assert_eq!(next.activities[0].params["step"], "1");
        assert_eq!(next.activities[1].code, "action_step_started");
        assert_eq!(next.activities[1].params["actionKind"], "hotkey");
        assert_eq!(next.activities[1].params["keys"], "primary+enter");
    }

    #[test]
    fn protocol_v6_device_session_runs_deferred_paste_delay_and_media_in_one_host_run() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![
                ButtonAction::Paste {
                    text: "甲乙丙".into(),
                },
                ButtonAction::Delay { duration_ms: 500 },
                ButtonAction::Media {
                    command: crate::profile::MediaCommand::PlayPause,
                },
            ]),
        );
        let mut session = DeviceSession::new(runtime);
        session.on_message_deferred(protocol6_hello(), 0, 100);
        session.on_message_deferred(DeviceMessage::ConfigOk { revision: 1 }, 0, 101);

        let pending = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 700,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            91,
            102,
        );
        assert!(pending.lines.is_empty());
        assert_eq!(pending.paste_requests.len(), 1);
        let request = &pending.paste_requests[0];
        assert_eq!(request.receive_sequence, 91);
        assert_eq!(request.event_id, 1);
        assert_eq!(request.step, 1);
        assert_eq!(request.total, 3);
        assert_eq!(request.text, "甲乙丙");

        let mismatched_grant = session.grant_paste(700, 1);
        assert!(mismatched_grant.lines.is_empty());
        assert_eq!(mismatched_grant.activities[0].code, "paste_grant_mismatch");

        let paste = session.grant_paste(1, 1);
        assert_eq!(paste.lines, [format_paste_command(1, 1, 3)]);

        let delay = session.on_message_deferred(DeviceMessage::Done { run_id: 1, step: 1 }, 0, 103);
        assert_eq!(delay.lines, ["DELAY 1 2 3 500\n"]);
        assert_eq!(
            delay.action_timeout,
            Some(ACTION_ACK_TIMEOUT + Duration::from_millis(500)),
        );

        let media = session.on_message_deferred(DeviceMessage::Done { run_id: 1, step: 2 }, 0, 604);
        assert_eq!(media.lines, ["MEDIA 1 3 3 205\n"]);

        let completed =
            session.on_message_deferred(DeviceMessage::Done { run_id: 1, step: 3 }, 0, 605);
        assert!(completed.lines.is_empty());
        assert!(completed.activities.iter().any(|activity| {
            activity.code == "action_step_completed"
                && activity.params.get("runId").map(String::as_str) == Some("1")
                && activity.params.get("step").map(String::as_str) == Some("3")
        }));
        assert!(session.active.is_none());
    }

    struct FailingClipboard;

    impl ClipboardWriter for FailingClipboard {
        fn write(&self, _text: &str) -> Result<(), String> {
            Err("clipboard unavailable".into())
        }
    }

    struct EchoClipboard;

    impl ClipboardWriter for EchoClipboard {
        fn write(&self, text: &str) -> Result<(), String> {
            Err(text.into())
        }
    }

    #[test]
    fn immediate_paste_failure_redacts_clipboard_error() {
        let secret = "甲乙丙";
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![ButtonAction::Paste {
                text: secret.into(),
            }]),
        );
        let mut session = DeviceSession::new(runtime);
        session.ready = true;

        let output = session.on_message(
            DeviceMessage::State {
                event_id: 46,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            &mut |text| Err(text.into()),
        );

        let failed = output
            .activities
            .iter()
            .find(|activity| activity.code == "action_step_failed")
            .unwrap();
        assert_eq!(failed.detail.as_deref(), Some("clipboard_write_failed"));
        let activity_debug = format!("{:?}", output.activities);
        let activity_json = serde_json::to_string(&output.activities).unwrap();
        assert!(!activity_debug.contains(secret));
        assert!(!activity_json.contains(secret));
    }

    #[test]
    fn unrelated_receive_sequence_completion_preserves_active_action_deadline() {
        let runtime = runtime_model();
        let device_id = runtime.metric_attribution.device_id.clone();
        let start = WorkerStart {
            generation: 1,
            device_id,
            port: "/dev/action-deadline".into(),
            board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
        };
        let paste = PasteCoordinator::with_timeout(FailingClipboard, Duration::from_millis(100));
        let (events, _received_events) = mpsc::channel();
        let mut writer = Vec::new();
        let mut pending_paste = None;
        let mut action_deadline = None;
        let context = RuntimeEventContext::from_snapshot(1, Some(&runtime));
        let clock = SystemClock::default();
        let stop = AtomicBool::new(false);
        let barrier = RwLock::new(());
        let mut action = SessionOutput::default();
        action.lines.push("CHORD 10 1 1 0 1 40\n".into());

        write_isolated_output(
            &start,
            &events,
            &paste.handle(),
            None,
            &barrier,
            &mut writer,
            action,
            &mut pending_paste,
            &mut action_deadline,
            &context,
            &clock,
            &stop,
        )
        .unwrap();
        let active_deadline = action_deadline;
        assert!(active_deadline.is_some());

        let mut release = SessionOutput::default();
        release.completed_receive_sequences.push(2);
        write_isolated_output(
            &start,
            &events,
            &paste.handle(),
            None,
            &barrier,
            &mut writer,
            release,
            &mut pending_paste,
            &mut action_deadline,
            &context,
            &clock,
            &stop,
        )
        .unwrap();

        assert_eq!(action_deadline, active_deadline);
        paste.shutdown();
    }

    #[test]
    fn deferred_paste_failure_keeps_captured_context_after_overtaking_reconfigure() {
        let secret = "甲乙丙";
        let mut old = runtime_model();
        old.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![ButtonAction::Paste {
                text: secret.into(),
            }]),
        );
        let old = Arc::new(old);
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
        assert_eq!(configured.paste_requests[0].text, secret);

        let paste = PasteCoordinator::with_timeout(EchoClipboard, Duration::from_millis(100));
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
        assert_eq!(reply, PasteReply::ClipboardError(secret.into()));
        pending_paste = None;
        let failed = session.fail_active_deferred("action_step_failed", Some(secret.into()));
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
        let activities = received_events
            .try_iter()
            .filter_map(|event| match event {
                WorkerEvent::Activity {
                    device_id,
                    context,
                    activity,
                    ..
                } => Some((device_id, context, activity)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let (actual_device, context, failed) = activities
            .iter()
            .find(|(_, _, activity)| activity.code == "action_step_failed")
            .unwrap();

        assert_eq!(*actual_device, device_id);
        assert_eq!(context.device_profile_id.as_deref(), Some("phone"));
        assert_eq!(context.hardware_profile_id.as_deref(), Some("esp-primary"));
        assert_eq!(context.port.as_deref(), Some("/dev/old-captured"));
        assert_eq!(failed.detail.as_deref(), Some("clipboard_write_failed"));
        for (_, _, activity) in &activities {
            let activity_debug = format!("{activity:?}");
            let activity_json = serde_json::to_string(activity).unwrap();
            assert!(!activity_debug.contains(secret));
            assert!(!activity_json.contains(secret));
        }
        paste.shutdown();
    }

    #[test]
    fn hotkey_only_action_bypasses_global_paste_coordinator() {
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![ButtonAction::Hotkey {
                keys: vec!["enter".into()],
            }]),
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
    fn advanced_actions_preserve_order_and_open_through_the_host() {
        let targets = Arc::new(Mutex::new(Vec::new()));
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![
                ButtonAction::Open {
                    target: "https://example.com".into(),
                },
                ButtonAction::Delay { duration_ms: 200 },
                ButtonAction::Media {
                    command: crate::profile::MediaCommand::PlayPause,
                },
            ]),
        );
        let mut session = DeviceSession::new(runtime);
        session.target_opener = Arc::new(RecordingTargetOpener {
            targets: Arc::clone(&targets),
            error: None,
        });
        session.ready = true;

        let first = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 43,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            9,
            125,
        );
        assert_eq!(first.lines, ["HOST 43 1 3\n"]);
        assert_eq!(targets.lock().unwrap().as_slice(), ["https://example.com"]);

        let second = session.on_message_deferred(
            DeviceMessage::Done {
                run_id: 43,
                step: 1,
            },
            0,
            126,
        );
        assert_eq!(second.lines, ["DELAY 43 2 3 200\n"]);
        assert_eq!(
            second.action_timeout,
            Some(ACTION_ACK_TIMEOUT + Duration::from_millis(200))
        );

        let third = session.on_message_deferred(
            DeviceMessage::Done {
                run_id: 43,
                step: 2,
            },
            0,
            127,
        );
        assert_eq!(third.lines, ["MEDIA 43 3 3 205\n"]);
        assert_eq!(third.activities[1].code, "action_step_started");
        assert_eq!(third.activities[1].params["actionKind"], "media");
        assert_eq!(third.activities[1].params["command"], "play_pause");
        assert!(!third.activities[1].params.contains_key("mediaCommand"));
    }

    #[test]
    fn open_action_activity_redacts_target() {
        let target = "https://example.test/private?token=secret";
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![ButtonAction::Open {
                target: target.into(),
            }]),
        );
        let mut session = DeviceSession::new(runtime);
        session.target_opener = Arc::new(RecordingTargetOpener {
            targets: Arc::new(Mutex::new(Vec::new())),
            error: None,
        });
        session.ready = true;

        let output = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 45,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            11,
            129,
        );

        let started = &output.activities[1];
        assert_eq!(started.code, "action_step_started");
        assert_eq!(started.params["actionKind"], "open");
        assert_eq!(started.params["targetKind"], "url");
        assert_eq!(
            started.params["characterCount"],
            target.chars().count().to_string()
        );
        let activity_debug = format!("{started:?}");
        let activity_json = serde_json::to_string(started).unwrap();
        for private_value in ["private", "secret", target] {
            assert!(!activity_debug.contains(private_value));
            assert!(!activity_json.contains(private_value));
        }
    }

    #[test]
    fn failed_open_aborts_only_the_current_action_sequence() {
        let target = "https://example.test/private?token=secret";
        let mut runtime = runtime_model();
        runtime.profile.actions.insert(
            "A".into(),
            TriggerActions::press(vec![ButtonAction::Open {
                target: target.into(),
            }]),
        );
        let mut session = DeviceSession::new(runtime);
        session.target_opener = Arc::new(RecordingTargetOpener {
            targets: Arc::new(Mutex::new(Vec::new())),
            error: Some(target.into()),
        });
        session.ready = true;

        let output = session.on_message_deferred(
            DeviceMessage::State {
                event_id: 44,
                input: PhysicalInput::Direct { gpio: 6 },
                state: InputState::Down,
            },
            10,
            128,
        );

        assert_eq!(output.lines, ["SKIP 44\n"]);
        assert_eq!(output.activities[1].code, "action_step_started");
        let failed = &output.activities[2];
        assert_eq!(failed.code, "action_step_failed");
        assert_eq!(failed.detail.as_deref(), Some("open_target_failed"));
        let activity_debug = format!("{:?}", output.activities);
        let activity_json = serde_json::to_string(&output.activities).unwrap();
        for private_value in [target, "private", "secret"] {
            assert!(!activity_debug.contains(private_value));
            assert!(!activity_json.contains(private_value));
        }
        assert_eq!(output.completed_receive_sequences, [10]);
        assert!(session.active.is_none());
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
            TriggerActions::press(vec![ButtonAction::Paste {
                text: "新动作".into(),
            }]),
        );
        let swapped = session.update_snapshot(Some(Arc::new(updated)));
        assert!(swapped.lines.is_empty());

        session.grant_paste(50, 1);
        let second = session.on_message_deferred(
            DeviceMessage::Done {
                run_id: 50,
                step: 1,
            },
            0,
            101,
        );
        assert_eq!(second.paste_requests[0].text, "第二步");
        session.grant_paste(50, 2);
        session.on_message_deferred(
            DeviceMessage::Done {
                run_id: 50,
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
                run_id: 60,
                step: 1,
            },
            0,
            104,
        );
        assert_eq!(old_second.paste_requests[0].text, "第二步");
        session.grant_paste(60, 2);
        let configured = session.on_message_deferred(
            DeviceMessage::Done {
                run_id: 60,
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
            device_id: DeviceId::new(crate::hardware::YD_ESP32_S3_BOARD_ID, "ABCDEF123456")
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
                protocol: 4,
                controller_family_id: crate::hardware::ESP32S3_FAMILY_ID.into(),
                board_profile_id: crate::hardware::YD_ESP32_S3_BOARD_ID.into(),
                firmware_build_id: "test".into(),
                product_version_id: None,
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

        session.on_line("HELLO 4 esp32s3 yd-esp32-s3 build 2 0", &mut copy);
        assert!(!session.ready);
        assert!(session.hello.is_none());

        session.on_message(DeviceMessage::Hello(hello.clone()), &mut copy);
        session.on_message(DeviceMessage::ConfigOk { revision: 3 }, &mut copy);
        let _ = hello;
        session.on_line("HELLO 4 esp32s3 yd-rp2040 build 2 0 6", &mut copy);
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
