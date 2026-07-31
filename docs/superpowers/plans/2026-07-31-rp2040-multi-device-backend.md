# RP2040 Multi-Device Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Kivo's singleton ESP32-S3 runtime with protocol-v3 discovery, durable per-Device assignments, independent serial sessions, globally ordered paste transactions, targeted learning, event-time metrics, and atomic full backup/restore for any compiled Board Profile.

**Architecture:** `hardware.rs` is the compiled Controller Family and Board Profile registry. `profile.rs` owns portable Device Profiles and board-specific Hardware Profiles. `workspace.rs` owns version-2 persistent configuration and known Devices. `coordinator.rs` reconciles USB observations into stable Device IDs and supervises one `device.rs` worker per runtime Device. One central event loop assigns host receive order and one `paste.rs` coordinator protects the global clipboard.

**Tech Stack:** Rust 2024, Tauri 2.11, `serialport`, `nusb`, `serde`, `serde_yaml_ng`, `rusqlite`, standard threads/channels, Cargo tests.

## Global Constraints

- Protocol, Device Profile, settings, and backup schemas are version 3/2/2/2 only; no pre-release migration or fallback parser remains.
- Device ID is derived only from exact Board Profile ID plus non-empty hardware serial. Port paths and firmware build IDs never participate.
- Invalid and duplicate identities are visible but never enrolled, assigned, configured, learned from, or allowed to execute actions.
- Runtime and bootloader observations with the same Board Profile and serial reconcile to one known Device. An unknown bootloader stays ephemeral.
- Every worker has independent serial state, topology revision, action queue, timeout, controls, errors, and learning state.
- One Device failure must not mutate another Device's status or assignment.
- Paste is globally FIFO from host receive order through clipboard write, `PASTE`, and matching `DONE`/timeout. Hotkeys stay per-Device and concurrent.
- Physical database foreign keys are not introduced. Restore validates all logical references before activation.
- Every test, build, and Git command is prefixed with `rtk`.

---

### Task 1: Add Extensible Hardware Registries And Stable Device Identity

**Files:**
- Create: `src-tauri/src/hardware.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

**Interfaces:**
- Produces: `ControllerFamily`, `BoardProfile`, `UsbIdentity`, `UsbMode`, `DeviceId`, `CONTROLLER_FAMILIES`, `BOARD_PROFILES`, `board_by_runtime_usb`, `board_by_bootloader_usb`, and `board_by_id`.
- Consumes: Runtime identities `303a:4002`, `2e8a:102e`; bootloader identity `2e8a:0003`; exact safe-pin registries.

- [ ] **Step 1: Write failing registry and Device ID tests**

Add module tests covering both real boards plus synthetic extension entries:

```rust
#[test]
fn registries_classify_modes_without_family_branches() {
    let esp = board_by_runtime_usb(0x303a, 0x4002).unwrap();
    assert_eq!(esp.family_id, "esp32s3");
    assert_eq!(esp.id, "luatos-esp32s3-aio");
    let rp = board_by_runtime_usb(0x2e8a, 0x102e).unwrap();
    assert_eq!(rp.family_id, "rp2040");
    assert_eq!(board_by_bootloader_usb(0x2e8a, 0x0003), Some(rp));
    assert!(board_by_runtime_usb(0x2e8a, 0x0003).is_none());
}

#[test]
fn device_id_ignores_port_and_round_trips() {
    let id = DeviceId::new("vccgnd-yd-rp2040", "E0C9125B0D9B").unwrap();
    assert_eq!(DeviceId::parse(id.as_str()).unwrap(), id);
    assert_eq!(id.board_profile_id(), "vccgnd-yd-rp2040");
    assert_eq!(id.hardware_serial(), "E0C9125B0D9B");
}
```

The extension contract test constructs a test-only slice with a second RP2040 board and an `esp32c3` family, then runs the same generic lookup and Device ID helpers. It must not add `match "rp2040"` or `match "esp32c3"` in production orchestration.

- [ ] **Step 2: Run the focused test and verify the module is missing**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml hardware::tests`

Expected: FAIL because `hardware` and its types do not exist.

- [ ] **Step 3: Implement compiled registries**

