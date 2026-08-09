# Runtime File Logging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add bounded JSON Lines runtime logs under `data/log` that cover lifecycle, device state, every input and action, configuration operations, and failures without storing pasted text or open targets.

**Architecture:** A new `runtime_log.rs` module owns official `tauri-plugin-log` setup, JSON serialization, and scan-state deduplication. `device.rs` emits sanitized action lifecycle activities into the existing `RuntimeEvent` pipeline, while `lib.rs` records enriched events, device transitions, lifecycle, and command outcomes.

**Tech Stack:** Rust 2024, Tauri 2.11, `tauri-plugin-log` 2.9, Serde/JSON, Rust unit and integration tests.

---

## File Map

- Create `src-tauri/src/runtime_log.rs`: plugin setup, JSON envelope, safe event conversion, scan transition deduplication, and operation-result helpers.
- Create `src-tauri/tests/runtime_log_rotation.rs`: real official-plugin rotation test in an isolated test process.
- Modify `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`: add and lock `tauri-plugin-log = "2.9.0"`.
- Modify `src-tauri/src/protocol.rs`: return the acknowledged `ActionStep`.
- Modify `src-tauri/src/device.rs`: emit sanitized action start/completion activities.
- Modify `src-tauri/src/coordinator.rs`: classify new successful action activities as info.
- Modify `src-tauri/src/lib.rs`: install logging and connect runtime, scan, lifecycle, and command events.

### Task 1: Logging Foundation

**Files:**
- Create: `src-tauri/src/runtime_log.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

- [ ] **Step 1: Write failing path and envelope tests**

Create `src-tauri/src/runtime_log.rs` with this test module, and add `mod runtime_log;` to `src-tauri/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn log_directory_is_nested_under_runtime_data() {
        assert_eq!(
            log_directory(Path::new("/tmp/kivo")),
            Path::new("/tmp/kivo/data/log")
        );
    }

    #[test]
    fn entry_is_one_parseable_json_object() {
        let line = serialize_entry(&RuntimeLogEntry::new(
            1_722_355_200_000,
            RuntimeLogLevel::Info,
            "application_started",
            json!({"version": "0.1.0"}),
        ))
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["timestampMs"], 1_722_355_200_000_u64);
        assert_eq!(value["level"], "info");
        assert_eq!(value["event"], "application_started");
        assert_eq!(value["context"]["version"], "0.1.0");
        assert!(!line.contains('\n'));
    }
}
```

- [ ] **Step 2: Run RED**

Run `cargo test --manifest-path src-tauri/Cargo.toml runtime_log::tests --lib`.

Expected: compilation fails because the four tested logging symbols do not exist.

- [ ] **Step 3: Add the dependency and minimal implementation**

Add `tauri-plugin-log = "2.9.0"` under `[dependencies]`. Implement:

```rust
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::Runtime;
use tauri_plugin_log::{
    Builder, RotationStrategy, Target, TargetKind,
    log::{LevelFilter, error, info, warn},
};

pub(crate) const LOG_TARGET: &str = "kivo::runtime";
pub(crate) const MAX_FILE_SIZE: u128 = 10 * 1024 * 1024;
pub(crate) const RETAINED_FILES: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RuntimeLogLevel { Info, Warning, Error }

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeLogEntry {
    pub timestamp_ms: u64,
    pub level: RuntimeLogLevel,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub context: Value,
}

impl RuntimeLogEntry {
    pub(crate) fn new(timestamp_ms: u64, level: RuntimeLogLevel, event: impl Into<String>, context: Value) -> Self {
        Self { timestamp_ms, level, event: event.into(), result: None, detail: None, context }
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

pub(crate) fn emit(entry: RuntimeLogEntry) {
    let level = entry.level;
    let message = match serialize_entry(&entry) {
        Ok(message) => message,
        Err(error) => {
            eprintln!("serialize runtime log entry: {error}");
            return;
        }
    };
    match level {
        RuntimeLogLevel::Info => info!(target: LOG_TARGET, "{message}"),
        RuntimeLogLevel::Warning => warn!(target: LOG_TARGET, "{message}"),
        RuntimeLogLevel::Error => error!(target: LOG_TARGET, "{message}"),
    }
}

pub(crate) fn install<R: Runtime>(app: &tauri::AppHandle<R>, config_directory: &Path) -> tauri::Result<()> {
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
                }).filter(|metadata| metadata.target() == LOG_TARGET),
                Target::new(TargetKind::Stderr)
                    .filter(|metadata| metadata.target() == LOG_TARGET),
            ])
            .build(),
    )
}
```

- [ ] **Step 4: Run GREEN**

Run `cargo test --manifest-path src-tauri/Cargo.toml runtime_log::tests --lib`.

Expected: both tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src-tauri/src/runtime_log.rs
git commit -m "feat: add bounded runtime log target"
```

