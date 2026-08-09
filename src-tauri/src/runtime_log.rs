use crate::{
    coordinator::{
        CandidateIssue, CandidateStatus, ConnectionDimension, DeviceStatus, EventLevel,
        RuntimeDimension, RuntimeEvent,
    },
    hardware::DeviceId,
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use tauri_plugin_log::{
    Builder, RotationStrategy, Target, TargetKind,
    log::{self, LevelFilter},
};

pub(crate) const LOG_TARGET: &str = "kivo::runtime";
pub(crate) const MAX_FILE_SIZE: u128 = 10 * 1024 * 1024;
pub(crate) const RETAINED_FILES: usize = 5;

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

    #[allow(dead_code)] // Used when mutating command result logs are connected in Task 4.
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

pub(crate) fn emit(entry: RuntimeLogEntry) {
    let line = match serialize_entry(&entry) {
        Ok(line) => line,
        Err(error) => {
            eprintln!("failed to serialize runtime log entry: {error}");
            return;
        }
    };

    match entry.level {
        RuntimeLogLevel::Info => log::info!(target: LOG_TARGET, "{line}"),
        RuntimeLogLevel::Warning => log::warn!(target: LOG_TARGET, "{line}"),
        RuntimeLogLevel::Error => log::error!(target: LOG_TARGET, "{line}"),
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
            if let Some(entry) = changed_entry(timestamp_ms, level, event, previous, Some(current))
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
            if let Some(entry) = changed_entry(
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
                && let Some(entry) = changed_entry::<CandidateStatus>(
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
    )
}

#[cfg(test)]
mod tests {
    use super::{
        DeviceLogInventory, RuntimeLogEntry, RuntimeLogLevel, log_directory, runtime_event_entry,
        serialize_entry,
    };
    use crate::{
        coordinator::{
            AssignmentDimension, CandidateIssue, CandidateStatus, ConnectionDimension, DeviceMode,
            DeviceStatus, EventLevel, IdentityDimension, RuntimeDimension, RuntimeEvent,
        },
        device::RuntimeActivity,
        hardware::{DeviceId, ESP32S3_FAMILY_ID, LUATOS_ESP32S3_AIO_BOARD_ID},
        metrics::HomeMetricsSnapshot,
    };
    use serde_json::json;
    use std::path::Path;

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
}