Use immutable entries with stable string IDs:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsbMode { Runtime, Bootloader }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsbIdentity { pub vid: u16, pub pid: u16, pub mode: UsbMode }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControllerFamily { pub id: &'static str, pub display_name: &'static str }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardProfile {
    pub id: &'static str,
    pub family_id: &'static str,
    pub display_name: &'static str,
    pub runtime_usb: UsbIdentity,
    pub bootloader_usb: Option<UsbIdentity>,
    pub safe_pins: &'static [u8],
    pub firmware_environment: &'static str,
}
```

The ESP32-S3 safe pins stay `[0,1,2,3,4,5,6,7,8,9,12,13,14,15,16,17,18]`; YD-RP2040 is every integer `0..=22`. Validate at test time that IDs are unique, every Board Profile references one Controller Family, USB identities do not collide within a mode, and safe pins are non-empty and unique.

Encode `DeviceId` injectively as `<decimal-board-byte-length>:<board-id><hardware-serial>`. `DeviceId::new` trims neither component, rejects empty values, rejects control/whitespace characters, verifies the Board Profile exists, and stores the raw board and serial alongside the canonical string. Parsing uses the byte length, not delimiter guessing. Implement `Serialize`/`Deserialize` as that canonical string so it is a stable YAML map key; deserialization always calls the same validating parser.

- [ ] **Step 4: Add USB enumeration support for bootloader-only devices**

Add `nusb = "0.2.5"` to dependencies. Use it only through a later `UsbEnumerator` adapter; do not open a ROM bootloader interface. `serialport` remains the CDC runtime enumerator.

- [ ] **Step 5: Run tests and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml hardware::tests`

Expected: PASS for real identities, safe sets, Device ID round-trip, and synthetic extensions.

```bash
rtk git add src-tauri/src/hardware.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
rtk git commit -m "feat: define controller and board registries"
```

---

### Task 2: Parse And Validate Protocol-v3 Capabilities

**Files:**
- Modify: `src-tauri/src/protocol.rs`
- Modify: `src-tauri/src/device.rs`

**Interfaces:**
- Produces: `HelloCapabilities { protocol, controller_family_id, board_profile_id, firmware_build_id, pins }` and `validate_hello(candidate_board, hello)`.
- Consumes: `HELLO 3 <family> <board> <build> <count> <pins...>` and a VID/PID-classified Board Profile.

- [ ] **Step 1: Replace v2 parser tests with strict v3 cases**

Cover one valid ESP32-S3 HELLO, one valid full RP2040 HELLO, one valid RP2040 subset, and rejection of v2, empty/whitespace build, wrong token count, empty pins, duplicates, out-of-range integers, wrong family, wrong board, and GPIO23 for YD-RP2040.

```rust
#[test]
fn parses_protocol_v3_identity_and_build() {
    let message = parse_device(
        "HELLO 3 rp2040 vccgnd-yd-rp2040 0.1.0+gabc1234 3 0 11 22",
    ).unwrap();
    assert_eq!(message, DeviceMessage::Hello(HelloCapabilities {
        protocol: 3,
        controller_family_id: "rp2040".into(),
        board_profile_id: "vccgnd-yd-rp2040".into(),
        firmware_build_id: "0.1.0+gabc1234".into(),
        pins: vec![0, 11, 22],
    }));
}
```

- [ ] **Step 2: Run protocol tests and see v3 fail**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml protocol::tests`

Expected: FAIL because the parser still expects v2's platform/count shape.

- [ ] **Step 3: Implement syntax parsing separately from board validation**

`parse_device` parses the v3 token shape, requires protocol `3`, one non-whitespace build token, a non-zero count equal to the pin count, unique `u8` pins, and no trailing tokens. `validate_hello` then requires exact family and board IDs from the classified Board Profile and ensures every reported pin is in `safe_pins`. Return structured error codes `protocol_mismatch`, `controller_family_mismatch`, `board_profile_mismatch`, or `capability_mismatch`.

Remove `DeviceCapabilities.platform` and all protocol-2 checks. Firmware build remains live diagnostics only and is not passed to `DeviceId` or assignment validation.

- [ ] **Step 4: Make topology generation consume one Hardware Profile**

Change the topology entry point to:

```rust
pub fn topology_commands(
    hardware: &HardwareProfile,
    revision: u32,
    reported_pins: &BTreeSet<u8>,
) -> Result<Vec<String>, AppError>
```

Validate every owned/bound pin against both the Hardware Profile's Board Profile safe set and the Device-reported subset before emitting `CONFIG_BEGIN`. A missing unreferenced safe pin remains valid.

- [ ] **Step 5: Run tests and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml protocol::tests`