### Task 2: Sanitized Action Lifecycle

**Files:**
- Modify: `src-tauri/src/protocol.rs`
- Modify: `src-tauri/src/device.rs`
- Modify: `src-tauri/src/coordinator.rs`

- [ ] **Step 1: Write failing lifecycle and privacy tests**

Extend `deferred_paste_waits_for_global_grant_before_emitting_device_command` to use `"甲乙丙"` and assert the pending output contains:

```rust
let started = pending.activities.iter()
    .find(|activity| activity.code == "action_step_started")
    .unwrap();
assert_eq!(started.params["eventId"], "41");
assert_eq!(started.params["button"], "A");
assert_eq!(started.params["step"], "1");
assert_eq!(started.params["total"], "2");
assert_eq!(started.params["actionKind"], "paste");
assert_eq!(started.params["characterCount"], "3");
assert!(!format!("{started:?}").contains("甲乙丙"));
```

After acknowledging step 1, assert:

```rust
assert!(next.activities.iter().any(|activity| {
    activity.code == "action_step_completed"
        && activity.params["step"] == "1"
        && activity.params["actionKind"] == "paste"
}));
assert!(next.activities.iter().any(|activity| {
    activity.code == "action_step_started"
        && activity.params["step"] == "2"
        && activity.params["actionKind"] == "hotkey"
}));
```

Add `open_action_activity_redacts_target`, using `https://example.test/private?token=secret`, and assert only `targetKind=url` and `characterCount` appear; debug output must not contain `private` or `secret`.

Extend `persists_a_mapped_button_press_as_metrics_and_activity` with a release
event and assert both the press and release `input_state` activities contain
`params["button"] == "A"`.

- [ ] **Step 2: Run RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml deferred_paste_waits_for_global_grant --lib
cargo test --manifest-path src-tauri/Cargo.toml open_action_activity_redacts_target --lib
```

Expected: both fail because the lifecycle activities are absent.

- [ ] **Step 3: Return the acknowledged action snapshot**

Change `ActionSequence::acknowledge` to `Result<ActionStep, String>`. Validate the IDs first, build `ActionStep` from `self.event_id`, `self.button`, `step`, action count, and `self.actions[self.next].clone()`, then clear `awaiting`, increment `next`, and return the completed step. Existing tests can ignore the new successful value.

- [ ] **Step 4: Add sanitized metadata**

Add `action_activity(code, step)` in `device.rs`. Always add `eventId`, `button`, `step`, `total`, and `actionKind`. Use these variant-specific fields:

```rust
match &step.action {
    ButtonAction::Paste { text } => activity.with_param("characterCount", text.chars().count().to_string()),
    ButtonAction::Hotkey { keys } => activity.with_param("keys", keys.join("+")),
    ButtonAction::Delay { duration_ms } => activity.with_param("durationMs", duration_ms.to_string()),
    ButtonAction::Media { command } => activity.with_param("command", stable_media_command_name(*command)),
    ButtonAction::Open { target } => activity
        .with_param("targetKind", if target.contains("://") { "url" } else { "path" })
        .with_param("characterCount", target.chars().count().to_string()),
}
```

Implement the stable media names with an exhaustive match:

```rust
fn stable_media_command_name(command: MediaCommand) -> &'static str {
    match command {
        MediaCommand::PlayPause => "play_pause",
        MediaCommand::PreviousTrack => "previous_track",
        MediaCommand::NextTrack => "next_track",
        MediaCommand::Stop => "stop",
        MediaCommand::VolumeUp => "volume_up",
        MediaCommand::VolumeDown => "volume_down",
        MediaCommand::Mute => "mute",
    }
}
```

Never put `text` or `target` themselves in params or detail. Push `action_step_started` in `emit_active_step` before submission. In `handle_done`, use the returned `ActionStep` to push `action_step_completed` before advancing. Add both codes to the info arm in `coordinator::activity_level`.

In `handle_input`, resolve the mapped button independently of press/release and
insert it into the `input_state` activity params:

```rust
let mapped_button = metric_snapshot.as_ref().and_then(|runtime| {
    runtime.profile
        .button_for(&runtime.hardware_profile_id, &input)
        .map(str::to_owned)
});
let mut activity = RuntimeActivity {
    input: Some(input),
    pressed: Some(state == InputState::Down),
    context: context.clone(),
    metric_press,
    ..RuntimeActivity::new("input_state")
};
if let Some(button) = mapped_button {
    activity.params.insert("button".into(), button);
}
output.activities.push(activity);
```

Build `metric_press` from the same `mapped_button` before it is moved into the
activity so metrics behavior remains unchanged.

- [ ] **Step 5: Run GREEN**

```bash
cargo test --manifest-path src-tauri/Cargo.toml deferred_paste_waits_for_global_grant --lib
cargo test --manifest-path src-tauri/Cargo.toml open_action_activity_redacts_target --lib
cargo test --manifest-path src-tauri/Cargo.toml persists_a_mapped_button_press --lib
cargo test --manifest-path src-tauri/Cargo.toml device::tests --lib
cargo test --manifest-path src-tauri/Cargo.toml protocol::tests --lib
```

Expected: all selected tests pass and secret payload assertions remain negative.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/protocol.rs src-tauri/src/device.rs src-tauri/src/coordinator.rs
git commit -m "feat: log sanitized action lifecycle"
```

