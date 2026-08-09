use crate::{
    coordinator::{
        AssignmentDimension, CandidateIssue, CandidateStatus, ConnectionDimension, DeviceMode,
        DeviceStatus, EventLevel, IdentityDimension, RuntimeDimension, RuntimeEvent,
    },
    hardware::DeviceId,
    workspace::RuntimeAssignment,
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri_plugin_log::{
    Builder, RotationStrategy, Target, TargetKind,
    log::{self, LevelFilter},
};

pub(crate) const LOG_TARGET: &str = "kivo::runtime";
pub(crate) const MAX_FILE_SIZE: u128 = 10 * 1024 * 1024;
pub(crate) const RETAINED_FILES: usize = 5;
const LOG_QUEUE_CAPACITY: usize = 1024;

static DISPATCHER: OnceLock<LogDispatcher> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RuntimeLogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeLogEntry {
    timestamp_ms: u64,
    level: RuntimeLogLevel,
    event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    context: Value,
}

impl RuntimeLogEntry {
    pub(crate) fn new(
        timestamp_ms: u64,
        level: RuntimeLogLevel,
        event: impl Into<String>,
        context: Value,
    ) -> Self {
        Self {
            timestamp_ms,
            level,
            event: event.into(),
            result: None,
            detail: None,
            context,
        }
    }

    pub(crate) fn with_result(mut self, result: impl Into<String>) -> Self {
        self.result = Some(result.into());
        self
    }

    pub(crate) fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

pub(crate) fn log_directory(config_directory: &Path) -> PathBuf {
    config_directory.join("data/log")
}

pub(crate) fn serialize_entry(entry: &RuntimeLogEntry) -> serde_json::Result<String> {
    serde_json::to_string(entry)
}

pub(crate) fn metrics_initialization_failure_detail(error: &rusqlite::Error) -> String {
    match error {
        rusqlite::Error::SqliteFailure(error, _) => {
            format!("sqlite_failure:{}", error.extended_code)
        }
        rusqlite::Error::InvalidPath(_) => "invalid_path".into(),
        _ => "database_error".into(),
    }
}

pub(crate) fn emit(entry: RuntimeLogEntry) {
    if let Some(entry) = queued_entry(entry)
        && let Some(dispatcher) = DISPATCHER.get()
    {
        let _ = dispatcher.try_enqueue(entry);
    }
}

pub(crate) fn emit_priority(entry: RuntimeLogEntry) {
    if let Some(entry) = queued_entry(entry)
        && let Some(dispatcher) = DISPATCHER.get()
    {
        let _ = dispatcher.enqueue_priority(entry);
    }
}

pub(crate) fn emit_lifecycle(entry: RuntimeLogEntry) {
    emit_priority(entry);
}

pub(crate) fn operation<T>(
    timestamp_ms: u64,
    event: &str,
    context: Value,
    action: impl FnOnce() -> Result<T, crate::workspace::AppError>,
) -> Result<T, crate::workspace::AppError> {
    let result = action();
    emit_priority(operation_entry(timestamp_ms, event, context, &result));
    result
}

fn operation_entry<T>(
    timestamp_ms: u64,
    event: &str,
    context: Value,
    result: &Result<T, crate::workspace::AppError>,
) -> RuntimeLogEntry {
    match result {
        Ok(_) => RuntimeLogEntry::new(timestamp_ms, RuntimeLogLevel::Info, event, context)
            .with_result("succeeded"),
        Err(error) => RuntimeLogEntry::new(timestamp_ms, RuntimeLogLevel::Error, event, context)
            .with_result("failed")
            .with_detail(error.code.clone()),
    }
}

fn queued_entry(entry: RuntimeLogEntry) -> Option<QueuedLogEntry> {
    match serialize_entry(&entry) {
        Ok(line) => Some(QueuedLogEntry {
            level: entry.level,
            line,
        }),
        Err(error) => {
            eprintln!("failed to serialize runtime log entry: {error}");
            None
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct QueuedLogEntry {
    level: RuntimeLogLevel,
    line: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DispatchOutcome {
    Accepted,
    Dropped,
    Stopped,
}

enum WorkerMessage {
    Entry {
        entry: QueuedLogEntry,
        normal_reserved: bool,
    },
    Shutdown,
}

trait LogSink: Send + 'static {
    fn write(&mut self, entry: QueuedLogEntry);
    fn flush(&mut self);
}

struct OfficialLogSink;

impl LogSink for OfficialLogSink {
    fn write(&mut self, entry: QueuedLogEntry) {
        match entry.level {
            RuntimeLogLevel::Info => log::info!(target: LOG_TARGET, "{}", entry.line),
            RuntimeLogLevel::Warning => log::warn!(target: LOG_TARGET, "{}", entry.line),
            RuntimeLogLevel::Error => log::error!(target: LOG_TARGET, "{}", entry.line),
        }
    }

    fn flush(&mut self) {
        log::logger().flush();
    }
}

struct LogDispatcher {
    sender: Mutex<Option<Sender<WorkerMessage>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    normal_capacity: usize,
    normal_queued: Arc<AtomicUsize>,
    dropped_total: Arc<AtomicU64>,
    dropped_unreported: Arc<AtomicU64>,
}

impl LogDispatcher {
    fn start(capacity: usize, sink: impl LogSink) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let normal_queued = Arc::new(AtomicUsize::new(0));
        let dropped_total = Arc::new(AtomicU64::new(0));
        let dropped_unreported = Arc::new(AtomicU64::new(0));
        let worker_normal_queued = Arc::clone(&normal_queued);
        let worker_dropped = Arc::clone(&dropped_unreported);
        let worker = thread::Builder::new()
            .name("runtime-log-writer".into())
            .spawn(move || {
                let mut sink = sink;
                while let Ok(message) = receiver.recv() {
                    match message {
                        WorkerMessage::Entry {
                            entry,
                            normal_reserved,
                        } => {
                            if normal_reserved {
                                worker_normal_queued.fetch_sub(1, Ordering::Release);
                            }
                            sink.write(entry);
                            report_dropped_entries(&mut sink, &worker_dropped);
                        }
                        WorkerMessage::Shutdown => break,
                    }
                }
                report_dropped_entries(&mut sink, &worker_dropped);
                sink.flush();
            })?;
        Ok(Self {
            sender: Mutex::new(Some(sender)),
            worker: Mutex::new(Some(worker)),
            normal_capacity: capacity,
            normal_queued,
            dropped_total,
            dropped_unreported,
        })
    }

    fn try_enqueue(&self, entry: QueuedLogEntry) -> DispatchOutcome {
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(sender) = sender.as_ref() else {
            return DispatchOutcome::Stopped;
        };
        if !self.reserve_normal_slot() {
            self.dropped_total.fetch_add(1, Ordering::Relaxed);
            self.dropped_unreported.fetch_add(1, Ordering::Relaxed);
            return DispatchOutcome::Dropped;
        }
        match sender.send(WorkerMessage::Entry {
            entry,
            normal_reserved: true,
        }) {
            Ok(()) => DispatchOutcome::Accepted,
            Err(_) => {
                self.normal_queued.fetch_sub(1, Ordering::Release);
                DispatchOutcome::Stopped
            }
        }
    }

    fn enqueue_priority(&self, entry: QueuedLogEntry) -> DispatchOutcome {
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(sender) = sender.as_ref() else {
            return DispatchOutcome::Stopped;
        };
        match sender.send(WorkerMessage::Entry {
            entry,
            normal_reserved: false,
        }) {
            Ok(()) => DispatchOutcome::Accepted,
            Err(_) => DispatchOutcome::Stopped,
        }
    }

    fn reserve_normal_slot(&self) -> bool {
        self.normal_queued
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |queued| {
                (queued < self.normal_capacity).then_some(queued + 1)
            })
            .is_ok()
    }

    #[cfg(test)]
    fn dropped_count(&self) -> u64 {
        self.dropped_total.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    fn shutdown(&self) {
        self.finish(None);
    }

    fn shutdown_with_entry(&self, entry: QueuedLogEntry) {
        self.finish(Some(entry));
    }

    fn finish(&self, final_entry: Option<QueuedLogEntry>) {
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(sender) = sender {
            if let Some(entry) = final_entry {
                let _ = sender.send(WorkerMessage::Entry {
                    entry,
                    normal_reserved: false,
                });
            }
            let _ = sender.send(WorkerMessage::Shutdown);
        }
        if let Some(worker) = self
            .worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            let _ = worker.join();
        }
    }
}

fn report_dropped_entries(sink: &mut impl LogSink, dropped: &AtomicU64) {
    let count = dropped.swap(0, Ordering::Relaxed);
    if count == 0 {
        return;
    }
    let entry = RuntimeLogEntry::new(
        current_timestamp_ms(),
        RuntimeLogLevel::Warning,
        "runtime_log_entries_dropped",
        serde_json::json!({"count": count, "policy": "drop_newest"}),
    );
    if let Ok(line) = serialize_entry(&entry) {
        sink.write(QueuedLogEntry {
            level: entry.level,
            line,
        });
    }
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) fn shutdown_with_entry(entry: RuntimeLogEntry) {
    let entry = queued_entry(entry);
    if let Some(dispatcher) = DISPATCHER.get() {
        if let Some(entry) = entry {
            dispatcher.shutdown_with_entry(entry);
        } else {
            dispatcher.finish(None);
        }
    }
}

pub(crate) fn runtime_event_entry(event: &RuntimeEvent) -> serde_json::Result<RuntimeLogEntry> {
    let level = match event.level {
        EventLevel::Info => RuntimeLogLevel::Info,
        EventLevel::Warning => RuntimeLogLevel::Warning,
        EventLevel::Error => RuntimeLogLevel::Error,
    };
    let context = Value::Object(Map::from_iter([
        (
            "deviceId".into(),
            Value::String(event.device_id.as_str().into()),
        ),
        ("rawSerial".into(), Value::String(event.raw_serial.clone())),
        (
            "controllerFamilyId".into(),
            Value::String(event.controller_family_id.clone()),
        ),
        (
            "boardProfileId".into(),
            Value::String(event.board_profile_id.clone()),
        ),
        ("port".into(), optional_string(&event.port)),
        (
            "deviceProfileId".into(),
            optional_string(&event.device_profile_id),
        ),
        (
            "hardwareProfileId".into(),
            optional_string(&event.hardware_profile_id),
        ),
        ("activity".into(), serde_json::to_value(&event.activity)?),
    ]));

    Ok(RuntimeLogEntry::new(
        event.timestamp_ms,
        level,
        event.activity.code.clone(),
        context,
    ))
}

pub(crate) fn emit_runtime_event(event: &RuntimeEvent) {
    match runtime_event_entry(event) {
        Ok(entry) => emit(entry),
        Err(error) => eprintln!("failed to serialize runtime event log entry: {error}"),
    }
}

fn optional_string(value: &Option<String>) -> Value {
    value.clone().map(Value::String).unwrap_or(Value::Null)
}

#[derive(Default)]
pub(crate) struct DeviceLogInventory {
    devices: BTreeMap<DeviceId, DeviceStatus>,
    candidates: BTreeMap<String, CandidateStatus>,
    last_scan_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceLogSnapshot<'a> {
    device_id: &'a str,
    connection: ConnectionDimension,
    mode: Option<DeviceMode>,
    identity: IdentityDimension,
    assignment: AssignmentDimension,
    runtime: RuntimeDimension,
    raw_serial: &'a str,
    port: Option<&'a str>,
    controller_family_id: &'a str,
    board_profile_id: &'a str,
    firmware_build_id: Option<&'a str>,
    runtime_assignment: Option<RuntimeAssignmentLogSnapshot<'a>>,
    learning_active: bool,
    latest_error_code: Option<&'a str>,
}

impl<'a> From<&'a DeviceStatus> for DeviceLogSnapshot<'a> {
    fn from(status: &'a DeviceStatus) -> Self {
        Self {
            device_id: status.device_id.as_str(),
            connection: status.connection,
            mode: status.mode,
            identity: status.identity,
            assignment: status.assignment,
            runtime: status.runtime,
            raw_serial: &status.raw_serial,
            port: status.port.as_deref(),
            controller_family_id: &status.controller_family_id,
            board_profile_id: &status.board_profile_id,
            firmware_build_id: status.firmware_build_id.as_deref(),
            runtime_assignment: status
                .runtime_assignment
                .as_ref()
                .map(RuntimeAssignmentLogSnapshot::from),
            learning_active: status.learning.is_some(),
            latest_error_code: status
                .latest_error
                .as_ref()
                .map(|error| stable_runtime_activity_code(&error.code)),
        }
    }
}

fn stable_runtime_activity_code(code: &str) -> &'static str {
    for (prefix, stable) in [
        ("serial_open_failed:", "serial_open_failed"),
        ("serial_handshake_failed:", "serial_handshake_failed"),
        ("serial_read_failed:", "serial_read_failed"),
        ("serial_write_failed:", "serial_write_failed"),
        ("metrics_write_failed:", "metrics_write_failed"),
        ("paste_submit_failed:", "paste_submit_failed"),
    ] {
        if code.starts_with(prefix) {
            return stable;
        }
    }
    match code {
        "action_step_completed" => "action_step_completed",
        "action_step_failed" => "action_step_failed",
        "assignment_board_mismatch" => "assignment_board_mismatch",
        "device_disconnected" => "device_disconnected",
        "device_offline" => "device_offline",
        "device_worker_stopped" => "device_worker_stopped",
        "duplicate_identity" => "duplicate_identity",
        "empty_action_list" => "empty_action_list",
        "input_before_configuration" => "input_before_configuration",
        "input_state" => "input_state",
        "invalid_action_acknowledgement" => "invalid_action_acknowledgement",
        "invalid_assignment" => "invalid_assignment",
        "invalid_learning_target" => "invalid_learning_target",
        "invalid_topology" => "invalid_topology",
        "learning_input" => "learning_input",
        "learning_ready" => "learning_ready",
        "learning_session_active" => "learning_session_active",
        "metrics_write_failed" => "metrics_write_failed",
        "no_runtime_assignment" => "no_runtime_assignment",
        "paste_coordinator_stopped" => "paste_coordinator_stopped",
        "paste_grant_mismatch" => "paste_grant_mismatch",
        "paste_submit_failed" => "paste_submit_failed",
        "protocol_mismatch" => "protocol_mismatch",
        "serial_handshake_failed" => "serial_handshake_failed",
        "serial_handshake_timeout" => "serial_handshake_timeout",
        "serial_open_failed" => "serial_open_failed",
        "serial_read_failed" => "serial_read_failed",
        "serial_write_failed" => "serial_write_failed",
        "topology_active" => "topology_active",
        "topology_cleared" => "topology_cleared",
        "topology_rejected" => "topology_rejected",
        "unexpected_action_acknowledgement" => "unexpected_action_acknowledgement",
        "unexpected_paste_grant" => "unexpected_paste_grant",
        "unmapped_input" => "unmapped_input",
        _ => "runtime_error",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeAssignmentLogSnapshot<'a> {
    device_profile_id: &'a str,
    hardware_profile_id: &'a str,
}

impl<'a> From<&'a RuntimeAssignment> for RuntimeAssignmentLogSnapshot<'a> {
    fn from(assignment: &'a RuntimeAssignment) -> Self {
        Self {
            device_profile_id: &assignment.device_profile_id,
            hardware_profile_id: &assignment.hardware_profile_id,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidateLogSnapshot<'a> {
    key: &'a str,
    device_id: Option<&'a str>,
    mode: DeviceMode,
    identity: IdentityDimension,
    issue: CandidateIssue,
    raw_serial: Option<&'a str>,
    port: Option<&'a str>,
    controller_family_id: &'a str,
    board_profile_id: &'a str,
}

impl<'a> From<&'a CandidateStatus> for CandidateLogSnapshot<'a> {
    fn from(status: &'a CandidateStatus) -> Self {
        Self {
            key: &status.key,
            device_id: status.device_id.as_ref().map(DeviceId::as_str),
            mode: status.mode,
            identity: status.identity,
            issue: status.issue,
            raw_serial: status.raw_serial.as_deref(),
            port: status.port.as_deref(),
            controller_family_id: &status.controller_family_id,
            board_profile_id: &status.board_profile_id,
        }
    }
}

impl DeviceLogInventory {
    pub(crate) fn observe(
        &mut self,
        timestamp_ms: u64,
        devices: &[DeviceStatus],
        candidates: &[CandidateStatus],
    ) -> Vec<RuntimeLogEntry> {
        let next_devices = devices
            .iter()
            .cloned()
            .map(|status| (status.device_id.clone(), status))
            .collect::<BTreeMap<_, _>>();
        let next_candidates = candidates
            .iter()
            .cloned()
            .map(|status| (status.key.clone(), status))
            .collect::<BTreeMap<_, _>>();
        let mut entries = Vec::new();

        for (device_id, current) in &next_devices {
            let previous = self.devices.get(device_id);
            if previous == Some(current) {
                continue;
            }
            let event = match previous {
                None if current.connection == ConnectionDimension::Online => "device_connected",
                Some(previous)
                    if previous.connection == ConnectionDimension::Offline
                        && current.connection == ConnectionDimension::Online =>
                {
                    "device_connected"
                }
                Some(previous)
                    if previous.connection == ConnectionDimension::Online
                        && current.connection == ConnectionDimension::Offline =>
                {
                    "device_disconnected"
                }
                _ => "device_status_changed",
            };
            let level = if current.runtime == RuntimeDimension::RuntimeError {
                RuntimeLogLevel::Error
            } else if event == "device_disconnected" {
                RuntimeLogLevel::Warning
            } else {
                RuntimeLogLevel::Info
            };
            if let Some(entry) =
                device_changed_entry(timestamp_ms, level, event, previous, Some(current))
            {
                entries.push(entry);
            }
        }

        for (device_id, previous) in &self.devices {
            if !next_devices.contains_key(device_id)
                && previous.connection == ConnectionDimension::Online
                && let Some(entry) = device_changed_entry(
                    timestamp_ms,
                    RuntimeLogLevel::Warning,
                    "device_disconnected",
                    Some(previous),
                    None,
                )
            {
                entries.push(entry);
            }
        }

        for (key, current) in &next_candidates {
            let previous = self.candidates.get(key);
            if previous == Some(current) {
                continue;
            }
            let level = if current.issue == CandidateIssue::Validating {
                RuntimeLogLevel::Info
            } else {
                RuntimeLogLevel::Warning
            };
            if let Some(entry) = candidate_changed_entry(
                timestamp_ms,
                level,
                "device_candidate_changed",
                previous,
                Some(current),
            ) {
                entries.push(entry);
            }
        }

        for (key, previous) in &self.candidates {
            if !next_candidates.contains_key(key)
                && let Some(entry) = candidate_changed_entry(
                    timestamp_ms,
                    RuntimeLogLevel::Info,
                    "device_candidate_resolved",
                    Some(previous),
                    None,
                )
            {
                entries.push(entry);
            }
        }

        self.devices = next_devices;
        self.candidates = next_candidates;
        entries
    }

    pub(crate) fn observe_scan_error(
        &mut self,
        timestamp_ms: u64,
        error: Option<&str>,
    ) -> Vec<RuntimeLogEntry> {
        let Some(error) = error else {
            self.last_scan_error = None;
            return Vec::new();
        };
        if self.last_scan_error.as_deref() == Some(error) {
            return Vec::new();
        }
        self.last_scan_error = Some(error.into());
        vec![
            RuntimeLogEntry::new(
                timestamp_ms,
                RuntimeLogLevel::Error,
                "device_scan_failed",
                Value::Object(Map::new()),
            )
            .with_detail(error),
        ]
    }
}

fn device_changed_entry(
    timestamp_ms: u64,
    level: RuntimeLogLevel,
    event: &str,
    previous: Option<&DeviceStatus>,
    current: Option<&DeviceStatus>,
) -> Option<RuntimeLogEntry> {
    let previous = previous.map(DeviceLogSnapshot::from);
    let current = current.map(DeviceLogSnapshot::from);
    changed_entry(
        timestamp_ms,
        level,
        event,
        previous.as_ref(),
        current.as_ref(),
    )
}

fn candidate_changed_entry(
    timestamp_ms: u64,
    level: RuntimeLogLevel,
    event: &str,
    previous: Option<&CandidateStatus>,
    current: Option<&CandidateStatus>,
) -> Option<RuntimeLogEntry> {
    let previous = previous.map(CandidateLogSnapshot::from);
    let current = current.map(CandidateLogSnapshot::from);
    changed_entry(
        timestamp_ms,
        level,
        event,
        previous.as_ref(),
        current.as_ref(),
    )
}

fn changed_entry<T: Serialize>(
    timestamp_ms: u64,
    level: RuntimeLogLevel,
    event: &str,
    previous: Option<&T>,
    current: Option<&T>,
) -> Option<RuntimeLogEntry> {
    let context = changed_context(previous, current).map_err(|error| {
        eprintln!("failed to serialize {event} runtime log entry: {error}");
    });
    context
        .ok()
        .map(|context| RuntimeLogEntry::new(timestamp_ms, level, event, context))
}

fn changed_context<T: Serialize>(
    previous: Option<&T>,
    current: Option<&T>,
) -> serde_json::Result<Value> {
    let mut context = Map::new();
    if let Some(previous) = previous {
        context.insert("previous".into(), serde_json::to_value(previous)?);
    }
    if let Some(current) = current {
        context.insert("current".into(), serde_json::to_value(current)?);
    }
    Ok(Value::Object(context))
}

pub(crate) fn install<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    config_directory: &Path,
) -> tauri::Result<()> {
    app.plugin(
        Builder::new()
            .level(LevelFilter::Info)
            .clear_format()
            .max_file_size(MAX_FILE_SIZE)
            .rotation_strategy(RotationStrategy::KeepSome(RETAINED_FILES))
            .targets([
                Target::new(TargetKind::Folder {
                    path: log_directory(config_directory),
                    file_name: Some("kivo".into()),
                })
                .filter(|metadata| metadata.target() == LOG_TARGET),
                Target::new(TargetKind::Stderr).filter(|metadata| metadata.target() == LOG_TARGET),
            ])
            .build(),
    )?;
    if DISPATCHER.get().is_none() {
        let dispatcher = LogDispatcher::start(LOG_QUEUE_CAPACITY, OfficialLogSink)?;
        let _ = DISPATCHER.set(dispatcher);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DeviceLogInventory, DispatchOutcome, LogDispatcher, LogSink, QueuedLogEntry,
        RuntimeLogEntry, RuntimeLogLevel, log_directory, metrics_initialization_failure_detail,
        operation, operation_entry, queued_entry, runtime_event_entry, serialize_entry,
    };
    use crate::{
        coordinator::{
            AssignmentDimension, CandidateIssue, CandidateStatus, ConnectionDimension, DeviceMode,
            DeviceStatus, EventLevel, IdentityDimension, RuntimeDimension, RuntimeEvent,
        },
        device::{LearningTarget, RuntimeActivity},
        hardware::{DeviceId, ESP32S3_FAMILY_ID, LUATOS_ESP32S3_AIO_BOARD_ID},
        metrics::HomeMetricsSnapshot,
        workspace::{AppError, RuntimeAssignment},
    };
    use serde_json::json;
    use std::{
        cell::Cell,
        collections::BTreeSet,
        path::Path,
        sync::{Arc, Mutex, mpsc},
        time::Duration,
    };

    struct RecordingSink {
        entries: Arc<Mutex<Vec<QueuedLogEntry>>>,
        flush_count: Arc<Mutex<usize>>,
    }

    impl LogSink for RecordingSink {
        fn write(&mut self, entry: QueuedLogEntry) {
            self.entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(entry);
        }

        fn flush(&mut self) {
            *self
                .flush_count
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) += 1;
        }
    }

    struct BlockingSink {
        started: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        entries: Arc<Mutex<Vec<QueuedLogEntry>>>,
    }

    impl LogSink for BlockingSink {
        fn write(&mut self, entry: QueuedLogEntry) {
            if self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_empty()
            {
                self.started.send(()).unwrap();
                self.release.recv().unwrap();
            }
            self.entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(entry);
        }

        fn flush(&mut self) {}
    }

    fn queued(event: &str) -> QueuedLogEntry {
        QueuedLogEntry {
            level: RuntimeLogLevel::Info,
            line: event.into(),
        }
    }

    fn device_status(connection: ConnectionDimension) -> DeviceStatus {
        let online = connection == ConnectionDimension::Online;
        DeviceStatus {
            device_id: DeviceId::new(LUATOS_ESP32S3_AIO_BOARD_ID, "ABCDEF123456").unwrap(),
            name: "Desk".into(),
            connection,
            mode: online.then_some(DeviceMode::Runtime),
            identity: IdentityDimension::Valid,
            assignment: AssignmentDimension::Unassigned,
            runtime: RuntimeDimension::Inactive,
            raw_serial: "ABCDEF123456".into(),
            port: online.then(|| "/dev/cu.test".into()),
            controller_family_id: ESP32S3_FAMILY_ID.into(),
            board_profile_id: LUATOS_ESP32S3_AIO_BOARD_ID.into(),
            firmware_build_id: None,
            pins: Vec::new(),
            runtime_assignment: None,
            latest_error: None,
            learning: None,
        }
    }

    fn candidate_status(issue: CandidateIssue) -> CandidateStatus {
        CandidateStatus {
            key: "runtime:/dev/cu.candidate".into(),
            device_id: None,
            mode: DeviceMode::Runtime,
            identity: if issue == CandidateIssue::Validating {
                IdentityDimension::Validating
            } else {
                IdentityDimension::Valid
            },
            issue,
            raw_serial: Some("CANDIDATE123".into()),
            port: Some("/dev/cu.candidate".into()),
            controller_family_id: ESP32S3_FAMILY_ID.into(),
            board_profile_id: LUATOS_ESP32S3_AIO_BOARD_ID.into(),
            latest_error: None,
        }
    }

    #[test]
    fn places_runtime_logs_under_the_application_data_directory() {
        assert_eq!(
            log_directory(Path::new("/tmp/kivo")),
            Path::new("/tmp/kivo/data/log")
        );
    }

    #[test]
    fn serializes_runtime_log_entries_as_single_json_lines() {
        let entry = RuntimeLogEntry::new(
            1_722_355_200_000,
            RuntimeLogLevel::Info,
            "application_started",
            json!({"version": "0.1.0"}),
        );

        let line = serialize_entry(&entry).expect("entry serializes");
        let value: serde_json::Value = serde_json::from_str(&line).expect("line is JSON");

        assert!(!line.contains('\n'));
        assert_eq!(value["timestampMs"], 1_722_355_200_000_u64);
        assert_eq!(value["level"], "info");
        assert_eq!(value["event"], "application_started");
        assert_eq!(value["context"]["version"], "0.1.0");
    }

    #[test]
    fn metrics_initialization_failure_detail_omits_invalid_paths() {
        let private_path = "/Users/alice/private/client/metrics.sqlite3";
        let detail = metrics_initialization_failure_detail(&rusqlite::Error::InvalidPath(
            private_path.into(),
        ));

        assert_eq!(detail, "invalid_path");
        assert!(!detail.contains("alice"));
        assert!(!detail.contains("private"));
        assert!(!detail.contains(private_path));
    }

    #[test]
    fn operation_entries_capture_result_without_payloads() {
        let success: Result<(), AppError> = Ok(());
        let failed: Result<(), AppError> = Err(AppError::new("invalid_assignment")
            .with_param("profileName", "Private Profile")
            .with_detail("/Users/alice/private/secret"));
        let context = json!({"deviceId": "device-1"});

        let succeeded = operation_entry(100, "runtime_assignment_saved", context.clone(), &success);
        let rejected = operation_entry(200, "runtime_assignment_saved", context, &failed);

        assert_eq!(succeeded.result.as_deref(), Some("succeeded"));
        assert_eq!(succeeded.level, RuntimeLogLevel::Info);
        assert_eq!(rejected.result.as_deref(), Some("failed"));
        assert_eq!(rejected.level, RuntimeLogLevel::Error);
        assert_eq!(rejected.detail.as_deref(), Some("invalid_assignment"));
        let line = serialize_entry(&rejected).expect("entry serializes");
        assert!(!line.contains("/Users/alice/private/secret"));
        assert!(!line.contains("Private Profile"));
        assert!(!line.contains("profileName"));
    }

    #[test]
    fn operation_returns_action_result_unchanged() {
        let success_calls = Cell::new(0);
        let success = operation(100, "test_succeeded", json!({}), || {
            success_calls.set(success_calls.get() + 1);
            Ok("original value")
        });
        let expected_error = AppError::new("invalid_assignment")
            .with_param("profileName", "Private Profile")
            .with_detail("/Users/alice/private/secret");
        let failed_calls = Cell::new(0);
        let failed = operation(200, "test_failed", json!({}), || {
            failed_calls.set(failed_calls.get() + 1);
            Err::<(), _>(expected_error.clone())
        });

        assert_eq!(success_calls.get(), 1);
        assert_eq!(success, Ok("original value"));
        assert_eq!(failed_calls.get(), 1);
        assert_eq!(failed, Err(expected_error));
    }

    #[test]
    fn log_dispatcher_writes_accepted_entries_in_fifo_order() {
        let entries = Arc::new(Mutex::new(Vec::new()));
        let flush_count = Arc::new(Mutex::new(0));
        let dispatcher = LogDispatcher::start(
            4,
            RecordingSink {
                entries: Arc::clone(&entries),
                flush_count: Arc::clone(&flush_count),
            },
        )
        .unwrap();

        assert_eq!(
            dispatcher.try_enqueue(queued("first")),
            DispatchOutcome::Accepted
        );
        assert_eq!(
            dispatcher.try_enqueue(queued("second")),
            DispatchOutcome::Accepted
        );
        assert_eq!(
            dispatcher.try_enqueue(queued("third")),
            DispatchOutcome::Accepted
        );
        dispatcher.shutdown();

        let lines = entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .filter(|entry| !entry.line.contains("runtime_log_entries_dropped"))
            .map(|entry| entry.line.clone())
            .collect::<Vec<_>>();
        assert_eq!(lines, ["first", "second", "third"]);
    }

    #[test]
    fn log_dispatcher_accepts_priority_operation_when_normal_queue_is_full() {
        let entries = Arc::new(Mutex::new(Vec::new()));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let dispatcher = LogDispatcher::start(
            1,
            BlockingSink {
                started: started_tx,
                release: release_rx,
                entries: Arc::clone(&entries),
            },
        )
        .unwrap();

        assert_eq!(
            dispatcher.try_enqueue(queued("first")),
            DispatchOutcome::Accepted
        );
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(
            dispatcher.try_enqueue(queued("second")),
            DispatchOutcome::Accepted
        );
        assert_eq!(
            dispatcher.try_enqueue(queued("newest")),
            DispatchOutcome::Dropped
        );
        let operation_result: Result<(), AppError> = Ok(());
        let operation_entry = queued_entry(operation_entry(
            100,
            "runtime_assignment_saved",
            json!({"deviceId": "device-1"}),
            &operation_result,
        ))
        .unwrap();
        let operation_line = operation_entry.line.clone();
        assert_eq!(
            dispatcher.enqueue_priority(operation_entry),
            DispatchOutcome::Accepted
        );
        assert_eq!(dispatcher.dropped_count(), 1);
        release_tx.send(()).unwrap();
        dispatcher.shutdown();

        let lines = entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .map(|entry| entry.line.clone())
            .collect::<Vec<_>>();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("runtime_log_entries_dropped"))
        );
        let accepted_lines = lines
            .iter()
            .filter(|line| !line.contains("runtime_log_entries_dropped"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(accepted_lines, ["first", "second", &operation_line]);
    }

    #[test]
    fn log_dispatcher_shutdown_drains_accepted_entries_and_flushes_once() {
        let entries = Arc::new(Mutex::new(Vec::new()));
        let flush_count = Arc::new(Mutex::new(0));
        let dispatcher = LogDispatcher::start(
            8,
            RecordingSink {
                entries: Arc::clone(&entries),
                flush_count: Arc::clone(&flush_count),
            },
        )
        .unwrap();

        for event in ["one", "two", "three", "four"] {
            assert_eq!(
                dispatcher.try_enqueue(queued(event)),
                DispatchOutcome::Accepted
            );
        }
        dispatcher.shutdown_with_entry(queued("application_stopped"));

        let lines = entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .map(|entry| entry.line.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            lines,
            ["one", "two", "three", "four", "application_stopped"]
        );
        assert_eq!(*flush_count.lock().unwrap(), 1);
        assert_eq!(
            dispatcher.try_enqueue(queued("after")),
            DispatchOutcome::Stopped
        );
    }

    #[test]
    fn runtime_event_entry_keeps_safe_context_and_omits_home_metrics() {
        let mut activity = RuntimeActivity::new("action_step_completed");
        activity.params.insert("button".into(), "A".into());
        let event = RuntimeEvent {
            timestamp_ms: 1_722_355_200_123,
            level: EventLevel::Warning,
            device_id: DeviceId::new(LUATOS_ESP32S3_AIO_BOARD_ID, "ABCDEF123456").unwrap(),
            raw_serial: "ABCDEF123456".into(),
            controller_family_id: ESP32S3_FAMILY_ID.into(),
            board_profile_id: LUATOS_ESP32S3_AIO_BOARD_ID.into(),
            port: Some("/dev/cu.test".into()),
            device_profile_id: Some("desk-profile".into()),
            hardware_profile_id: Some("desk-hardware".into()),
            home_update: Some(HomeMetricsSnapshot {
                total_presses: 42,
                ..HomeMetricsSnapshot::default()
            }),
            activity,
        };

        let entry = runtime_event_entry(&event).expect("safe runtime event serializes");
        let value: serde_json::Value =
            serde_json::from_str(&serialize_entry(&entry).expect("runtime log entry serializes"))
                .unwrap();

        assert_eq!(value["timestampMs"], event.timestamp_ms);
        assert_eq!(value["level"], "warning");
        assert_eq!(value["event"], "action_step_completed");
        assert_eq!(value["context"]["deviceId"], event.device_id.as_str());
        assert_eq!(value["context"]["rawSerial"], "ABCDEF123456");
        assert_eq!(value["context"]["controllerFamilyId"], "esp32s3");
        assert_eq!(
            value["context"]["boardProfileId"],
            LUATOS_ESP32S3_AIO_BOARD_ID
        );
        assert_eq!(value["context"]["port"], "/dev/cu.test");
        assert_eq!(value["context"]["deviceProfileId"], "desk-profile");
        assert_eq!(value["context"]["hardwareProfileId"], "desk-hardware");
        assert_eq!(
            value["context"]["activity"]["code"],
            "action_step_completed"
        );
        assert_eq!(value["context"]["activity"]["params"]["button"], "A");
        assert_eq!(value["context"].as_object().unwrap().len(), 8);
        let line = serialize_entry(&entry).unwrap();
        assert!(!line.contains("homeUpdate"));
        assert!(!line.contains("metrics"));

        for (level, expected) in [
            (EventLevel::Info, RuntimeLogLevel::Info),
            (EventLevel::Warning, RuntimeLogLevel::Warning),
            (EventLevel::Error, RuntimeLogLevel::Error),
        ] {
            let mapped = runtime_event_entry(&RuntimeEvent {
                level,
                ..event.clone()
            })
            .unwrap();
            assert_eq!(mapped.level, expected);
        }
    }

    #[test]
    fn device_log_inventory_emits_only_device_and_candidate_changes() {
        let mut inventory = DeviceLogInventory::default();
        let online = device_status(ConnectionDimension::Online);

        let connected = inventory.observe(100, std::slice::from_ref(&online), &[]);
        assert_eq!(connected.len(), 1);
        assert_eq!(connected[0].event, "device_connected");
        assert_eq!(connected[0].level, RuntimeLogLevel::Info);
        assert_eq!(connected[0].context["current"]["connection"], "online");
        assert!(
            inventory
                .observe(200, std::slice::from_ref(&online), &[])
                .is_empty()
        );

        let mut runtime_error = online.clone();
        runtime_error.runtime = RuntimeDimension::RuntimeError;
        let changed = inventory.observe(300, std::slice::from_ref(&runtime_error), &[]);
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].event, "device_status_changed");
        assert_eq!(changed[0].level, RuntimeLogLevel::Error);
        assert_eq!(changed[0].context["previous"]["runtime"], "inactive");
        assert_eq!(changed[0].context["current"]["runtime"], "runtime_error");

        let offline = device_status(ConnectionDimension::Offline);
        let disconnected = inventory.observe(400, std::slice::from_ref(&offline), &[]);
        assert_eq!(disconnected.len(), 1);
        assert_eq!(disconnected[0].event, "device_disconnected");
        assert_eq!(disconnected[0].level, RuntimeLogLevel::Warning);

        let validating = candidate_status(CandidateIssue::Validating);
        let candidate_new = inventory.observe(500, &[offline], std::slice::from_ref(&validating));
        assert_eq!(candidate_new.len(), 1);
        assert_eq!(candidate_new[0].event, "device_candidate_changed");
        assert_eq!(candidate_new[0].level, RuntimeLogLevel::Info);

        let unavailable = candidate_status(CandidateIssue::PortUnavailable);
        let candidate_changed = inventory.observe(600, &[], std::slice::from_ref(&unavailable));
        assert_eq!(candidate_changed.len(), 1);
        assert_eq!(candidate_changed[0].event, "device_candidate_changed");
        assert_eq!(candidate_changed[0].level, RuntimeLogLevel::Warning);
        assert_eq!(
            candidate_changed[0].context["previous"]["issue"],
            "validating"
        );
        assert_eq!(
            candidate_changed[0].context["current"]["issue"],
            "port_unavailable"
        );

        let resolved = inventory.observe(700, &[], &[]);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].event, "device_candidate_resolved");
        assert_eq!(resolved[0].level, RuntimeLogLevel::Info);
        assert_eq!(resolved[0].context["previous"]["issue"], "port_unavailable");
    }

    #[test]
    fn device_log_inventory_deduplicates_consecutive_scan_errors() {
        let mut inventory = DeviceLogInventory::default();

        let first = inventory.observe_scan_error(100, Some("usb unavailable"));
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].event, "device_scan_failed");
        assert_eq!(first[0].level, RuntimeLogLevel::Error);
        assert_eq!(first[0].detail.as_deref(), Some("usb unavailable"));
        assert!(
            inventory
                .observe_scan_error(200, Some("usb unavailable"))
                .is_empty()
        );
        assert!(inventory.observe_scan_error(300, None).is_empty());
        assert_eq!(
            inventory
                .observe_scan_error(400, Some("usb unavailable"))
                .len(),
            1
        );
    }

    #[test]
    fn device_log_inventory_uses_allowlisted_status_projections() {
        let device_name_secret = "Private desk owned by alice";
        let device_error_secret = "/Users/alice/private/device-error.txt";
        let prefixed_code_secret = "/Users/alice/private/serial-device.txt";
        let unknown_code_secret = "diagnostic_token_abc123";
        let candidate_error_secret = "/Users/alice/private/candidate-error.txt";
        let learning_secret = "private-learning-profile";
        let mut device = device_status(ConnectionDimension::Online);
        device.name = device_name_secret.into();
        device.pins = vec![201, 202, 203];
        device.runtime_assignment = Some(RuntimeAssignment {
            device_profile_id: "desk-profile".into(),
            hardware_profile_id: "desk-hardware".into(),
        });
        let mut latest_error =
            RuntimeActivity::new(format!("serial_open_failed: {prefixed_code_secret}"));
        latest_error
            .params
            .insert("rawError".into(), device_error_secret.into());
        latest_error.detail = Some(device_error_secret.into());
        device.latest_error = Some(latest_error);
        device.learning = Some(LearningTarget {
            device_id: device.device_id.clone(),
            device_profile_id: learning_secret.into(),
            hardware_profile_id: "private-learning-hardware".into(),
            editing_revision: 991,
            firmware_revision: 992,
            pins: vec![204, 205],
        });
        let mut candidate = candidate_status(CandidateIssue::PortUnavailable);
        candidate.latest_error = Some(candidate_error_secret.into());

        let mut inventory = DeviceLogInventory::default();
        let mut unknown_error_device = device.clone();
        unknown_error_device.latest_error = Some(RuntimeActivity::new(unknown_code_secret));
        let entries = inventory.observe(100, &[device], &[candidate]);
        let unknown_error_entries = inventory.observe(200, &[unknown_error_device], &[]);
        let line = entries
            .iter()
            .chain(&unknown_error_entries)
            .map(serialize_entry)
            .collect::<serde_json::Result<Vec<_>>>()
            .unwrap()
            .join("\n");
        let device_context = &entries
            .iter()
            .find(|entry| entry.event == "device_connected")
            .unwrap()
            .context["current"];
        let candidate_context = &entries
            .iter()
            .find(|entry| entry.event == "device_candidate_changed")
            .unwrap()
            .context["current"];

        assert_eq!(
            device_context
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            [
                "assignment",
                "boardProfileId",
                "connection",
                "controllerFamilyId",
                "deviceId",
                "firmwareBuildId",
                "identity",
                "latestErrorCode",
                "learningActive",
                "mode",
                "port",
                "rawSerial",
                "runtime",
                "runtimeAssignment",
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(device_context["latestErrorCode"], "serial_open_failed");
        assert_eq!(
            unknown_error_entries[0].context["current"]["latestErrorCode"],
            "runtime_error"
        );
        assert_eq!(device_context["learningActive"], true);
        assert_eq!(
            device_context["runtimeAssignment"],
            json!({
                "deviceProfileId": "desk-profile",
                "hardwareProfileId": "desk-hardware",
            })
        );
        assert_eq!(
            candidate_context
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            [
                "boardProfileId",
                "controllerFamilyId",
                "deviceId",
                "identity",
                "issue",
                "key",
                "mode",
                "port",
                "rawSerial",
            ]
            .into_iter()
            .collect()
        );
        for secret in [
            device_name_secret,
            device_error_secret,
            prefixed_code_secret,
            unknown_code_secret,
            candidate_error_secret,
            learning_secret,
            "201",
            "202",
            "203",
            "204",
            "205",
        ] {
            assert!(!line.contains(secret), "leaked {secret}");
        }
    }

    #[test]
    fn device_log_inventory_disconnects_only_disappearing_online_devices() {
        let mut online_inventory = DeviceLogInventory::default();
        online_inventory.observe(100, &[device_status(ConnectionDimension::Online)], &[]);

        let disappeared = online_inventory.observe(200, &[], &[]);

        assert_eq!(disappeared.len(), 1);
        assert_eq!(disappeared[0].event, "device_disconnected");
        assert_eq!(disappeared[0].level, RuntimeLogLevel::Warning);
        assert_eq!(disappeared[0].context["previous"]["connection"], "online");
        assert!(disappeared[0].context.get("current").is_none());

        let mut offline_inventory = DeviceLogInventory::default();
        offline_inventory.observe(300, &[device_status(ConnectionDimension::Offline)], &[]);
        assert!(offline_inventory.observe(400, &[], &[]).is_empty());
    }
}