Expected: PASS with no accepted `HELLO 2` case.

```bash
rtk git add src-tauri/src/protocol.rs src-tauri/src/device.rs
rtk git commit -m "feat: require board-aware protocol v3"
```

---

### Task 3: Replace Singleton Model Settings With Version-2 Profiles And Devices

**Files:**
- Create: `src-tauri/src/profile.rs`
- Create: `models/prod/tel001.yaml`
- Delete: `models/prod/tel001.json`
- Delete: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/workspace.rs`
- Modify: `src-tauri/src/model.rs`
- Modify: `src-tauri/src/protocol.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tauri.conf.json`

**Interfaces:**
- Produces: schema-v2 `DeviceProfile`, `HardwareProfile`, `RuntimeAssignment`, `DeviceRecord`, `SettingsDocument`, and workspace validation/mutation APIs.
- Removes: `ModelConfig.hardware`, `active_model`, `LegacyPaths`, legacy config migration, and schema-v1 import acceptance.

- [ ] **Step 1: Write failing version-2 serialization and validation tests**

Define the expected domain shape:

```rust
pub struct DeviceProfile {
    pub schema_version: u16,
    pub profile: ModelLayout,
    pub hardware_profiles: Vec<HardwareProfile>,
    pub actions: BTreeMap<String, Vec<ButtonAction>>,
}

pub struct HardwareProfile {
    pub id: String,
    pub name: String,
    pub board_profile_id: String,
    pub debounce_ms: u16,
    pub inputs: Vec<InputSource>,
}

pub struct RuntimeAssignment {
    pub device_profile_id: String,
    pub hardware_profile_id: String,
}

pub struct DeviceRecord {
    pub device_id: DeviceId,
    pub name: String,
    pub board_profile_id: String,
    pub runtime_assignment: Option<RuntimeAssignment>,
}

pub struct SettingsDocument {
    pub schema_version: u16,
    pub editor_profile: Option<String>,
    pub language: Language,
    pub devices: BTreeMap<DeviceId, DeviceRecord>,
}
```

Tests must prove: multiple Hardware Profiles can target the same board; one Device Profile can target both real boards; an assignment requires exact board equality; deleting/incompatibly editing a Hardware Profile retains the assignment as invalid; schema v1 rejects with `unsupported_*_schema`; duplicate Hardware Profile IDs reject; and settings reject key/embedded Device ID mismatch, malformed IDs, and unknown Board Profiles.

- [ ] **Step 2: Run workspace tests and verify the new schema is absent**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml workspace::tests`

Expected: FAIL because version-1 singleton fields are still compiled.

- [ ] **Step 3: Implement profile validation against the hardware registry**

Move `ButtonAction`, `InputSource`, and hardware validation to `profile.rs`. Keep layout validation in `model.rs`. Validate IDs, debounce `1..=1000`, button bindings, matrix bipartiteness, actions, and every pin against `board_by_id(hardware.board_profile_id).safe_pins`. Expose:

```rust
impl DeviceProfile {
    pub fn hardware_profile(&self, id: &str) -> Option<&HardwareProfile>;
    pub fn compatible_hardware(&self, board_id: &str) -> Vec<&HardwareProfile>;
    pub fn button_for(&self, hardware_id: &str, input: &PhysicalInput) -> Option<&str>;
}
```

- [ ] **Step 4: Implement settings, enrollment, assignment, and forget transactions**

Set all schema constants to `2`. On load, reject old documents rather than migrate them. Add workspace methods:

```rust
pub fn enroll_device(&mut self, id: DeviceId) -> Result<&DeviceRecord, AppError>;
pub fn rename_device(&mut self, id: &DeviceId, name: String) -> Result<(), AppError>;
pub fn set_assignment(&mut self, id: &DeviceId, value: RuntimeAssignment) -> Result<(), AppError>;
pub fn clear_assignment(&mut self, id: &DeviceId) -> Result<(), AppError>;
pub fn forget_offline_device(&mut self, id: &DeviceId, online: bool) -> Result<(), AppError>;
pub fn assignment_resolution(&self, id: &DeviceId) -> AssignmentResolution<'_>;
```