### Task 3: Runtime, Scan, And Lifecycle Events

**Files:**
- Modify: `src-tauri/src/runtime_log.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing conversion and deduplication tests**

Add a `RuntimeEvent` fixture with `home_update` populated. Assert `runtime_event_entry` keeps identifiers and activity but omits `homeUpdate`. Build scan fixtures with:

```rust
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
        controller_family_id: "esp32s3".into(),
        board_profile_id: LUATOS_ESP32S3_AIO_BOARD_ID.into(),
        firmware_build_id: None,
        pins: Vec::new(),
        runtime_assignment: None,
        latest_error: None,
        learning: None,
    }
}
```

Then add the deduplication test:

```rust
let mut inventory = DeviceLogInventory::default();
assert_eq!(inventory.observe(100, &[device_status(ConnectionDimension::Online)], &[])[0].event, "device_connected");
assert!(inventory.observe(200, &[device_status(ConnectionDimension::Online)], &[]).is_empty());
assert_eq!(inventory.observe(300, &[device_status(ConnectionDimension::Offline)], &[])[0].event, "device_disconnected");
assert_eq!(inventory.observe_scan_error(400, Some("usb unavailable")).len(), 1);
assert!(inventory.observe_scan_error(500, Some("usb unavailable")).is_empty());
assert!(inventory.observe_scan_error(600, None).is_empty());
assert_eq!(inventory.observe_scan_error(700, Some("usb unavailable")).len(), 1);
```

- [ ] **Step 2: Run RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml runtime_event_entry --lib
cargo test --manifest-path src-tauri/Cargo.toml device_log_inventory --lib
```

Expected: compilation fails because converters and inventory do not exist.

- [ ] **Step 3: Implement event conversion and inventory**

`runtime_event_entry` maps coordinator levels to log levels, uses the activity code as the event, and builds only this context:

```rust
serde_json::json!({
    "deviceId": event.device_id,
    "rawSerial": event.raw_serial,
    "controllerFamilyId": event.controller_family_id,
    "boardProfileId": event.board_profile_id,
    "port": event.port,
    "deviceProfileId": event.device_profile_id,
    "hardwareProfileId": event.hardware_profile_id,
    "activity": event.activity,
})
```

Implement `DeviceLogInventory` with `BTreeMap<DeviceId, DeviceStatus>`, `BTreeMap<String, CandidateStatus>`, and `last_scan_error`. Emit `device_connected`, `device_disconnected`, `device_status_changed`, `device_candidate_changed`, `device_candidate_resolved`, and deduplicated `device_scan_failed`. Serialize only previous/current changed status, then replace the saved snapshots.