Enrollment is idempotent and names a new record `<Board display name> · <last six serial characters>`. `set_assignment` requires the Device Profile and Hardware Profile to exist and match the Device Board Profile. `assignment_resolution` preserves broken references as `InvalidAssignment`; it never chooses another compatible profile. Forget rejects `online == true` and removes only the Device record.

- [ ] **Step 5: Replace the bundled product artifact with a complete v2 profile**

Convert `models/prod/tel001.json` into `tel001.yaml` containing the same layout, empty actions, and one `luatos-esp32s3-aio` Hardware Profile named `LuatOS ESP32-S3` with an empty `inputs` list. The profile is editable but cannot drive input until its bindings are configured. Update Tauri resources to bundle `models/prod/*.yaml` and make the loader accept only YAML v2 profiles.

- [ ] **Step 6: Run workspace and complete Rust tests**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml workspace::tests`

Expected: PASS for schema v2, enrollment, exact assignments, invalid-reference retention, and forget rules.

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml`

Expected: remaining failures are limited to singleton runtime/lib call sites scheduled in later tasks; no schema-v1 test remains as a success case.

- [ ] **Step 7: Commit the persistent domain change**

```bash
rtk git add src-tauri/src/profile.rs src-tauri/src/workspace.rs src-tauri/src/model.rs src-tauri/src/protocol.rs src-tauri/src/lib.rs src-tauri/tauri.conf.json models/prod/tel001.yaml models/prod/tel001.json src-tauri/src/config.rs
rtk git commit -m "feat: persist device profiles and runtime assignments"
```

---

### Task 4: Attribute Metrics By Device And Include Them In Atomic Backups

**Files:**
- Modify: `src-tauri/src/metrics.rs`
- Modify: `src-tauri/src/workspace.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: event-time `MetricAttribution`, per-profile/per-device queries, `MetricsBackup`, expanded `BackupDocument`, and staged configuration-plus-metrics restore.
- Consumes: Device ID, event-time Device name, Device Profile ID, Hardware Profile ID, button ID, and timestamp.

- [ ] **Step 1: Write failing attribution and backup tests**

Test two Devices assigned to one Device Profile, one reassignment, one forgotten Device, and one restore failure. Assert that the Home query sums both Devices, a Device filter returns only one, old rows retain old profile/name attribution after reassignment/forget, backup round-trips aggregates and at most 500 activity rows, and a staged restore failure leaves both original settings and original metrics visible.

- [ ] **Step 2: Run metrics tests and verify the singleton schema fails**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml metrics::tests`

Expected: FAIL because metrics are currently keyed only by model/button and logs carry no attribution.

- [ ] **Step 3: Replace the pre-release SQLite schema**

Because the app is unreleased, create only the v2 tables and reject/reset an existing v1 metrics file during development. Use these logical keys without SQL foreign keys:

```text
button_metrics(device_profile_id, device_id, button_id, total_presses, last_pressed_at_ms)
button_metric_days(device_profile_id, device_id, button_id, day, presses)
activity_logs(id, occurred_at_ms, kind, message, device_id, device_name,
              device_profile_id, hardware_profile_id, button_id)
```

Primary keys are `(device_profile_id, device_id, button_id)` and `(device_profile_id, device_id, button_id, day)`. Keep the newest 500 activity rows globally. `record_button_press` writes aggregates and its activity row in one SQLite transaction.

- [ ] **Step 4: Add profile aggregate and Device filter queries**

Use:

```rust
pub fn home_snapshot(
    &self,
    device_profile_id: &str,
    device_id: Option<&DeviceId>,
    now_ms: u64,
) -> Result<HomeMetricsSnapshot, rusqlite::Error>;
```

All aggregate predicates include Device Profile ID; add Device ID only when filtering. Activity DTOs expose the event-time identity and assignment snapshot.

- [ ] **Step 5: Put metrics inside the persistent data generation and stage restore**

Move the SQLite path to `<config>/data/metrics.sqlite3`. Full backup serializes settings, Device Profiles, all aggregate rows, and the retained activity rows into schema-v2 `BackupDocument`. Device Profile export still serializes only one `DeviceProfile`.

Restore holds the workspace/runtime write lock, validates the complete backup, writes YAML plus a newly populated SQLite file under `<config>/data.next`, closes the staged DB, then swaps directories: `data -> data.previous`, `data.next -> data`. If activation or reopening metrics fails, rename `data.previous -> data` before releasing the lock. Delete `data.previous` only after workspace and metrics reopen successfully, then resynchronize workers from the restored snapshot.

- [ ] **Step 6: Run tests and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml metrics::tests workspace::tests`

Expected: PASS for multi-Device aggregation, immutable historical attribution, bounded logs, backup boundary, and rollback.

```bash
rtk git add src-tauri/src/metrics.rs src-tauri/src/workspace.rs src-tauri/src/lib.rs
rtk git commit -m "feat: attribute metrics and backup complete state"
```

---

### Task 5: Reconcile USB Observations Into Independent Device Workers

**Files:**
- Create: `src-tauri/src/coordinator.rs`
- Create: `src-tauri/src/paste.rs`
- Modify: `src-tauri/src/device.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/tray.rs`

**Interfaces:**
- Produces: `UsbEnumerator`, `SerialObservation`, `BootloaderObservation`, `RuntimeCoordinator`, `DeviceStatus`, `CandidateStatus`, `WorkerCommand`, `WorkerEvent`, and `PasteCoordinator`.
- Removes: global `active_model`, `connection`, `capabilities`, `runtime_error`, `learning`, and a single `worker` handle.

- [ ] **Step 1: Write failing reconciliation tests with a fake enumerator**

Cover: two ESP32-S3 plus two RP2040 runtime observations start four workers; a port rename starts none; one departure stops only one; missing serial is quarantined; duplicate Device IDs quarantine both and stop an existing worker; runtime/bootloader same identity changes one known row's mode; unknown bootloader remains a candidate; valid unknown runtime enrolls once unassigned; one open/handshake failure leaves other workers alive.

Use an injected trait so tests never touch physical USB:

```rust
pub trait UsbEnumerator: Send + Sync {
    fn serial_ports(&self) -> Result<Vec<SerialObservation>, String>;
    fn usb_devices(&self) -> Result<Vec<BootloaderObservation>, String>;
}
```

- [ ] **Step 2: Run coordinator tests and verify the singleton worker fails**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml coordinator::tests`

Expected: FAIL because `RuntimeCoordinator` does not exist.

- [ ] **Step 3: Implement generic discovery and identity quarantine**

Production `SystemUsbEnumerator` reads runtime candidates from `serialport::available_ports()` and bootloader identities from `nusb::list_devices()`. Classify through registry functions only. Runtime candidates require serial before port open; bootloader candidates never open as serial.

Build all observations for a scan before reconciliation. Group valid identities by Device ID; a group size greater than one marks every observation `duplicate_identity` and ensures no worker owns that ID. Missing serial produces a candidate-only `invalid_identity`. Persist enrollment only after exact v3 HELLO validation succeeds.

- [ ] **Step 4: Split serial I/O from coordinator event ordering**

Each worker owns its port, `DeviceSession`, current immutable assignment snapshot, revision, control receiver, action timeout, and stop flag. It emits parsed events to the central coordinator channel. The coordinator assigns a monotonically increasing `u64 receive_sequence` when it dequeues each `WorkerEvent::Input`; it sends that sequence back in the Device-specific action command. No worker or UI sorts by port, family, board, or timestamp.

Define status source dimensions exactly:

```rust
pub enum ConnectionDimension { Online, Offline }
pub enum DeviceMode { Runtime, Bootloader }
pub enum IdentityDimension { Validating, Valid, InvalidIdentity, DuplicateIdentity }
pub enum AssignmentDimension { Unassigned, Valid, InvalidAssignment }
pub enum RuntimeDimension { Inactive, Configuring, Learning, Ready, RuntimeError }
```

`DeviceStatus` carries `mode: Option<DeviceMode>` because mode exists only while online; Offline status uses `None`. It also carries raw serial, port, family ID, board ID, firmware build, pins, assignment, latest error, and optional learning session. Persist none of the live fields.

- [ ] **Step 5: Implement the global paste transaction coordinator**

Workers submit `PasteRequest { receive_sequence, device_id, event_id, step, text, reply }`. The paste coordinator processes requests strictly by receive sequence. For one request it writes the clipboard, grants that Device permission to send `PASTE`, and does not release the slot until the worker reports matching `DONE` or its 1800 ms action timeout. It never coalesces requests. A timeout marks only the source Device, tells its session to abort/advance, and starts the next request.