- [ ] **Step 4: Preserve poll errors and connect logging**

Replace the tuple returned by `poll_runtime_coordinator` with:

```rust
struct RuntimeScanSnapshot {
    devices: Vec<DeviceStatus>,
    candidates: Vec<CandidateStatus>,
}

struct RuntimePoll {
    scan: Option<RuntimeScanSnapshot>,
    scan_error: Option<String>,
    events: Vec<RuntimeEvent>,
}
```

Update existing tests to read named fields. In `setup`, call `Workspace::load` first and do not install the runtime logger before it has completed initialization or schema migration. Only then call `runtime_log::install(app.handle(), &config_directory)` and fall back to `eprintln!` on failure. If workspace loading fails, report the failure to stderr only because the file logger is not safely available yet. Emit `application_started`, `application_ready`, `application_exit_requested`, and `application_stopped`. Log metrics initialization errors without changing their existing return/fallback behavior.

In the coordinator thread, keep one inventory, emit deduplicated scan entries, then call `emit_runtime_event` for every enriched event before sending the unchanged event to the frontend.

- [ ] **Step 5: Run GREEN**

```bash
cargo test --manifest-path src-tauri/Cargo.toml runtime_log::tests --lib
cargo test --manifest-path src-tauri/Cargo.toml tests::runtime_event --lib
cargo test --manifest-path src-tauri/Cargo.toml coordinator::tests --lib
```

Expected: all selected tests pass; unchanged scans create no duplicate entries.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/runtime_log.rs src-tauri/src/lib.rs
git commit -m "feat: persist runtime and device events"
```

### Task 4: Configuration Operation Results

**Files:**
- Modify: `src-tauri/src/runtime_log.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write a failing pure result test**

```rust
#[test]
fn operation_entries_capture_result_without_payloads() {
    let success: Result<(), AppError> = Ok(());
    let failed: Result<(), AppError> = Err(AppError::new("invalid_assignment"));
    let context = json!({"deviceId": "device-1"});
    let succeeded = operation_entry(100, "runtime_assignment_saved", context.clone(), &success);
    let rejected = operation_entry(200, "runtime_assignment_saved", context, &failed);
    assert_eq!(succeeded.result.as_deref(), Some("succeeded"));
    assert_eq!(succeeded.level, RuntimeLogLevel::Info);
    assert_eq!(rejected.result.as_deref(), Some("failed"));
    assert_eq!(rejected.level, RuntimeLogLevel::Error);
    assert_eq!(rejected.detail.as_deref(), Some("invalid_assignment"));
}
```

- [ ] **Step 2: Run RED**

Run `cargo test --manifest-path src-tauri/Cargo.toml operation_entries_capture_result --lib`.

Expected: compilation fails because `operation_entry` does not exist.

- [ ] **Step 3: Implement the result-preserving wrapper**

```rust
pub(crate) fn operation<T>(
    timestamp_ms: u64,
    event: &str,
    context: Value,
    action: impl FnOnce() -> Result<T, crate::workspace::AppError>,
) -> Result<T, crate::workspace::AppError> {
    let result = action();
    emit(operation_entry(timestamp_ms, event, context, &result));
    result
}
```

`operation_entry` emits `succeeded`/info for `Ok` and `failed`/error with only `AppError.code` as detail for `Err`. It must not serialize `AppError.detail`.

- [ ] **Step 4: Wrap every mutating Tauri command**

Use this exact event/context mapping:

| Command | Event | Context fields |
|---|---|---|
| `retry_candidate` | `device_candidate_retry` | `deviceId` |
| `save_device_profile` | `device_profile_saved` | `deviceProfileId` |
| `create_device_profile` | `device_profile_created` | `kind` plus `sourceProfileId` or `boardProfileId` |
| `save_settings` | `settings_saved` | `schemaVersion`, `editorProfile`, `language` |
| `rename_device` | `device_renamed` | `deviceId` |
| `save_runtime_assignment` | `runtime_assignment_saved` | `deviceId`, `deviceProfileId`, `hardwareProfileId` |
| `complete_device_setup` | `device_setup_completed` | `deviceId`, `deviceProfileId`, `hardwareProfileId` |
| `clear_runtime_assignment` | `runtime_assignment_cleared` | `deviceId` |
| `forget_device` | `device_forgotten` | `deviceId` |
| `begin_learning` | `learning_started` | device/profile/hardware IDs, `editingRevision`, `pinCount` |
| `end_learning` | `learning_ended` | `deviceId` |
| `import_device_profile` | `device_profile_imported` | empty object |
| `export_device_profile` | `device_profile_exported` | `deviceProfileId` |
| `delete_device_profile` | `device_profile_deleted` | `deviceProfileId` |
| `export_backup` | `backup_exported` | empty object |
| `restore_backup` | `backup_restored` | empty object |