Hotkey steps bypass the paste coordinator. A Device's action sequence waits for its paste completion before its following hotkey, preserving `Paste -> Enter` ordering while another Device's hotkey can complete.

- [ ] **Step 6: Implement isolated disconnect and shutdown**

On disconnect, stop only that worker, clear only its live capabilities/controls/pending actions, cancel only its learning session, and notify the paste coordinator if it owns the active slot. Keep its persisted record and assignment Offline. On application exit, signal the coordinator, join every worker, then join the coordinator and paste threads. Tray summary derives from the whole registry instead of one connection.

- [ ] **Step 7: Run focused and complete Rust tests**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml coordinator::tests device::tests paste::tests`

Expected: PASS for four workers, identity quarantine, bootloader reconciliation, FIFO paste, timeout release, and Device-local failure.

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 8: Commit the multi-Device runtime**

```bash
rtk git add src-tauri/src/coordinator.rs src-tauri/src/paste.rs src-tauri/src/device.rs src-tauri/src/lib.rs src-tauri/src/tray.rs
rtk git commit -m "feat: run independent sessions for every device"
```

---

### Task 6: Apply Profile Edits Live And Target Learning To One Device

**Files:**
- Modify: `src-tauri/src/coordinator.rs`
- Modify: `src-tauri/src/device.rs`
- Modify: `src-tauri/src/profile.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `ProfileChange`, immutable `RuntimeProfileSnapshot`, revision-aware reconfiguration, `LearningTarget`, and Device-specific controls.

- [ ] **Step 1: Write failing live-update and learning tests**

Test action-only edits with two assigned Devices, topology edits affecting only one Hardware Profile, stale `CONFIG_OK`, per-Device configuration failure, in-flight old action snapshot, exact Device learning, disconnect cancellation, editor draft behavior, and save-triggered reconfiguration of all Devices assigned to the learned Hardware Profile.

- [ ] **Step 2: Run tests and verify current replacement behavior is too broad**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml live_update learning`

Expected: FAIL because the current backend replaces one global model and broadcasts one learning queue.

- [ ] **Step 3: Classify successful profile changes**

After persistence, compare the old and new Device Profile. Layout/actions changes produce an atomically swapped host mapping for every assignment to that Device Profile and send no topology. Each in-flight action keeps its `Arc<RuntimeProfileSnapshot>`; the next input reads the new snapshot.

A debounce/input/binding change carries the exact Hardware Profile ID. Only matching assigned workers settle their current action, reject new input, increment their own non-zero revision, enter Configuring, and send the new topology. Only matching `CONFIG_OK <revision>` returns Ready; stale acknowledgements are ignored.

- [ ] **Step 4: Bind learning to the complete target tuple**

```rust
pub struct LearningTarget {
    pub device_id: DeviceId,
    pub device_profile_id: String,
    pub hardware_profile_id: String,
    pub editing_revision: u64,
    pub firmware_revision: u32,
    pub pins: Vec<u8>,
}
```

Begin requires an online valid runtime Device, exact Board Profile match, candidate pins within both board and reported capabilities, and no active session for that Device. Pause/configure only the target. Captures are emitted with the target tuple and update frontend draft only. End/cancel/disconnect restores that Device's saved assignment topology; it does not persist or fan out the draft. Normal profile save later applies the ordinary live-update path.

- [ ] **Step 5: Run tests and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml live_update learning`

Expected: PASS for independent revisions, old/new snapshots, stale ack rejection, and targeted draft learning.

```bash
rtk git add src-tauri/src/coordinator.rs src-tauri/src/device.rs src-tauri/src/profile.rs src-tauri/src/lib.rs
rtk git commit -m "feat: live-apply profiles and target device learning"
```

---

### Task 7: Expose Structured Snapshots And Atomic Device Commands

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/coordinator.rs`
- Modify: `src-tauri/src/metrics.rs`

**Interfaces:**
- Produces: frontend `AppSnapshot`, Device commands, filtered metrics command, and Device-attributed runtime events.

- [ ] **Step 1: Write failing command-boundary tests**

Assert the snapshot contains `deviceProfiles`, `editorProfile`, `boardProfiles`, `devices`, `candidates`, `homeMetrics`, and no singleton `connection`, `supportedGpios`, `runtimeError`, or `learning`. Test each mutation against exactly one Device ID and ensure invalid identity candidates cannot call it.

- [ ] **Step 2: Define the command API**

Register these exact commands:

```text
get_snapshot()
save_device_profile(profile)
save_settings(settings)
rename_device(device_id, name)
save_runtime_assignment(device_id, assignment)
clear_runtime_assignment(device_id)
forget_device(device_id)
get_device_metrics(device_id)
begin_learning(device_id, device_profile_id, hardware_profile_id, editing_revision, pins)
end_learning(device_id)
```

Keep import/export/delete/backup/restore commands but update their v2 DTOs and names from model to Device Profile at the serialized boundary. Assignment save accepts the pair in one request and validates under one workspace lock.

`save_settings` accepts an `EditorSettingsPatch { schema_version, editor_profile, language }`, not the full persisted `SettingsDocument`. Under the workspace lock it updates those fields while preserving the authoritative Device records and Runtime Assignments, so a stale frontend snapshot cannot overwrite discovery or Device Management changes.

- [ ] **Step 3: Attribute all events**

Every `runtime-event` includes timestamp, level, Device ID, raw serial, Controller Family, Board Profile, current port, event-time Device Profile ID, Hardware Profile ID, activity, and optional Home update. Only events whose Device Profile equals the Editor Profile may drive keypad press highlighting in the frontend.

- [ ] **Step 4: Synchronize commands with the coordinator**

Persist first. On success, send one immutable workspace revision to the coordinator, let it resolve every assignment generically, and return the resulting snapshot. Persistence/validation errors do not change worker state. Restore replaces coordinator configuration only after staged workspace+metrics activation succeeds.

- [ ] **Step 5: Run all Rust tests and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS with protocol v3 only, four concurrent fake Devices, structured dimensions, targeted commands, metrics attribution, and atomic restore.

Run: `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: PASS with no warnings.

```bash
rtk git add src-tauri/src/lib.rs src-tauri/src/coordinator.rs src-tauri/src/metrics.rs
rtk git commit -m "feat: expose multi-device application state"
```

---

### Task 8: Prove The Future-Controller Extension Boundary

**Files:**
- Modify: `src-tauri/src/hardware.rs`
- Modify: `src-tauri/src/coordinator.rs`
- Modify: `src-tauri/src/workspace.rs`
- Create: `docs/verification/2026-07-31-device-registry-contract.md`

**Interfaces:**
- Consumes: Test-only second RP2040 board and test-only ESP32-C3 family/board entries.
- Produces: Evidence that discovery, Device ID, enrollment, assignment filtering, snapshots, and metrics require no Device-schema or UI-specific branch.

- [ ] **Step 1: Add a generic registry fixture constructor**

Refactor lookup/reconciliation tests to accept `HardwareRegistry<'_>` rather than directly reading globals. Production passes the compiled constants; tests pass constants plus `test-rp2040-board` and `test-esp32c3-board`.

- [ ] **Step 2: Exercise both extension cases end to end**

For the extra RP2040 board, classify, handshake, enroll, assign, and reach Ready using existing family behavior. For ESP32-C3, prove the same domain flow after registering its family and board, while the test adapter supplies its protocol-compatible serial worker. Assert serialized `DeviceStatus`, `RuntimeAssignment`, metrics keys, and commands have identical shapes across all boards.

- [ ] **Step 3: Enforce absence of orchestration branches**

Run: `rtk proxy rg -n '"esp32s3"|"rp2040"|"esp32c3"|"luatos-esp32s3-aio"|"vccgnd-yd-rp2040"' src-tauri/src/coordinator.rs src-tauri/src/device.rs src-tauri/src/workspace.rs src-tauri/src/lib.rs`

Expected: no matches. Hardware-specific strings may exist only in `hardware.rs`, firmware adapters, and tests.

- [ ] **Step 4: Record and commit the contract evidence**

Record the test names, command outputs, and searched files in the verification document.

```bash
rtk git add src-tauri/src/hardware.rs src-tauri/src/coordinator.rs src-tauri/src/workspace.rs docs/verification/2026-07-31-device-registry-contract.md
rtk git commit -m "test: prove device registry extension boundary"
```