For example:

```rust
let context = serde_json::json!({
    "deviceId": device_id,
    "deviceProfileId": assignment.device_profile_id,
    "hardwareProfileId": assignment.hardware_profile_id,
});
runtime_log::operation(now_ms(), "runtime_assignment_saved", context, || {
    save_runtime_assignment_inner(&state, &device_id, assignment)
})
```

Do not include profile bodies, device names, paste text, open targets, file paths, or backup contents.

- [ ] **Step 5: Run GREEN**

```bash
cargo test --manifest-path src-tauri/Cargo.toml operation_entries_capture_result --lib
cargo test --manifest-path src-tauri/Cargo.toml tests:: --lib
cargo test --manifest-path src-tauri/Cargo.toml workspace::tests --lib
```

Expected: all selected tests pass and command errors remain unchanged.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/runtime_log.rs src-tauri/src/lib.rs
git commit -m "feat: log runtime configuration operations"
```

### Task 5: Rotation Integration And Full Verification

**Files:**
- Create: `src-tauri/tests/runtime_log_rotation.rs`
- Modify only when verification exposes a defect: files from Tasks 1-4

- [ ] **Step 1: Write the real rotation test**

Create a single integration-test binary so its process-global logger cannot conflict with unit tests:

```rust
use std::fs;
use tauri_plugin_log::{
    Builder, RotationStrategy, Target, TargetKind,
    log::{LevelFilter, info},
};

#[test]
fn official_plugin_bounds_rotated_runtime_logs() {
    let directory = tempfile::tempdir().unwrap();
    let app = tauri::test::mock_builder()
        .plugin(
            Builder::new()
                .level(LevelFilter::Info)
                .clear_format()
                .max_file_size(256)
                .rotation_strategy(RotationStrategy::KeepSome(3))
                .targets([Target::new(TargetKind::Folder {
                    path: directory.path().to_path_buf(),
                    file_name: Some("kivo".into()),
                })])
                .build(),
        )
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();

    for sequence in 0..100 {
        info!(
            target: "kivo::runtime",
            "{{\"timestampMs\":{sequence},\"event\":\"rotation_probe\",\"padding\":\"0123456789012345678901234567890123456789\"}}"
        );
    }
    tauri_plugin_log::log::logger().flush();

    let files = fs::read_dir(directory.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("kivo"))
        .collect::<Vec<_>>();
    assert!(!files.is_empty());
    assert!(files.len() <= 3, "rotation left {} files", files.len());
    drop(app);
}
```

- [ ] **Step 2: Verify real rotation**

Run `cargo test --manifest-path src-tauri/Cargo.toml --test runtime_log_rotation -- --nocapture`.

Expected: one test passes and no more than three matching files remain.

- [ ] **Step 3: Run complete Rust verification**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

Expected: every command exits 0 with zero failures or warnings.

- [ ] **Step 4: Run repository regression checks**

```bash
npm test -- --run
npm run build
git diff --check
```

Expected: frontend tests and build pass, and whitespace validation is clean.

- [ ] **Step 5: Inspect privacy and scope**

```bash
rg -n "action_step_(started|completed)|characterCount|targetKind" src-tauri/src src-tauri/tests
rg -n "text|target" src-tauri/src/runtime_log.rs
git status --short
git diff --stat HEAD
```

Expected: action log records contain only counts and kinds; `runtime_log.rs` never serializes text or open targets; the unrelated pre-existing `uv.lock` modification remains untouched.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/tests/runtime_log_rotation.rs src-tauri/src src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "test: verify bounded runtime log rotation"
```
