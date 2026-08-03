# Device Onboarding and Profile Creation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a guided keyboard setup flow that explains RP2040 firmware problems, retries exact candidates, creates clone or blank Device Profiles independently, and atomically binds validated devices without adding firmware flashing.

**Architecture:** The Runtime Coordinator remains the authority for USB discovery and HELLO validation, but exports a structured `CandidateIssue` and an exact retry operation. Workspace gains create-only profile persistence and one-transaction device completion; React consumes only authoritative `AppSnapshot` values through a reusable profile form and a centralized setup wizard, while `App` owns insertion-cycle auto-open state.

**Tech Stack:** Rust, Serde, Tauri 2 commands, React 19, TypeScript, Vitest, Testing Library, lucide-react, CSS.

---

## Scope Guardrails

- Do not add UF2 files, `picotool`, PlatformIO, firmware build/install commands, or a flashing button.
- A Candidate cannot receive a `RuntimeAssignment`; only an online runtime Device with valid identity can finish setup.
- `/dev/cu.*` remains searchable but appears visually only inside collapsed technical details.
- Creating a Device Profile is durable and independent from completing physical-device setup.
- Every mutation returns and applies an authoritative `AppSnapshot`; do not create optimistic Device/Profile registry entries.
- Multiple physical Devices may point at the same Device Profile and Hardware Profile.

## File Map

- Modify `src-tauri/src/coordinator.rs`: candidate issue classification and exact Candidate retry.
- Modify `src-tauri/src/profile.rs`: serialized profile-creation request type and blank/clone constructors.
- Modify `src-tauri/src/workspace.rs`: create-only profile transaction and atomic name/assignment transaction.
- Modify `src-tauri/src/lib.rs`: Tauri command boundaries and authoritative snapshots.
- Modify `src/types.ts`: frontend command/request and Candidate issue types.
- Create `src/CreateDeviceProfileForm.tsx`: reusable clone/blank form with no registry ownership.
- Create `src/CreateDeviceProfileForm.test.tsx`: focused form validation and payload tests.
- Create `src/DeviceSetupWizard.tsx`: target selection, Candidate state, profile choice/create, confirmation.
- Create `src/DeviceSetupWizard.test.tsx`: wizard state transition and exact-command tests.
- Modify `src/App.tsx`: insertion-cycle orchestration, command callbacks, snapshot application, navigation.
- Modify `src/App.test.tsx`: auto-open/suppression, independent creation, and command integration tests.
- Modify `src/DeviceManagement.tsx`: friendly Candidate actions, continue setup, collapsed diagnostics, no port column.
- Modify `src/DeviceManagement.test.tsx`: device-list and Candidate recovery behavior.
- Modify `src/HomeDashboard.tsx`: show a friendly connected-keyboard name instead of a system port.
- Modify `src/i18n.ts`: complete Simplified Chinese and English labels/errors.
- Modify `src/styles/views.css`: stable wizard, form, details, table, desktop/mobile layout.
- Modify `src/styles/app.css`: include the new primary/setup commands in the existing shared button rule.

### Task 1: Export Structured Candidate Diagnostics

**Files:**
- Modify: `src-tauri/src/coordinator.rs`
- Modify: `src/types.ts`

- [ ] **Step 1: Write the failing Rust classification test**

Add these tests inside `src-tauri/src/coordinator.rs`'s existing `tests` module:

```rust
#[test]
fn candidate_issue_covers_identity_mode_and_worker_failures() {
    use CandidateIssue::*;

    assert_eq!(candidate_issue(DeviceMode::Runtime, IdentityDimension::Validating, None), Validating);
    assert_eq!(candidate_issue(DeviceMode::Bootloader, IdentityDimension::Valid, None), Bootloader);
    assert_eq!(candidate_issue(DeviceMode::Runtime, IdentityDimension::InvalidIdentity, None), InvalidIdentity);
    assert_eq!(candidate_issue(DeviceMode::Runtime, IdentityDimension::DuplicateIdentity, None), DuplicateIdentity);
    assert_eq!(candidate_issue(DeviceMode::Runtime, IdentityDimension::Validating, Some("serial_handshake_timeout")), FirmwareNotResponding);
    assert_eq!(candidate_issue(DeviceMode::Runtime, IdentityDimension::Validating, Some("protocol_mismatch")), FirmwareIncompatible);
    assert_eq!(candidate_issue(DeviceMode::Runtime, IdentityDimension::Validating, Some("controller_family_mismatch")), FirmwareIncompatible);
    assert_eq!(candidate_issue(DeviceMode::Runtime, IdentityDimension::Validating, Some("board_profile_mismatch")), FirmwareIncompatible);
    assert_eq!(candidate_issue(DeviceMode::Runtime, IdentityDimension::Validating, Some("capability_mismatch")), FirmwareIncompatible);
    assert_eq!(candidate_issue(DeviceMode::Runtime, IdentityDimension::Validating, Some("serial_open_failed: busy")), PortUnavailable);
    assert_eq!(candidate_issue(DeviceMode::Runtime, IdentityDimension::Validating, Some("serial_handshake_failed: denied")), PortUnavailable);
    assert_eq!(candidate_issue(DeviceMode::Runtime, IdentityDimension::Validating, Some("unclassified failure")), Unknown);
}

#[test]
fn candidate_status_serializes_issue_without_removing_raw_error() {
    let candidate = CandidateStatus {
        key: "runtime:/dev/cu.usbmodem1101".into(),
        device_id: None,
        mode: DeviceMode::Runtime,
        identity: IdentityDimension::Validating,
        issue: CandidateIssue::FirmwareNotResponding,
        raw_serial: Some("50031519384E811C".into()),
        port: Some("/dev/cu.usbmodem1101".into()),
        controller_family_id: "rp2040".into(),
        board_profile_id: crate::hardware::VCCGND_YD_RP2040_BOARD_ID.into(),
        latest_error: Some("serial_handshake_timeout".into()),
    };

    let value = serde_json::to_value(candidate).unwrap();
    assert_eq!(value["issue"], "firmware_not_responding");
    assert_eq!(value["latestError"], "serial_handshake_timeout");
}
```

- [ ] **Step 2: Run the test to verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml candidate_issue -- --nocapture`

Expected: FAIL because `CandidateIssue`, `candidate_issue`, and `CandidateStatus.issue` do not exist.

- [ ] **Step 3: Add the enum and centralized mapping**

Add beside the existing Candidate types in `src-tauri/src/coordinator.rs`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateIssue {
    Validating,
    FirmwareNotResponding,
    FirmwareIncompatible,
    Bootloader,
    PortUnavailable,
    InvalidIdentity,
    DuplicateIdentity,
    Unknown,
}
```

Add `pub issue: CandidateIssue` to `CandidateStatus`, then use this helper in both `candidate_from_runtime` and `candidate_from`:

```rust
fn candidate_issue(
    mode: DeviceMode,
    identity: IdentityDimension,
    latest_error: Option<&str>,
) -> CandidateIssue {
    match identity {
        IdentityDimension::InvalidIdentity => CandidateIssue::InvalidIdentity,
        IdentityDimension::DuplicateIdentity => CandidateIssue::DuplicateIdentity,
        IdentityDimension::Validating | IdentityDimension::Valid => {
            if mode == DeviceMode::Bootloader {
                return CandidateIssue::Bootloader;
            }
            match latest_error {
                None => CandidateIssue::Validating,
                Some("serial_handshake_timeout" | "device_disconnected") => {
                    CandidateIssue::FirmwareNotResponding
                }
                Some(
                    "protocol_mismatch"
                    | "controller_family_mismatch"
                    | "board_profile_mismatch"
                    | "capability_mismatch",
                ) => CandidateIssue::FirmwareIncompatible,
                Some(error)
                    if error.starts_with("serial_open_failed:")
                        || error.starts_with("serial_handshake_failed:")
                        || error.starts_with("serial_read_failed:") =>
                {
                    CandidateIssue::PortUnavailable
                }
                Some(_) => CandidateIssue::Unknown,
            }
        }
    }
}
```

Construct the field before moving `latest_error` in `candidate_from_runtime`:

```rust
let issue = candidate_issue(DeviceMode::Runtime, identity, latest_error.as_deref());
CandidateStatus {
    key: format!("runtime:{}", observation.port),
    device_id,
    mode: DeviceMode::Runtime,
    identity,
    issue,
    raw_serial: observation.serial_number.clone(),
    port: Some(observation.port.clone()),
    controller_family_id: board.family_id.into(),
    board_profile_id: board.id.into(),
    latest_error,
}
```

Use this complete construction in `candidate_from`:

```rust
let mode = observation.mode();
let issue = candidate_issue(mode, identity, latest_error.as_deref());
CandidateStatus {
    key: observation.key(),
    device_id,
    mode,
    identity,
    issue,
    raw_serial: observation.serial().map(str::to_owned),
    port: observation.port(),
    controller_family_id: observation.board().family_id.into(),
    board_profile_id: observation.board().id.into(),
    latest_error,
}
```

Whenever `handle_worker_event` or `reject_worker` changes a Candidate's `latest_error`, immediately assign:

```rust
candidate.issue = candidate_issue(
    candidate.mode,
    candidate.identity,
    candidate.latest_error.as_deref(),
);
```

Add the matching frontend union and field in `src/types.ts`:

```typescript
export type CandidateIssue =
  | "validating"
  | "firmware_not_responding"
  | "firmware_incompatible"
  | "bootloader"
  | "port_unavailable"
  | "invalid_identity"
  | "duplicate_identity"
  | "unknown";

export interface CandidateStatus {
  key: string;
  deviceId: string | null;
  mode: DeviceMode;
  identity: IdentityDimension;
  issue: CandidateIssue;
  rawSerial: string | null;
  port: string | null;
  controllerFamilyId: string;
  boardProfileId: string;
  latestError: string | null;
}
```

Update every existing Candidate fixture in `src/App.test.tsx` and `src/DeviceManagement.test.tsx` with the correct `issue` literal.

- [ ] **Step 4: Run focused and type verification**

Run: `cargo test --manifest-path src-tauri/Cargo.toml candidate_issue -- --nocapture && npm test -- src/App.test.tsx src/DeviceManagement.test.tsx && npm run build`

Expected: PASS; Candidate JSON contains both friendly classification and raw diagnostics.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/coordinator.rs src/types.ts src/App.test.tsx src/DeviceManagement.test.tsx
git commit -m "feat: classify candidate device issues"
```

### Task 2: Retry One Exact Candidate

**Files:**
- Modify: `src-tauri/src/coordinator.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing Coordinator tests**

Add to the existing Coordinator tests:

```rust
#[test]
fn retry_candidate_restarts_only_the_exact_identity() {
    let (_directory, enumerator, launcher, mut coordinator) = harness();
    launcher.set_hello(
        "/dev/a",
        HelloCapabilities {
            protocol: 3,
            controller_family_id: "wrong-family".into(),
            board_profile_id: crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID.into(),
            firmware_build_id: "bad".into(),
            pins: vec![0],
        },
    );
    enumerator.set(
        vec![
            serial("/dev/a", 0x303a, 0x4002, Some("RETRY-A")),
            serial("/dev/b", 0x303a, 0x4002, Some("RETRY-B")),
        ],
        Vec::new(),
    );
    scan(&mut coordinator);
    let a = DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "RETRY-A").unwrap();
    let b = DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "RETRY-B").unwrap();
    assert!(coordinator.candidates().iter().any(|candidate| candidate.device_id.as_ref() == Some(&a)));
    assert!(coordinator.devices().iter().any(|device| device.device_id == b));

    coordinator.retry_candidate(&a).unwrap();

    let starts_after = launcher.starts();
    assert_eq!(starts_after.iter().filter(|start| start.device_id == a).count(), 2);
    assert_eq!(starts_after.iter().filter(|start| start.device_id == b).count(), 1);
}

#[test]
fn retry_candidate_rejects_missing_and_duplicate_identity() {
    let (_directory, enumerator, _launcher, mut coordinator) = harness();
    let missing = DeviceId::new(crate::hardware::VCCGND_YD_RP2040_BOARD_ID, "MISSING").unwrap();
    assert_eq!(coordinator.retry_candidate(&missing).unwrap_err(), "candidate_not_found");

    enumerator.set(
        vec![
            serial("/dev/one", 0x2e8a, 0x102e, Some("DUPLICATE")),
            serial("/dev/two", 0x2e8a, 0x102e, Some("DUPLICATE")),
        ],
        Vec::new(),
    );
    scan(&mut coordinator);
    let duplicate = DeviceId::new(crate::hardware::VCCGND_YD_RP2040_BOARD_ID, "DUPLICATE").unwrap();
    assert_eq!(coordinator.retry_candidate(&duplicate).unwrap_err(), "candidate_identity_conflict");
}
```

- [ ] **Step 2: Run the test to verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml retry_candidate -- --nocapture`

Expected: FAIL because `RuntimeCoordinator::retry_candidate` does not exist.

- [ ] **Step 3: Implement exact retry in the Coordinator**

Add this public method beside `candidates()`:

```rust
pub fn retry_candidate(&mut self, device_id: &DeviceId) -> Result<(), String> {
    let matching = self
        .candidates
        .iter()
        .filter(|candidate| candidate.device_id.as_ref() == Some(device_id))
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return Err("candidate_not_found".into());
    }
    if matching.len() != 1 || matching[0].identity == IdentityDimension::DuplicateIdentity {
        return Err("candidate_identity_conflict".into());
    }
    if matching[0].mode != DeviceMode::Runtime
        || matching[0].identity == IdentityDimension::InvalidIdentity
    {
        return Err("candidate_not_retryable".into());
    }
    self.stop_worker(device_id);
    self.scan_once()
}
```

This rescans USB globally through the existing enumerator but retires/restarts only the addressed worker; the reconciliation logic reuses every unaffected worker.

- [ ] **Step 4: Add the command boundary and failing boundary assertion**

Add this launcher and test in `src-tauri/src/lib.rs`'s test module:

```rust
struct CandidateLauncher;

impl WorkerLauncher for CandidateLauncher {
    fn start(
        &self,
        _start: WorkerStart,
        _events: mpsc::Sender<WorkerEvent>,
    ) -> Result<Box<dyn DeviceWorker>, String> {
        Err("serial_handshake_timeout".into())
    }
}

#[test]
fn retry_candidate_command_returns_an_authoritative_snapshot() {
    let directory = TestDirectory::new();
    let mut state = product_state(&directory.0, vec![product_profile()]);
    let mut coordinator = RuntimeCoordinator::new(
        Arc::new(SaveEnumerator),
        Arc::new(CandidateLauncher),
        Arc::clone(&state.workspace),
    );
    coordinator.scan_once().unwrap();
    let id = hardware::DeviceId::new(
        crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID,
        "SAVE-A",
    ).unwrap();
    state.coordinator = Some(Arc::new(Mutex::new(coordinator)));

    let snapshot = retry_candidate_inner(&state, &id).unwrap();

    assert!(snapshot.candidates.iter().any(|candidate| {
        candidate.device_id.as_ref() == Some(&id) &&
        candidate.issue == coordinator::CandidateIssue::FirmwareNotResponding
    }));
    let missing = hardware::DeviceId::new(
        crate::hardware::VCCGND_YD_RP2040_BOARD_ID,
        "MISSING",
    ).unwrap();
    assert_eq!(retry_candidate_inner(&state, &missing).unwrap_err().code, "candidate_not_found");
}
```

Then add these functions:

Add these functions:

```rust
fn retry_candidate_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    let coordinator = state
        .coordinator
        .as_ref()
        .ok_or_else(|| state_error("coordinator_unavailable"))?;
    coordinator
        .lock()
        .map_err(|_| state_error("coordinator_unavailable"))?
        .retry_candidate(device_id)
        .map_err(|error| state_error(&error))?;
    snapshot(state)
}

#[tauri::command]
fn retry_candidate(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
) -> Result<AppSnapshot, AppError> {
    retry_candidate_inner(&state, &device_id)
}
```

Register `retry_candidate` in `tauri::generate_handler!`.

- [ ] **Step 5: Verify GREEN**

Run: `cargo test --manifest-path src-tauri/Cargo.toml retry_candidate -- --nocapture && cargo test --manifest-path src-tauri/Cargo.toml command_boundary -- --nocapture`

Expected: PASS; the exact Candidate restarts and the other worker is untouched.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/coordinator.rs src-tauri/src/lib.rs
git commit -m "feat: retry exact candidate devices"
```

### Task 3: Create Clone or Blank Device Profiles Without Overwrite

**Files:**
- Modify: `src-tauri/src/profile.rs`
- Modify: `src-tauri/src/workspace.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types.ts`

- [ ] **Step 1: Write failing Workspace tests for clone, blank, unique ID, and rollback**

Add to `src-tauri/src/workspace.rs` tests:

```rust
#[test]
fn creates_a_deep_cloned_profile_with_a_unique_stable_id() {
    let directory = TestDirectory::new();
    let mut workspace = workspace(&directory);
    let original = workspace.profiles["red-phone-v1"].clone();

    let created = workspace
        .create_profile(CreateDeviceProfileRequest::Clone {
            name: "Red Phone".into(),
            source_profile_id: "red-phone-v1".into(),
        })
        .unwrap()
        .clone();

    assert_eq!(created.profile.id, "red-phone");
    assert_eq!(created.profile.name, "Red Phone");
    assert_eq!(created.profile.groups, original.profile.groups);
    assert_eq!(created.actions, original.actions);
    assert_eq!(created.hardware_profiles, original.hardware_profiles);
    assert_eq!(workspace.settings.editor_profile.as_deref(), Some("red-phone"));
    assert_eq!(workspace.profiles["red-phone-v1"], original);

    let second = workspace
        .create_profile(CreateDeviceProfileRequest::Clone {
            name: "Red Phone".into(),
            source_profile_id: "red-phone-v1".into(),
        })
        .unwrap();
    assert_eq!(second.profile.id, "red-phone-2");
}

#[test]
fn creates_a_valid_blank_profile_for_the_exact_board() {
    let directory = TestDirectory::new();
    let mut workspace = workspace(&directory);

    let created = workspace
        .create_profile(CreateDeviceProfileRequest::Blank {
            name: "新键盘".into(),
            board_profile_id: crate::hardware::VCCGND_YD_RP2040_BOARD_ID.into(),
        })
        .unwrap();

    created.validate().unwrap();
    assert_eq!(created.profile.id, "vccgnd-yd-rp2040");
    assert_eq!(created.profile.name, "新键盘");
    assert!(created.profile.groups.is_empty());
    assert!(created.actions.is_empty());
    assert_eq!(created.hardware_profiles.len(), 1);
    assert_eq!(created.hardware_profiles[0].id, "hardware");
    assert_eq!(created.hardware_profiles[0].board_profile_id, crate::hardware::VCCGND_YD_RP2040_BOARD_ID);
    assert!(created.hardware_profiles[0].inputs.is_empty());
}

#[test]
fn profile_creation_rejects_bad_sources_and_never_overwrites() {
    let directory = TestDirectory::new();
    let mut workspace = workspace(&directory);
    let before = workspace.profiles.clone();

    assert_eq!(
        workspace
            .create_profile(CreateDeviceProfileRequest::Clone {
                name: "Copy".into(),
                source_profile_id: "missing".into(),
            })
            .unwrap_err()
            .code,
        "unknown_profile"
    );
    assert_eq!(
        workspace
            .create_profile(CreateDeviceProfileRequest::Blank {
                name: " ".into(),
                board_profile_id: crate::hardware::VCCGND_YD_RP2040_BOARD_ID.into(),
            })
            .unwrap_err()
            .code,
        "invalid_profile_name"
    );
    assert_eq!(workspace.profiles, before);
}
```

- [ ] **Step 2: Run the test to verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml creates_a_deep_cloned_profile -- --nocapture && cargo test --manifest-path src-tauri/Cargo.toml creates_a_valid_blank_profile -- --nocapture && cargo test --manifest-path src-tauri/Cargo.toml profile_creation_rejects -- --nocapture`

Expected: FAIL because `CreateDeviceProfileRequest` and `Workspace::create_profile` do not exist.

- [ ] **Step 3: Add the serialized request and constructors**

In `src-tauri/src/profile.rs`, import `ModelLayout` as already done and add:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreateDeviceProfileRequest {
    Clone {
        name: String,
        source_profile_id: String,
    },
    Blank {
        name: String,
        board_profile_id: String,
    },
}

pub fn blank_device_profile(
    id: String,
    name: String,
    board_profile_id: String,
) -> DeviceProfile {
    DeviceProfile {
        schema_version: PROFILE_SCHEMA_VERSION,
        profile: ModelLayout { id, name, groups: Vec::new() },
        hardware_profiles: vec![HardwareProfile {
            id: "hardware".into(),
            name: "Default hardware".into(),
            board_profile_id,
            debounce_ms: default_debounce_ms(),
            inputs: Vec::new(),
        }],
        actions: BTreeMap::new(),
    }
}
```

- [ ] **Step 4: Implement create-only persistence and deterministic IDs**

Import `board_by_id`, `blank_device_profile`, and `CreateDeviceProfileRequest` in `workspace.rs`. Add these helpers outside `impl Workspace`:

```rust
fn ascii_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push((byte as char).to_ascii_lowercase());
            separator = false;
        } else if !slug.is_empty() {
            separator = true;
        }
    }
    slug
}

fn next_profile_id(
    profiles: &BTreeMap<String, DeviceProfile>,
    name: &str,
    fallback: &str,
) -> String {
    let base = [ascii_slug(name), ascii_slug(fallback), "profile".into()]
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap();
    if !profiles.contains_key(&base) {
        return base;
    }
    (2..)
        .map(|suffix| format!("{base}-{suffix}"))
        .find(|candidate| !profiles.contains_key(candidate))
        .expect("profile ID suffix exhausted")
}
```

Add this Workspace method. It writes the new profile and updated Editor Profile as one recoverable operation; an existing ID is never passed to `write_yaml`:

```rust
pub fn create_profile(
    &mut self,
    request: CreateDeviceProfileRequest,
) -> Result<&DeviceProfile, AppError> {
    let (name, fallback, mut profile) = match request {
        CreateDeviceProfileRequest::Clone { name, source_profile_id } => {
            let source = self.profiles.get(&source_profile_id).ok_or_else(|| {
                AppError::new("unknown_profile").with_param("profile", source_profile_id)
            })?;
            (name, source.profile.id.clone(), source.clone())
        }
        CreateDeviceProfileRequest::Blank { name, board_profile_id } => {
            let board = board_by_id(&board_profile_id).ok_or_else(|| {
                AppError::new("unknown_board_profile").with_param("board_profile", &board_profile_id)
            })?;
            let profile = blank_device_profile(String::new(), name.clone(), board_profile_id);
            (name, board.id.to_owned(), profile)
        }
    };
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(AppError::new("invalid_profile_name"));
    }
    let id = next_profile_id(&self.profiles, &name, &fallback);
    profile.profile.id = id.clone();
    profile.profile.name = name;
    profile.validate()?;

    let path = self.profile_directory().join(format!("{id}.yaml"));
    if path.exists() || self.profiles.contains_key(&id) {
        return Err(AppError::new("profile_already_exists").with_param("profile", id));
    }
    write_yaml(&path, &profile)?;
    let mut settings = self.settings.clone();
    settings.editor_profile = Some(id.clone());
    if let Err(error) = self.persist_settings(&settings) {
        let _ = fs::remove_file(path);
        return Err(error);
    }
    self.settings = settings;
    self.profiles.insert(id.clone(), profile);
    Ok(&self.profiles[&id])
}
```

- [ ] **Step 5: Add frontend request types and the Tauri command**

Add to `src/types.ts`:

```typescript
export type CreateDeviceProfileRequest =
  | { kind: "clone"; name: string; source_profile_id: string }
  | { kind: "blank"; name: string; board_profile_id: string };
```

In `src-tauri/src/lib.rs`, import the request, add the inner function and command, then register it:

```rust
fn create_device_profile_inner(
    state: &AppState,
    request: CreateDeviceProfileRequest,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, _| {
        workspace.create_profile(request).map(|_| ())
    })
}

#[tauri::command]
fn create_device_profile(
    state: tauri::State<'_, AppState>,
    request: CreateDeviceProfileRequest,
) -> Result<AppSnapshot, AppError> {
    create_device_profile_inner(&state, request)
}
```

- [ ] **Step 6: Add a command-boundary snapshot test and verify GREEN**

Add this test to `src-tauri/src/lib.rs`:

```rust
#[test]
fn create_device_profile_command_returns_new_editor_snapshot_without_assignment() {
    let directory = TestDirectory::new();
    let state = product_state(&directory.0, vec![product_profile()]);
    let original = state.workspace.read().unwrap().profiles["red-phone-v1"].clone();

    let snapshot = create_device_profile_inner(
        &state,
        CreateDeviceProfileRequest::Clone {
            name: "Operator Copy".into(),
            source_profile_id: "red-phone-v1".into(),
        },
    ).unwrap();

    assert_eq!(snapshot.editor_profile.as_deref(), Some("operator-copy"));
    assert_eq!(snapshot.device_profiles.iter().filter(|profile| profile.profile.id == "operator-copy").count(), 1);
    assert_eq!(state.workspace.read().unwrap().profiles["red-phone-v1"], original);
    assert!(state.workspace.read().unwrap().settings.devices.is_empty());
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml workspace::tests -- --nocapture && cargo test --manifest-path src-tauri/Cargo.toml create_device_profile -- --nocapture && npm run build`

Expected: PASS; blank/clone profiles validate, persist, become the Editor Profile, and cannot overwrite existing profiles.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/profile.rs src-tauri/src/workspace.rs src-tauri/src/lib.rs src/types.ts
git commit -m "feat: create blank and cloned device profiles"
```

### Task 4: Complete Device Setup Atomically

**Files:**
- Modify: `src-tauri/src/workspace.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing Workspace transaction tests**

Add:

```rust
#[test]
fn complete_device_setup_persists_name_and_assignment_together() {
    let directory = TestDirectory::new();
    let mut workspace = workspace(&directory);
    let id = DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "SETUP-A").unwrap();
    workspace.enroll_device(id.clone()).unwrap();
    let assignment = RuntimeAssignment {
        device_profile_id: "red-phone-v1".into(),
        hardware_profile_id: "esp-primary".into(),
    };

    workspace.complete_device_setup(&id, "Front desk".into(), assignment.clone()).unwrap();

    let record = &workspace.settings.devices[&id];
    assert_eq!(record.name, "Front desk");
    assert_eq!(record.runtime_assignment, Some(assignment));
    let reloaded = Workspace::load_existing(&directory.0).unwrap();
    assert_eq!(reloaded.settings.devices[&id], *record);
}

#[test]
fn complete_device_setup_rolls_back_both_fields_when_assignment_is_invalid() {
    let directory = TestDirectory::new();
    let mut workspace = workspace(&directory);
    let id = DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "SETUP-B").unwrap();
    workspace.enroll_device(id.clone()).unwrap();
    let before = workspace.settings.clone();
    let disk_before = fs::read(directory.path("data/settings.yaml")).unwrap();

    let error = workspace
        .complete_device_setup(
            &id,
            "Partially written".into(),
            RuntimeAssignment {
                device_profile_id: "red-phone-v1".into(),
                hardware_profile_id: "missing".into(),
            },
        )
        .unwrap_err();

    assert_eq!(error.code, "unknown_hardware_profile");
    assert_eq!(workspace.settings, before);
    assert_eq!(fs::read(directory.path("data/settings.yaml")).unwrap(), disk_before);
}

#[test]
fn complete_device_setup_allows_multiple_devices_to_share_one_profile() {
    let directory = TestDirectory::new();
    let mut workspace = workspace(&directory);
    let a = DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "SHARED-A").unwrap();
    let b = DeviceId::new(crate::hardware::LUATOS_ESP32S3_AIO_BOARD_ID, "SHARED-B").unwrap();
    workspace.enroll_device(a.clone()).unwrap();
    workspace.enroll_device(b.clone()).unwrap();
    let assignment = RuntimeAssignment {
        device_profile_id: "red-phone-v1".into(),
        hardware_profile_id: "esp-primary".into(),
    };

    workspace.complete_device_setup(&a, "Shared A".into(), assignment.clone()).unwrap();
    workspace.complete_device_setup(&b, "Shared B".into(), assignment.clone()).unwrap();

    assert_eq!(workspace.settings.devices[&a].runtime_assignment, Some(assignment.clone()));
    assert_eq!(workspace.settings.devices[&b].runtime_assignment, Some(assignment));
}
```

- [ ] **Step 2: Run the tests to verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml complete_device_setup -- --nocapture`

Expected: FAIL because `Workspace::complete_device_setup` does not exist.

- [ ] **Step 3: Extract assignment validation and persist once**

Add a non-mutating validator and use it from both existing `set_assignment` and the new method:

```rust
fn validate_assignment(
    &self,
    id: &DeviceId,
    value: &RuntimeAssignment,
) -> Result<(), AppError> {
    let device = self.device(id)?;
    let profile = self.profiles.get(&value.device_profile_id).ok_or_else(|| {
        AppError::new("unknown_profile").with_param("profile", &value.device_profile_id)
    })?;
    let hardware = profile.hardware_profile(&value.hardware_profile_id).ok_or_else(|| {
        AppError::new("unknown_hardware_profile")
            .with_param("hardware_profile", &value.hardware_profile_id)
    })?;
    if hardware.board_profile_id != device.board_profile_id {
        return Err(AppError::new("assignment_board_mismatch")
            .with_param("device_board_profile", &device.board_profile_id)
            .with_param("hardware_board_profile", &hardware.board_profile_id));
    }
    Ok(())
}

pub fn complete_device_setup(
    &mut self,
    id: &DeviceId,
    name: String,
    assignment: RuntimeAssignment,
) -> Result<(), AppError> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(AppError::new("invalid_device_name"));
    }
    self.validate_assignment(id, &assignment)?;
    let mut settings = self.settings.clone();
    let record = settings.devices.get_mut(id).expect("device was validated");
    record.name = name;
    record.runtime_assignment = Some(assignment);
    self.persist_settings(&settings)?;
    self.settings = settings;
    Ok(())
}
```

Update `set_assignment` to call `self.validate_assignment(id, &value)?` before `update_device`.

- [ ] **Step 4: Write failing command-boundary eligibility tests**

Add this table test in `src-tauri/src/lib.rs`:

```rust
#[test]
fn setup_eligibility_requires_online_valid_runtime_device() {
    assert!(validate_setup_eligibility(
        coordinator::ConnectionDimension::Online,
        Some(coordinator::DeviceMode::Runtime),
        IdentityDimension::Valid,
    ).is_ok());
    assert_eq!(
        validate_setup_eligibility(
            coordinator::ConnectionDimension::Offline,
            None,
            IdentityDimension::Valid,
        ).unwrap_err().code,
        "device_offline"
    );
    assert_eq!(
        validate_setup_eligibility(
            coordinator::ConnectionDimension::Online,
            Some(coordinator::DeviceMode::Bootloader),
            IdentityDimension::Valid,
        ).unwrap_err().code,
        "device_not_runtime"
    );
    assert_eq!(
        validate_setup_eligibility(
            coordinator::ConnectionDimension::Online,
            Some(coordinator::DeviceMode::Runtime),
            IdentityDimension::DuplicateIdentity,
        ).unwrap_err().code,
        "invalid_device_identity"
    );
}
```

In the existing `command_boundary_mutations_and_metrics_target_exactly_one_device` test, after creating online IDs `a` and `b`, add:

```rust
let completed = complete_device_setup_inner(
    &state,
    &a,
    "Setup A".into(),
    assignment.clone(),
).unwrap();
assert_eq!(state.workspace.read().unwrap().settings.devices[&a].name, "Setup A");
assert_eq!(state.workspace.read().unwrap().settings.devices[&a].runtime_assignment, Some(assignment.clone()));
assert_ne!(state.workspace.read().unwrap().settings.devices[&b].name, "Setup A");
assert_eq!(state.workspace.read().unwrap().settings.devices[&b].runtime_assignment, None);
assert!(completed.devices.iter().any(|device| {
    device.device_id == a &&
    device.name == "Setup A" &&
    device.runtime_assignment.as_ref() == Some(&assignment)
}));
assert!(matches!(
    launcher.commands.lock().unwrap().get(&a).unwrap().last(),
    Some(WorkerCommand::Reconfigure { .. })
));
assert!(launcher.commands.lock().unwrap().get(&b).is_none());
```

In `invalid_identity_candidate_cannot_enter_device_commands`, add:

```rust
assert_eq!(
    complete_device_setup_inner(
        &state,
        &unregistered,
        "Candidate".into(),
        workspace::RuntimeAssignment {
            device_profile_id: "red-phone-v1".into(),
            hardware_profile_id: "esp-primary".into(),
        },
    ).unwrap_err().code,
    "unknown_device"
);
```

- [ ] **Step 5: Implement and register the command**

Add the pure eligibility validator and command boundary:

```rust
fn validate_setup_eligibility(
    connection: coordinator::ConnectionDimension,
    mode: Option<coordinator::DeviceMode>,
    identity: IdentityDimension,
) -> Result<(), AppError> {
    if connection != coordinator::ConnectionDimension::Online {
        return Err(state_error("device_offline"));
    }
    if mode != Some(coordinator::DeviceMode::Runtime) {
        return Err(state_error("device_not_runtime"));
    }
    if identity != IdentityDimension::Valid {
        return Err(state_error("invalid_device_identity"));
    }
    Ok(())
}

fn require_setup_device(
    coordinator: Option<&RuntimeCoordinator>,
    device_id: &hardware::DeviceId,
) -> Result<(), AppError> {
    let status = coordinator
        .and_then(|coordinator| coordinator.devices().into_iter().find(|device| device.device_id == *device_id))
        .ok_or_else(|| state_error("unknown_device"))?;
    validate_setup_eligibility(status.connection, status.mode, status.identity)
}

fn complete_device_setup_inner(
    state: &AppState,
    device_id: &hardware::DeviceId,
    name: String,
    assignment: RuntimeAssignment,
) -> Result<AppSnapshot, AppError> {
    mutate_workspace(state, move |workspace, coordinator| {
        require_setup_device(coordinator, device_id)?;
        workspace.complete_device_setup(device_id, name, assignment)
    })
}

#[tauri::command]
fn complete_device_setup(
    state: tauri::State<'_, AppState>,
    device_id: hardware::DeviceId,
    name: String,
    assignment: RuntimeAssignment,
) -> Result<AppSnapshot, AppError> {
    complete_device_setup_inner(&state, &device_id, name, assignment)
}
```

Register `complete_device_setup` in `tauri::generate_handler!`.

- [ ] **Step 6: Verify GREEN**

Run: `cargo test --manifest-path src-tauri/Cargo.toml complete_device_setup -- --nocapture && cargo test --manifest-path src-tauri/Cargo.toml command_boundary -- --nocapture`

Expected: PASS; validation happens before one settings write and rejected setup changes neither name nor assignment.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/workspace.rs src-tauri/src/lib.rs
git commit -m "feat: complete device setup atomically"
```

### Task 5: Build the Reusable Device Profile Creation Form

**Files:**
- Create: `src/CreateDeviceProfileForm.tsx`
- Create: `src/CreateDeviceProfileForm.test.tsx`
- Modify: `src/i18n.ts`

- [ ] **Step 1: Add the failing form tests**

Create `src/CreateDeviceProfileForm.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { CreateDeviceProfileForm } from "./CreateDeviceProfileForm";
import type { BoardProfileSummary, DeviceProfile } from "./types";

const boards: BoardProfileSummary[] = [
  { id: "rp", controllerFamilyId: "rp2040", displayName: "RP2040 Pad", runtimeUsb: "2e8a:102e", bootloaderUsb: "2e8a:0003", safePins: [0, 1] },
  { id: "esp", controllerFamilyId: "esp32s3", displayName: "ESP32 Pad", runtimeUsb: "303a:4002", bootloaderUsb: null, safePins: [1, 2] },
];

const profiles: DeviceProfile[] = [
  { schema_version: 2, profile: { id: "rp-source", name: "RP Source", groups: [] }, hardware_profiles: [{ id: "rp-hardware", name: "RP Hardware", board_profile_id: "rp", debounce_ms: 30, inputs: [] }], actions: {} },
  { schema_version: 2, profile: { id: "esp-source", name: "ESP Source", groups: [] }, hardware_profiles: [{ id: "esp-hardware", name: "ESP Hardware", board_profile_id: "esp", debounce_ms: 30, inputs: [] }], actions: {} },
];

test("submits a blank profile for the fixed device board", async () => {
  const user = userEvent.setup();
  const onCreate = vi.fn().mockResolvedValue(undefined);
  render(<CreateDeviceProfileForm language="zh-CN" boardProfiles={boards} deviceProfiles={profiles} fixedBoardProfileId="rp" onCreate={onCreate} onCancel={vi.fn()} />);

  await user.click(screen.getByRole("radio", { name: "空白配置" }));
  await user.type(screen.getByRole("textbox", { name: "配置名称" }), "桌面键盘");
  expect(screen.queryByRole("combobox", { name: "板型" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "创建配置" }));

  expect(onCreate).toHaveBeenCalledWith({
    kind: "blank",
    name: "桌面键盘",
    board_profile_id: "rp",
  });
});

test("filters clone sources by a fixed board and submits once while pending", async () => {
  const user = userEvent.setup();
  let resolveCreate!: () => void;
  const onCreate = vi.fn(() => new Promise<void>((resolve) => { resolveCreate = resolve; }));
  render(<CreateDeviceProfileForm language="zh-CN" boardProfiles={boards} deviceProfiles={profiles} fixedBoardProfileId="rp" onCreate={onCreate} onCancel={vi.fn()} />);

  expect(screen.getByRole("option", { name: "RP Source" })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "ESP Source" })).toBeNull();
  await user.type(screen.getByRole("textbox", { name: "配置名称" }), "RP 副本");
  const create = screen.getByRole("button", { name: "创建配置" });
  await user.click(create);
  expect(onCreate).toHaveBeenCalledWith({ kind: "clone", name: "RP 副本", source_profile_id: "rp-source" });
  expect(create).toBeDisabled();
  await user.click(create);
  expect(onCreate).toHaveBeenCalledTimes(1);
  resolveCreate();
});

test("requires a board for an independent blank profile", async () => {
  const user = userEvent.setup();
  const onCreate = vi.fn();
  render(<CreateDeviceProfileForm language="zh-CN" boardProfiles={boards} deviceProfiles={[]} onCreate={onCreate} onCancel={vi.fn()} />);

  await user.type(screen.getByRole("textbox", { name: "配置名称" }), "离线配置");
  await user.selectOptions(screen.getByRole("combobox", { name: "板型" }), "esp");
  await user.click(screen.getByRole("button", { name: "创建配置" }));

  expect(onCreate).toHaveBeenCalledWith({ kind: "blank", name: "离线配置", board_profile_id: "esp" });
});
```

- [ ] **Step 2: Run the tests to verify RED**

Run: `npm test -- src/CreateDeviceProfileForm.test.tsx`

Expected: FAIL because `CreateDeviceProfileForm.tsx` does not exist.

- [ ] **Step 3: Add the exact i18n keys used by the form**

Insert these exact values into the two language maps in `src/i18n.ts`:

```typescript
// zh-CN
"profile.create": "新建配置",
"profile.createAction": "创建配置",
"profile.name": "配置名称",
"profile.mode": "创建方式",
"profile.clone": "复制已有配置",
"profile.blank": "空白配置",
"profile.source": "来源配置",
"profile.board": "板型",
"profile.nameRequired": "请输入配置名称",
"profile.sourceRequired": "请选择来源配置",
"profile.boardRequired": "请选择板型",

// en-US
"profile.create": "New profile",
"profile.createAction": "Create profile",
"profile.name": "Profile name",
"profile.mode": "Creation method",
"profile.clone": "Clone existing profile",
"profile.blank": "Blank profile",
"profile.source": "Source profile",
"profile.board": "Board Profile",
"profile.nameRequired": "Enter a profile name",
"profile.sourceRequired": "Select a source profile",
"profile.boardRequired": "Select a Board Profile",
```

- [ ] **Step 4: Implement the controlled async form**

Create `src/CreateDeviceProfileForm.tsx`:

```tsx
import { useMemo, useState } from "react";
import { t } from "./i18n";
import type {
  BoardProfileSummary,
  CreateDeviceProfileRequest,
  DeviceProfile,
  Language,
} from "./types";

interface CreateDeviceProfileFormProps {
  language: Language;
  boardProfiles: BoardProfileSummary[];
  deviceProfiles: DeviceProfile[];
  fixedBoardProfileId?: string;
  onCreate(request: CreateDeviceProfileRequest): Promise<void>;
  onCancel(): void;
}

export function CreateDeviceProfileForm({
  language,
  boardProfiles,
  deviceProfiles,
  fixedBoardProfileId,
  onCreate,
  onCancel,
}: CreateDeviceProfileFormProps) {
  const cloneSources = useMemo(
    () => fixedBoardProfileId
      ? deviceProfiles.filter((profile) => profile.hardware_profiles.some(
          (hardware) => hardware.board_profile_id === fixedBoardProfileId,
        ))
      : deviceProfiles,
    [deviceProfiles, fixedBoardProfileId],
  );
  const [mode, setMode] = useState<"clone" | "blank">(
    cloneSources.length > 0 ? "clone" : "blank",
  );
  const [name, setName] = useState("");
  const [sourceProfileId, setSourceProfileId] = useState(cloneSources[0]?.profile.id ?? "");
  const [boardProfileId, setBoardProfileId] = useState(fixedBoardProfileId ?? "");
  const [pending, setPending] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  const error = name.trim().length === 0
    ? t(language, "profile.nameRequired")
    : mode === "clone" && !sourceProfileId
      ? t(language, "profile.sourceRequired")
      : mode === "blank" && !(fixedBoardProfileId ?? boardProfileId)
        ? t(language, "profile.boardRequired")
        : null;

  async function submit() {
    setSubmitted(true);
    if (pending || error) return;
    const request: CreateDeviceProfileRequest = mode === "clone"
      ? { kind: "clone", name: name.trim(), source_profile_id: sourceProfileId }
      : { kind: "blank", name: name.trim(), board_profile_id: fixedBoardProfileId ?? boardProfileId };
    setPending(true);
    try {
      await onCreate(request);
    } finally {
      setPending(false);
    }
  }

  return (
    <form className="profile-create-form" onSubmit={(event) => {
      event.preventDefault();
      void submit();
    }}>
      <fieldset className="profile-create-mode" disabled={pending}>
        <legend>{t(language, "profile.mode")}</legend>
        <label>
          <input type="radio" name="profile-mode" checked={mode === "clone"} disabled={cloneSources.length === 0} onChange={() => setMode("clone")} />
          {t(language, "profile.clone")}
        </label>
        <label>
          <input type="radio" name="profile-mode" checked={mode === "blank"} onChange={() => setMode("blank")} />
          {t(language, "profile.blank")}
        </label>
      </fieldset>
      <label className="profile-create-field">
        <span>{t(language, "profile.name")}</span>
        <input aria-label={t(language, "profile.name")} value={name} disabled={pending} onChange={(event) => setName(event.target.value)} />
      </label>
      {mode === "clone" && (
        <label className="profile-create-field">
          <span>{t(language, "profile.source")}</span>
          <select aria-label={t(language, "profile.source")} value={sourceProfileId} disabled={pending} onChange={(event) => setSourceProfileId(event.target.value)}>
            {cloneSources.map((profile) => <option key={profile.profile.id} value={profile.profile.id}>{profile.profile.name}</option>)}
          </select>
        </label>
      )}
      {mode === "blank" && !fixedBoardProfileId && (
        <label className="profile-create-field">
          <span>{t(language, "profile.board")}</span>
          <select aria-label={t(language, "profile.board")} value={boardProfileId} disabled={pending} onChange={(event) => setBoardProfileId(event.target.value)}>
            <option value="">-</option>
            {boardProfiles.map((board) => <option key={board.id} value={board.id}>{board.displayName}</option>)}
          </select>
        </label>
      )}
      {submitted && error && <p className="field-error" role="alert">{error}</p>}
      <div className="profile-create-actions">
        <button type="button" disabled={pending} onClick={onCancel}>{t(language, "common.cancel")}</button>
        <button className="primary-button" type="submit" disabled={pending}>{t(language, "profile.createAction")}</button>
      </div>
    </form>
  );
}
```

- [ ] **Step 5: Verify GREEN**

Run: `npm test -- src/CreateDeviceProfileForm.test.tsx && npm run build`

Expected: PASS; fixed-board creation cannot select the wrong board, compatible clone sources are filtered, and pending submission is single-shot.

- [ ] **Step 6: Commit**

```bash
git add src/CreateDeviceProfileForm.tsx src/CreateDeviceProfileForm.test.tsx src/i18n.ts
git commit -m "feat: add reusable profile creation form"
```

### Task 6: Build the Centralized Device Setup Wizard

**Files:**
- Create: `src/DeviceSetupWizard.tsx`
- Create: `src/DeviceSetupWizard.test.tsx`
- Modify: `src/i18n.ts`

- [ ] **Step 1: Write failing Candidate and transition tests**

Create `src/DeviceSetupWizard.test.tsx` with these imports and fixtures before the tests:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { DeviceSetupWizard, type DeviceSetupWizardProps } from "./DeviceSetupWizard";
import type { CandidateStatus, DeviceProfile, DeviceStatus } from "./types";

const boards = [
  { id: "rp", controllerFamilyId: "rp2040", displayName: "RP2040 Pad", runtimeUsb: "2e8a:102e", bootloaderUsb: "2e8a:0003", safePins: [0, 1] },
  { id: "esp", controllerFamilyId: "esp32s3", displayName: "ESP32 Pad", runtimeUsb: "303a:4002", bootloaderUsb: null, safePins: [1, 2] },
];

const profiles: DeviceProfile[] = [
  { schema_version: 2, profile: { id: "rp-profile", name: "RP Profile", groups: [] }, hardware_profiles: [{ id: "rp-hardware", name: "RP Hardware", board_profile_id: "rp", debounce_ms: 30, inputs: [] }], actions: {} },
  { schema_version: 2, profile: { id: "esp-profile", name: "ESP Profile", groups: [] }, hardware_profiles: [{ id: "esp-hardware", name: "ESP Hardware", board_profile_id: "esp", debounce_ms: 30, inputs: [] }], actions: {} },
];

function candidate(overrides: Partial<CandidateStatus> = {}): CandidateStatus {
  return {
    key: "runtime:/dev/cu.usbmodem1101",
    deviceId: "rp-device-id",
    mode: "runtime",
    identity: "validating",
    issue: "validating",
    rawSerial: "50031519384E811C",
    port: "/dev/cu.usbmodem1101",
    controllerFamilyId: "rp2040",
    boardProfileId: "rp",
    latestError: null,
    ...overrides,
  };
}

function unassignedDevice(overrides: Partial<DeviceStatus> = {}): DeviceStatus {
  return {
    deviceId: "rp-device-id",
    name: "RP2040 Pad · 4E811C",
    connection: "online",
    mode: "runtime",
    identity: "valid",
    assignment: "unassigned",
    runtime: "inactive",
    hardwareSerial: "50031519384E811C",
    port: "/dev/cu.usbmodem1101",
    controllerFamilyId: "rp2040",
    boardProfileId: "rp",
    firmwareBuildId: "hello-v3",
    capabilities: [0, 1],
    runtimeAssignment: null,
    latestError: null,
    learning: null,
    ...overrides,
  };
}

function renderWizard(overrides: Partial<DeviceSetupWizardProps> = {}) {
  const props: DeviceSetupWizardProps = {
    open: true,
    targetId: "rp-device-id",
    language: "zh-CN",
    devices: [],
    candidates: [],
    boardProfiles: boards,
    deviceProfiles: profiles,
    onTargetChange: vi.fn(),
    onRetryCandidate: vi.fn().mockResolvedValue(undefined),
    onCreateProfile: vi.fn().mockResolvedValue({
      deviceProfiles: profiles,
      editorProfile: "rp-profile",
      boardProfiles: boards,
      devices: [],
      candidates: [],
      language: "zh-CN",
      homeMetrics: null,
    }),
    onComplete: vi.fn().mockResolvedValue(undefined),
    onClose: vi.fn(),
    ...overrides,
  };
  return { ...render(<DeviceSetupWizard {...props} />), props };
}
```

Then add:

```tsx
test("explains firmware failure, hides cu port until expanded, and retries the exact ID", async () => {
  const user = userEvent.setup();
  const onRetryCandidate = vi.fn().mockResolvedValue(undefined);
  renderWizard({
    targetId: "rp-device-id",
    candidates: [candidate({ issue: "firmware_not_responding", latestError: "serial_handshake_timeout" })],
    onRetryCandidate,
  });

  expect(screen.getByText(/Kivo 固件未响应/)).toBeInTheDocument();
  expect(screen.getByText("/dev/cu.usbmodem1101")).not.toBeVisible();
  await user.click(screen.getByText("查看技术详情"));
  expect(screen.getByText("/dev/cu.usbmodem1101")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "重新检测" }));
  expect(onRetryCandidate).toHaveBeenCalledWith("rp-device-id");
});

test("keeps the wizard open and advances when a Candidate becomes the same Device ID", () => {
  const { rerender, props } = renderWizard({
    targetId: "rp-device-id",
    candidates: [candidate()],
  });
  expect(screen.getByText("正在确认设备")).toBeInTheDocument();

  rerender(<DeviceSetupWizard {...props} candidates={[]} devices={[unassignedDevice()]} />);

  expect(screen.getByRole("heading", { name: "选择键盘配置" })).toBeInTheDocument();
  expect(screen.getByText("RP2040 Pad")).toBeInTheDocument();
});

test("firmware failure can enter independent profile creation", async () => {
  const user = userEvent.setup();
  const onCreateProfile = vi.fn().mockResolvedValue({
    deviceProfiles: profiles,
    editorProfile: "rp-profile",
    boardProfiles: boards,
    devices: [],
    candidates: [candidate({ issue: "firmware_incompatible" })],
    language: "zh-CN",
    homeMetrics: null,
  });
  renderWizard({
    targetId: "rp-device-id",
    candidates: [candidate({ issue: "firmware_incompatible", latestError: "protocol_mismatch" })],
    onCreateProfile,
  });

  await user.click(screen.getByRole("button", { name: "先新建配置" }));
  await user.click(screen.getByRole("radio", { name: "空白配置" }));
  await user.type(screen.getByRole("textbox", { name: "配置名称" }), "RP 离线配置");
  await user.click(screen.getByRole("button", { name: "创建配置" }));
  expect(onCreateProfile).toHaveBeenCalledWith({ kind: "blank", name: "RP 离线配置", board_profile_id: "rp" });
});
```

- [ ] **Step 2: Write failing selection and completion tests**

Add:

```tsx
test("selects among multiple setup targets explicitly", async () => {
  const user = userEvent.setup();
  renderWizard({
    targetId: null,
    candidates: [candidate(), candidate({ key: "runtime:/dev/cu.second", deviceId: "second", rawSerial: "SECOND" })],
  });

  expect(screen.getByRole("heading", { name: "选择键盘" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /SECOND/ }));
  expect(screen.getByText("正在确认设备")).toBeInTheDocument();
});

test("lists only exact-board profiles and completes one exact Device", async () => {
  const user = userEvent.setup();
  const onComplete = vi.fn().mockResolvedValue(undefined);
  renderWizard({ targetId: "rp-device-id", devices: [unassignedDevice()], onComplete });

  expect(screen.getByRole("option", { name: "RP Profile" })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "ESP Profile" })).toBeNull();
  await user.selectOptions(screen.getByRole("combobox", { name: "键盘配置" }), "rp-profile");
  await user.selectOptions(screen.getByRole("combobox", { name: "硬件配置" }), "rp-hardware");
  await user.click(screen.getByRole("button", { name: "下一步" }));
  await user.clear(screen.getByRole("textbox", { name: "键盘名称" }));
  await user.type(screen.getByRole("textbox", { name: "键盘名称" }), "桌面 RP2040");
  await user.click(screen.getByRole("button", { name: "完成设置" }));

  expect(onComplete).toHaveBeenCalledTimes(1);
  expect(onComplete).toHaveBeenCalledWith("rp-device-id", "桌面 RP2040", {
    device_profile_id: "rp-profile",
    hardware_profile_id: "rp-hardware",
  });
});

test("preserves confirmation fields after setup failure", async () => {
  const user = userEvent.setup();
  const onComplete = vi.fn().mockRejectedValue(new Error("device_offline"));
  renderWizard({ targetId: "rp-device-id", devices: [unassignedDevice()], onComplete });
  await user.click(screen.getByRole("button", { name: "下一步" }));
  await user.clear(screen.getByRole("textbox", { name: "键盘名称" }));
  await user.type(screen.getByRole("textbox", { name: "键盘名称" }), "保留名称");
  await user.click(screen.getByRole("button", { name: "完成设置" }));

  expect(await screen.findByRole("alert")).toHaveTextContent("device_offline");
  expect(screen.getByRole("textbox", { name: "键盘名称" })).toHaveValue("保留名称");
});
```

- [ ] **Step 3: Run tests to verify RED**

Run: `npm test -- src/DeviceSetupWizard.test.tsx`

Expected: FAIL because the wizard does not exist.

- [ ] **Step 4: Implement the wizard as a prop-driven state machine**

Create `src/DeviceSetupWizard.tsx`:

```tsx
import { RefreshCw, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { CreateDeviceProfileForm } from "./CreateDeviceProfileForm";
import { t, type MessageKey } from "./i18n";
import type {
  AppSnapshot,
  BoardProfileSummary,
  CandidateIssue,
  CandidateStatus,
  CreateDeviceProfileRequest,
  DeviceProfile,
  DeviceStatus,
  Language,
  RuntimeAssignment,
} from "./types";

export interface DeviceSetupWizardProps {
  open: boolean;
  targetId: string | null;
  language: Language;
  devices: DeviceStatus[];
  candidates: CandidateStatus[];
  boardProfiles: BoardProfileSummary[];
  deviceProfiles: DeviceProfile[];
  onTargetChange(targetId: string): void;
  onRetryCandidate(deviceId: string): Promise<void>;
  onCreateProfile(request: CreateDeviceProfileRequest): Promise<AppSnapshot>;
  onComplete(deviceId: string, name: string, assignment: RuntimeAssignment): Promise<void>;
  onClose(): void;
}

const issueMessages: Record<CandidateIssue, { title: MessageKey; body: MessageKey }> = {
  validating: { title: "candidate.validating.title", body: "candidate.validating.body" },
  firmware_not_responding: { title: "candidate.firmware_not_responding.title", body: "candidate.firmware_not_responding.body" },
  firmware_incompatible: { title: "candidate.firmware_incompatible.title", body: "candidate.firmware_incompatible.body" },
  bootloader: { title: "candidate.bootloader.title", body: "candidate.bootloader.body" },
  port_unavailable: { title: "candidate.port_unavailable.title", body: "candidate.port_unavailable.body" },
  invalid_identity: { title: "candidate.invalid_identity.title", body: "candidate.invalid_identity.body" },
  duplicate_identity: { title: "candidate.duplicate_identity.title", body: "candidate.duplicate_identity.body" },
  unknown: { title: "candidate.unknown.title", body: "candidate.unknown.body" },
};

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error && "code" in error) return String(error.code);
  return String(error);
}

function candidateTargetId(candidate: CandidateStatus) {
  return candidate.deviceId ?? `candidate:${candidate.key}`;
}

function setupDevices(devices: DeviceStatus[]) {
  return devices.filter((device) =>
    device.connection === "online" &&
    device.mode === "runtime" &&
    device.identity === "valid" &&
    device.assignment === "unassigned",
  );
}

function compatibleProfiles(deviceProfiles: DeviceProfile[], boardProfileId: string) {
  return deviceProfiles.filter((profile) => profile.hardware_profiles.some(
    (hardware) => hardware.board_profile_id === boardProfileId,
  ));
}

export function DeviceSetupWizard({
  open,
  targetId,
  language,
  devices,
  candidates,
  boardProfiles,
  deviceProfiles,
  onTargetChange,
  onRetryCandidate,
  onCreateProfile,
  onComplete,
  onClose,
}: DeviceSetupWizardProps) {
  const [creatingProfile, setCreatingProfile] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [deviceProfileId, setDeviceProfileId] = useState("");
  const [hardwareProfileId, setHardwareProfileId] = useState("");
  const [name, setName] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const eligibleDevices = useMemo(() => setupDevices(devices), [devices]);
  const selectedCandidate = candidates.find((candidate) => candidateTargetId(candidate) === targetId) ?? null;
  const selectedDevice = eligibleDevices.find((device) => device.deviceId === targetId) ?? null;
  const targets = useMemo(() => {
    const values = new Map<string, string>();
    for (const candidate of candidates) {
      values.set(candidateTargetId(candidate), candidate.rawSerial ?? candidate.key);
    }
    for (const device of eligibleDevices) values.set(device.deviceId, device.name);
    return [...values].map(([id, label]) => ({ id, label }));
  }, [candidates, eligibleDevices]);
  const compatible = useMemo(
    () => selectedDevice ? compatibleProfiles(deviceProfiles, selectedDevice.boardProfileId) : [],
    [deviceProfiles, selectedDevice],
  );
  const selectedProfile = compatible.find((profile) => profile.profile.id === deviceProfileId) ?? null;
  const compatibleHardware = selectedProfile?.hardware_profiles.filter(
    (hardware) => hardware.board_profile_id === selectedDevice?.boardProfileId,
  ) ?? [];
  const boardName = (boardProfileId: string) =>
    boardProfiles.find((board) => board.id === boardProfileId)?.displayName ?? boardProfileId;

  useEffect(() => {
    if (!selectedDevice) return;
    const firstProfile = compatible[0] ?? null;
    const hardware = firstProfile?.hardware_profiles.filter(
      (item) => item.board_profile_id === selectedDevice.boardProfileId,
    ) ?? [];
    setName(selectedDevice.name);
    setDeviceProfileId(firstProfile?.profile.id ?? "");
    setHardwareProfileId(hardware.length === 1 ? hardware[0].id : "");
    setCreatingProfile(false);
    setConfirming(false);
    setError(null);
  }, [selectedDevice?.deviceId]);

  if (!open) return null;

  async function retryCandidate() {
    if (!selectedCandidate?.deviceId || pending) return;
    setPending(true);
    setError(null);
    try {
      await onRetryCandidate(selectedCandidate.deviceId);
    } catch (operationError) {
      setError(errorMessage(operationError));
    } finally {
      setPending(false);
    }
  }

  async function createProfile(request: CreateDeviceProfileRequest) {
    setPending(true);
    setError(null);
    try {
      const snapshot = await onCreateProfile(request);
      if (selectedDevice && snapshot.editorProfile) {
        const created = snapshot.deviceProfiles.find(
          (profile) => profile.profile.id === snapshot.editorProfile,
        );
        const hardware = created?.hardware_profiles.filter(
          (item) => item.board_profile_id === selectedDevice.boardProfileId,
        ) ?? [];
        setDeviceProfileId(created?.profile.id ?? "");
        setHardwareProfileId(hardware.length === 1 ? hardware[0].id : "");
      }
      setCreatingProfile(false);
    } catch (operationError) {
      setError(errorMessage(operationError));
    } finally {
      setPending(false);
    }
  }

  async function complete() {
    if (!selectedDevice || !deviceProfileId || !hardwareProfileId || !name.trim() || pending) return;
    setPending(true);
    setError(null);
    try {
      await onComplete(selectedDevice.deviceId, name.trim(), {
        device_profile_id: deviceProfileId,
        hardware_profile_id: hardwareProfileId,
      });
    } catch (operationError) {
      setError(errorMessage(operationError));
    } finally {
      setPending(false);
    }
  }

  const canRetry = selectedCandidate !== null &&
    selectedCandidate.deviceId !== null &&
    [
      "validating",
      "firmware_not_responding",
      "firmware_incompatible",
      "port_unavailable",
      "unknown",
    ].includes(selectedCandidate.issue);

  return (
    <div className="modal-backdrop" role="presentation">
      <section className="device-setup-dialog" role="dialog" aria-modal="true" aria-labelledby="device-setup-title">
        <header className="device-setup-header">
          <h2 id="device-setup-title">{t(language, "setup.title")}</h2>
          <button className="icon-button" type="button" aria-label={t(language, "common.close")} disabled={pending} onClick={onClose}><X size={17} /></button>
        </header>
        <div className="device-setup-body">
          {creatingProfile ? (
            <CreateDeviceProfileForm
              language={language}
              boardProfiles={boardProfiles}
              deviceProfiles={deviceProfiles}
              fixedBoardProfileId={selectedCandidate?.boardProfileId ?? selectedDevice?.boardProfileId}
              onCreate={createProfile}
              onCancel={() => setCreatingProfile(false)}
            />
          ) : targetId === null && targets.length > 0 ? (
            <section className="setup-targets">
              <h3>{t(language, "setup.selectTarget")}</h3>
              {targets.map((target) => <button type="button" key={target.id} onClick={() => onTargetChange(target.id)}>{target.label}</button>)}
            </section>
          ) : targetId === null ? (
            <section className="setup-empty">
              <h3>{t(language, "setup.waiting")}</h3>
              <button type="button" onClick={() => setCreatingProfile(true)}>{t(language, "profile.create")}</button>
            </section>
          ) : selectedCandidate ? (
            <section className="candidate-setup">
              <h3>{t(language, issueMessages[selectedCandidate.issue].title)}</h3>
              <p>{t(language, issueMessages[selectedCandidate.issue].body)}</p>
              <div className="candidate-actions">
                {canRetry && <button type="button" disabled={pending} onClick={() => void retryCandidate()}><RefreshCw size={16} />{t(language, "setup.retry")}</button>}
                <button type="button" disabled={pending} onClick={() => setCreatingProfile(true)}>{t(language, "setup.createFirst")}</button>
                <button type="button" disabled={pending} onClick={onClose}>{t(language, "setup.later")}</button>
              </div>
              <details className="device-technical-details">
                <summary>{t(language, "setup.technicalDetails")}</summary>
                <dl>
                  <dt>{t(language, "devices.serial")}</dt><dd>{selectedCandidate.rawSerial ?? "-"}</dd>
                  <dt>{t(language, "devices.id")}</dt><dd>{selectedCandidate.deviceId ?? "-"}</dd>
                  <dt>{t(language, "devices.board")}</dt><dd>{boardName(selectedCandidate.boardProfileId)}</dd>
                  <dt>{t(language, "devices.controller")}</dt><dd>{selectedCandidate.controllerFamilyId}</dd>
                  <dt>{t(language, "devices.mode")}</dt><dd>{selectedCandidate.mode}</dd>
                  <dt>{t(language, "setup.systemPort")}</dt><dd>{selectedCandidate.port ?? "-"}</dd>
                  <dt>{t(language, "devices.error")}</dt><dd>{selectedCandidate.latestError ?? "-"}</dd>
                </dl>
              </details>
            </section>
          ) : !selectedDevice ? (
            <section className="setup-empty"><h3>{t(language, "setup.disconnected")}</h3></section>
          ) : confirming ? (
            <section className="setup-confirmation">
              <h3>{t(language, "setup.confirmTitle")}</h3>
              <label><span>{t(language, "setup.keyboardName")}</span><input aria-label={t(language, "setup.keyboardName")} value={name} disabled={pending} onChange={(event) => setName(event.target.value)} /></label>
              <dl>
                <dt>{t(language, "devices.board")}</dt><dd>{boardName(selectedDevice.boardProfileId)}</dd>
                <dt>{t(language, "devices.serial")}</dt><dd>{selectedDevice.hardwareSerial.slice(-6)}</dd>
                <dt>{t(language, "setup.deviceProfile")}</dt><dd>{selectedProfile?.profile.name ?? deviceProfileId}</dd>
                <dt>{t(language, "setup.hardwareProfile")}</dt><dd>{compatibleHardware.find((hardware) => hardware.id === hardwareProfileId)?.name ?? hardwareProfileId}</dd>
              </dl>
              <div className="device-setup-actions">
                <button type="button" disabled={pending} onClick={() => setConfirming(false)}>{t(language, "setup.back")}</button>
                <button className="primary-button" type="button" disabled={pending || !name.trim()} onClick={() => void complete()}>{t(language, "setup.complete")}</button>
              </div>
            </section>
          ) : (
            <section className="setup-profile-choice">
              <h3>{t(language, "setup.selectProfile")}</h3>
              <p>{boardName(selectedDevice.boardProfileId)}</p>
              <label>
                <span>{t(language, "setup.deviceProfile")}</span>
                <select aria-label={t(language, "setup.deviceProfile")} value={deviceProfileId} disabled={pending} onChange={(event) => {
                  const nextId = event.target.value;
                  const next = compatible.find((profile) => profile.profile.id === nextId);
                  const hardware = next?.hardware_profiles.filter((item) => item.board_profile_id === selectedDevice.boardProfileId) ?? [];
                  setDeviceProfileId(nextId);
                  setHardwareProfileId(hardware.length === 1 ? hardware[0].id : "");
                }}>
                  {compatible.map((profile) => <option key={profile.profile.id} value={profile.profile.id}>{profile.profile.name}</option>)}
                </select>
              </label>
              <label>
                <span>{t(language, "setup.hardwareProfile")}</span>
                <select aria-label={t(language, "setup.hardwareProfile")} value={hardwareProfileId} disabled={pending} onChange={(event) => setHardwareProfileId(event.target.value)}>
                  {compatibleHardware.length !== 1 && <option value="">-</option>}
                  {compatibleHardware.map((hardware) => <option key={hardware.id} value={hardware.id}>{hardware.name}</option>)}
                </select>
              </label>
              <div className="device-setup-actions">
                <button type="button" disabled={pending} onClick={() => setCreatingProfile(true)}>{t(language, "profile.create")}</button>
                <button className="primary-button" type="button" disabled={!deviceProfileId || !hardwareProfileId || pending} onClick={() => setConfirming(true)}>{t(language, "setup.next")}</button>
              </div>
            </section>
          )}
          {error && <p className="field-error" role="alert">{error}</p>}
        </div>
      </section>
    </div>
  );
}
```

The effect intentionally keys on `selectedDevice?.deviceId`: when a Candidate becomes a Device with the same stable ID, it initializes the Device step without closing the modal. The component always reads live Candidate/Device props; a missing target renders the disconnected state and cannot submit.

- [ ] **Step 5: Add all wizard translations**

Add these exact entries to `zhCN`:

```typescript
"setup.title": "添加键盘",
"setup.selectTarget": "选择键盘",
"setup.waiting": "等待连接键盘",
"setup.addKeyboard": "添加键盘",
"setup.continue": "继续设置",
"setup.retry": "重新检测",
"setup.createFirst": "先新建配置",
"setup.later": "稍后处理",
"setup.technicalDetails": "查看技术详情",
"setup.systemPort": "系统通信端口",
"setup.selectProfile": "选择键盘配置",
"setup.deviceProfile": "键盘配置",
"setup.hardwareProfile": "硬件配置",
"setup.next": "下一步",
"setup.keyboardName": "键盘名称",
"setup.confirmTitle": "确认键盘设置",
"setup.back": "返回",
"setup.complete": "完成设置",
"setup.disconnected": "键盘已断开；重新连接同一设备后可继续。",
"candidate.validating.title": "正在确认设备",
"candidate.validating.body": "Kivo 正在验证键盘身份和固件协议。",
"candidate.firmware_not_responding.title": "Kivo 固件未响应",
"candidate.firmware_not_responding.body": "Kivo 固件未响应。设备可能尚未刷入匹配固件，或固件协议版本不兼容。处理固件后保持 USB 连接，Kivo 会自动重新检测。",
"candidate.firmware_incompatible.title": "固件与 Kivo 不兼容",
"candidate.firmware_incompatible.body": "设备已响应，但协议版本、控制器、板型或能力与当前 Kivo 不匹配。请在 Kivo 外部处理固件后重新检测。",
"candidate.bootloader.title": "设备处于引导模式",
"candidate.bootloader.body": "引导模式不能作为键盘使用。请在 Kivo 外部完成固件处理，然后重新连接设备。",
"candidate.port_unavailable.title": "系统通信端口不可用",
"candidate.port_unavailable.body": "通信端口无法打开或正被其他程序占用。关闭占用程序后重新检测。",
"candidate.invalid_identity.title": "设备身份无效",
"candidate.invalid_identity.body": "设备没有可用的硬件序列号，不能按通信端口绑定。请重新连接或检查设备固件。",
"candidate.duplicate_identity.title": "设备身份冲突",
"candidate.duplicate_identity.body": "多个设备声明了相同身份。请断开重复设备，只保留要设置的键盘。",
"candidate.unknown.title": "无法确认设备",
"candidate.unknown.body": "验证设备时发生未知问题。可查看技术详情并重新检测。",
```

Add these exact entries to `enUS`:

```typescript
"setup.title": "Add keyboard",
"setup.selectTarget": "Select keyboard",
"setup.waiting": "Waiting for a keyboard",
"setup.addKeyboard": "Add keyboard",
"setup.continue": "Continue setup",
"setup.retry": "Check again",
"setup.createFirst": "Create a profile first",
"setup.later": "Handle later",
"setup.technicalDetails": "View technical details",
"setup.systemPort": "System communication port",
"setup.selectProfile": "Select keyboard profile",
"setup.deviceProfile": "Keyboard profile",
"setup.hardwareProfile": "Hardware profile",
"setup.next": "Next",
"setup.keyboardName": "Keyboard name",
"setup.confirmTitle": "Confirm keyboard setup",
"setup.back": "Back",
"setup.complete": "Finish setup",
"setup.disconnected": "The keyboard was disconnected. Reconnect the same device to continue.",
"candidate.validating.title": "Confirming device",
"candidate.validating.body": "Kivo is validating the keyboard identity and firmware protocol.",
"candidate.firmware_not_responding.title": "Kivo firmware is not responding",
"candidate.firmware_not_responding.body": "The device may not have matching Kivo firmware, or its protocol version may be incompatible. Repair firmware outside Kivo and keep USB connected so Kivo can detect it again.",
"candidate.firmware_incompatible.title": "Firmware is incompatible with Kivo",
"candidate.firmware_incompatible.body": "The device responded, but its protocol, controller, Board Profile, or capabilities do not match. Repair firmware outside Kivo, then check again.",
"candidate.bootloader.title": "Device is in bootloader mode",
"candidate.bootloader.body": "Bootloader mode cannot be used as a keyboard. Handle firmware outside Kivo, then reconnect the device.",
"candidate.port_unavailable.title": "System communication port unavailable",
"candidate.port_unavailable.body": "The communication port cannot be opened or is busy in another application. Close the other application, then check again.",
"candidate.invalid_identity.title": "Invalid device identity",
"candidate.invalid_identity.body": "The device has no usable hardware serial and cannot be bound by communication port. Reconnect it or inspect its firmware.",
"candidate.duplicate_identity.title": "Device identity conflict",
"candidate.duplicate_identity.body": "Multiple devices claim the same identity. Disconnect duplicates and leave only the keyboard you want to set up.",
"candidate.unknown.title": "Cannot confirm device",
"candidate.unknown.body": "An unexpected validation problem occurred. View technical details and check again.",
```

- [ ] **Step 6: Verify GREEN**

Run: `npm test -- src/DeviceSetupWizard.test.tsx src/CreateDeviceProfileForm.test.tsx && npm run build`

Expected: PASS; Candidate-to-Device continuation uses stable Device ID, compatibility is exact, and failed completion keeps the draft.

- [ ] **Step 7: Commit**

```bash
git add src/DeviceSetupWizard.tsx src/DeviceSetupWizard.test.tsx src/i18n.ts
git commit -m "feat: add guided keyboard setup wizard"
```

### Task 7: Orchestrate Auto-Open, Snapshots, and Navigation in App

**Files:**
- Create: `src/deviceSetupSession.ts`
- Create: `src/deviceSetupSession.test.ts`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/DeviceManagement.tsx`
- Modify: `src/DeviceManagement.test.tsx`

- [ ] **Step 1: Write failing pure session-policy tests**

Create `src/deviceSetupSession.test.ts`:

```typescript
import { expect, test } from "vitest";
import { reconcileSetupSession, setupPresence } from "./deviceSetupSession";
import type { CandidateStatus, DeviceStatus } from "./types";

const candidate = {
  key: "runtime:/dev/cu.usbmodem1101",
  deviceId: "stable-rp",
  mode: "runtime",
  identity: "validating",
  issue: "validating",
  rawSerial: "SERIAL",
  port: "/dev/cu.usbmodem1101",
  controllerFamilyId: "rp2040",
  boardProfileId: "rp",
  latestError: null,
} satisfies CandidateStatus;

const device = {
  deviceId: "stable-rp",
  name: "RP",
  connection: "online",
  mode: "runtime",
  identity: "valid",
  assignment: "unassigned",
  runtime: "inactive",
  hardwareSerial: "SERIAL",
  port: "/dev/cu.usbmodem1101",
  controllerFamilyId: "rp2040",
  boardProfileId: "rp",
  firmwareBuildId: "build",
  capabilities: [],
  runtimeAssignment: null,
  latestError: null,
  learning: null,
} satisfies DeviceStatus;

test("keeps one insertion identity across Candidate to Device transition", () => {
  expect(setupPresence([], [candidate])).toEqual([{ id: "stable-rp", eligible: true }]);
  expect(setupPresence([device], [])).toEqual([{ id: "stable-rp", eligible: true }]);
});

test("suppresses a dismissed identity until it fully disappears", () => {
  const opened = reconcileSetupSession(new Set(), setupPresence([], [candidate]));
  expect(opened.autoTargetId).toBe("stable-rp");
  expect(opened.seen).toEqual(new Set(["stable-rp"]));

  const stillPresent = reconcileSetupSession(opened.seen, setupPresence([device], []));
  expect(stillPresent.autoTargetId).toBeNull();
  expect(stillPresent.seen).toEqual(new Set(["stable-rp"]));

  const removed = reconcileSetupSession(stillPresent.seen, []);
  expect(removed.seen.size).toBe(0);
  const reinserted = reconcileSetupSession(removed.seen, setupPresence([], [candidate]));
  expect(reinserted.autoTargetId).toBe("stable-rp");
});

test("retains assigned online identities for cycle suppression but does not auto-open them", () => {
  const assigned = { ...device, assignment: "valid", runtimeAssignment: { device_profile_id: "p", hardware_profile_id: "h" } } satisfies DeviceStatus;
  expect(setupPresence([assigned], [])).toEqual([{ id: "stable-rp", eligible: false }]);
  expect(reconcileSetupSession(new Set(), setupPresence([assigned], [])).autoTargetId).toBeNull();
});
```

- [ ] **Step 2: Run the policy tests to verify RED**

Run: `npm test -- src/deviceSetupSession.test.ts`

Expected: FAIL because the session helper does not exist.

- [ ] **Step 3: Implement stable insertion-cycle policy**

Create `src/deviceSetupSession.ts`:

```typescript
import type { CandidateStatus, DeviceStatus } from "./types";

export interface SetupPresence {
  id: string;
  eligible: boolean;
}

export function candidateSetupId(candidate: CandidateStatus) {
  return candidate.deviceId ?? `candidate:${candidate.key}`;
}

export function setupPresence(
  devices: DeviceStatus[],
  candidates: CandidateStatus[],
): SetupPresence[] {
  const presence = new Map<string, SetupPresence>();
  for (const candidate of candidates) {
    const id = candidateSetupId(candidate);
    presence.set(id, { id, eligible: true });
  }
  for (const device of devices) {
    if (device.connection !== "online") continue;
    presence.set(device.deviceId, {
      id: device.deviceId,
      eligible:
        device.mode === "runtime" &&
        device.identity === "valid" &&
        device.assignment === "unassigned",
    });
  }
  return [...presence.values()];
}

export function reconcileSetupSession(
  previousSeen: Set<string>,
  presence: SetupPresence[],
) {
  const present = new Set(presence.map(({ id }) => id));
  const seen = new Set([...previousSeen].filter((id) => present.has(id)));
  const autoTargetId = presence.find(({ id, eligible }) => eligible && !seen.has(id))?.id ?? null;
  if (autoTargetId) seen.add(autoTargetId);
  return { seen, autoTargetId };
}
```

- [ ] **Step 4: Write failing App integration tests**

Add these fixtures beside the existing `device()` helper in `src/App.test.tsx`:

```tsx
const rpBoard: AppSnapshot["boardProfiles"][number] = {
  id: "rp",
  controllerFamilyId: "rp2040",
  displayName: "RP2040 Pad",
  runtimeUsb: "2e8a:102e",
  bootloaderUsb: "2e8a:0003",
  safePins: [0, 1],
};

const rpProfile: DeviceProfile = {
  schema_version: 2,
  profile: { id: "rp-profile", name: "RP Profile", groups: [] },
  hardware_profiles: [{ id: "rp-hardware", name: "RP Hardware", board_profile_id: "rp", debounce_ms: 30, inputs: [] }],
  actions: {},
};

function rpCandidate(overrides: Partial<AppSnapshot["candidates"][number]> = {}): AppSnapshot["candidates"][number] {
  return {
    key: "runtime:/dev/cu.usbmodem1101",
    deviceId: "stable-rp",
    mode: "runtime",
    identity: "validating",
    issue: "validating",
    rawSerial: "50031519384E811C",
    port: "/dev/cu.usbmodem1101",
    controllerFamilyId: "rp2040",
    boardProfileId: "rp",
    latestError: null,
    ...overrides,
  };
}

function rpUnassignedDevice(overrides: Partial<DeviceStatus> = {}): DeviceStatus {
  return device({
    deviceId: "stable-rp",
    name: "RP2040 Pad · 4E811C",
    assignment: "unassigned",
    runtime: "inactive",
    hardwareSerial: "50031519384E811C",
    port: "/dev/cu.usbmodem1101",
    controllerFamilyId: "rp2040",
    boardProfileId: "rp",
    firmwareBuildId: "hello-v3",
    capabilities: [0, 1],
    runtimeAssignment: null,
    ...overrides,
  });
}
```

Inside the existing default `invoke` mock, insert these command branches before returning the cloned snapshot:

```tsx
if (command === "retry_candidate") {
  const deviceId = (args as { deviceId: string }).deviceId;
  currentSnapshot.candidates = currentSnapshot.candidates.map((candidate) =>
    candidate.deviceId === deviceId
      ? { ...candidate, issue: "validating", latestError: null }
      : candidate,
  );
}
if (command === "create_device_profile") {
  const request = (args as { request: CreateDeviceProfileRequest }).request;
  const id = request.name === "Offline RP" ? "offline-rp" : "created-profile";
  const created = request.kind === "clone"
    ? {
        ...structuredClone(currentSnapshot.deviceProfiles.find((profile) => profile.profile.id === request.source_profile_id)!),
        profile: {
          ...structuredClone(currentSnapshot.deviceProfiles.find((profile) => profile.profile.id === request.source_profile_id)!.profile),
          id,
          name: request.name,
        },
      }
    : {
        schema_version: 2 as const,
        profile: { id, name: request.name, groups: [] },
        hardware_profiles: [{ id: "hardware", name: "Default hardware", board_profile_id: request.board_profile_id, debounce_ms: 30, inputs: [] }],
        actions: {},
      };
  currentSnapshot.deviceProfiles.push(created);
  currentSnapshot.editorProfile = id;
}
if (command === "complete_device_setup") {
  const { deviceId, name, assignment } = args as {
    deviceId: string;
    name: string;
    assignment: RuntimeAssignment;
  };
  currentSnapshot.devices = currentSnapshot.devices.map((item) =>
    item.deviceId === deviceId
      ? { ...item, name, assignment: "valid", runtime: "configuring", runtimeAssignment: assignment }
      : item,
  );
}
```

Import `CreateDeviceProfileRequest` and `RuntimeAssignment` into the test's type import. Then add:

```tsx
test("auto-opens one new Candidate once and keeps Continue setup after dismissal", async () => {
  const user = userEvent.setup();
  currentSnapshot.devices = [];
  currentSnapshot.candidates = [rpCandidate()];
  currentSnapshot.boardProfiles = [rpBoard];
  render(<App />);

  expect(await screen.findByRole("dialog", { name: "添加键盘" })).toBeInTheDocument();
  await user.click(within(screen.getByRole("dialog", { name: "添加键盘" })).getByRole("button", { name: "稍后处理" }));
  expect(screen.queryByRole("dialog", { name: "添加键盘" })).toBeNull();
  await act(async () => emitRuntimeEvent(runtimeEvent({ code: "topology_active", input: null, pressed: null })));
  expect(screen.queryByRole("dialog", { name: "添加键盘" })).toBeNull();

  await user.click(screen.getByRole("button", { name: "设备管理" }));
  expect(screen.getByRole("button", { name: "继续设置" })).toBeInTheDocument();
});

test("does not reopen when Candidate becomes the same unassigned Device", async () => {
  currentSnapshot.devices = [];
  currentSnapshot.candidates = [rpCandidate()];
  currentSnapshot.boardProfiles = [rpBoard];
  currentSnapshot.deviceProfiles = [rpProfile];
  const { rerender } = render(<App />);
  const dialog = await screen.findByRole("dialog", { name: "添加键盘" });
  expect(dialog).toHaveTextContent("正在确认设备");

  currentSnapshot.candidates = [];
  currentSnapshot.devices = [rpUnassignedDevice()];
  await act(async () => emitRuntimeEvent(runtimeEvent({ deviceId: "stable-rp", code: "topology_active", input: null, pressed: null })));
  rerender(<App />);

  expect(screen.getByRole("dialog", { name: "添加键盘" })).toHaveTextContent("选择键盘配置");
  expect(screen.getAllByRole("dialog", { name: "添加键盘" })).toHaveLength(1);
});

test("configuration page creates a profile while no device is usable", async () => {
  const user = userEvent.setup();
  currentSnapshot.devices = [];
  currentSnapshot.candidates = [rpCandidate({ issue: "firmware_not_responding" })];
  currentSnapshot.boardProfiles = [rpBoard];
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "稍后处理" }));
  await user.click(screen.getByRole("button", { name: "配置文件" }));
  await user.click(screen.getByRole("button", { name: "新建配置" }));
  await user.click(screen.getByRole("radio", { name: "空白配置" }));
  await user.type(screen.getByRole("textbox", { name: "配置名称" }), "Offline RP");
  await user.selectOptions(screen.getByRole("combobox", { name: "板型" }), "rp");
  await user.click(screen.getByRole("button", { name: "创建配置" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_device_profile", {
    request: { kind: "blank", name: "Offline RP", board_profile_id: "rp" },
  }));
  expect(screen.getByLabelText("当前编辑配置")).toHaveValue("offline-rp");
  expect(currentSnapshot.devices).toHaveLength(0);
});

test("completes one exact Device and navigates to its Hardware Profile", async () => {
  const user = userEvent.setup();
  currentSnapshot.devices = [rpUnassignedDevice(), rpUnassignedDevice({ deviceId: "other-rp", hardwareSerial: "OTHER" })];
  currentSnapshot.candidates = [];
  currentSnapshot.boardProfiles = [rpBoard];
  currentSnapshot.deviceProfiles = [rpProfile];
  currentSnapshot.editorProfile = rpProfile.profile.id;
  render(<App />);
  const dialog = await screen.findByRole("dialog", { name: "添加键盘" });
  await user.selectOptions(within(dialog).getByRole("combobox", { name: "键盘配置" }), "rp-profile");
  await user.selectOptions(within(dialog).getByRole("combobox", { name: "硬件配置" }), "rp-hardware");
  await user.click(within(dialog).getByRole("button", { name: "下一步" }));
  await user.click(within(dialog).getByRole("button", { name: "完成设置" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("complete_device_setup", {
    deviceId: "stable-rp",
    name: expect.any(String),
    assignment: { device_profile_id: "rp-profile", hardware_profile_id: "rp-hardware" },
  }));
  expect(currentSnapshot.devices.find((device) => device.deviceId === "other-rp")?.runtimeAssignment).toBeNull();
  expect(await screen.findByRole("heading", { name: "硬件配置" })).toBeInTheDocument();
});
```

- [ ] **Step 5: Add App state, effects, and authoritative callbacks**

Import `Plus`, `CreateDeviceProfileForm`, `DeviceSetupWizard`, `reconcileSetupSession`, `setupPresence`, and the new request type. Add state/refs:

```tsx
const [setupOpen, setSetupOpen] = useState(false);
const [setupTargetId, setSetupTargetId] = useState<string | null>(null);
const [profileCreatorOpen, setProfileCreatorOpen] = useState(false);
const setupSeenRef = useRef<Set<string>>(new Set());
```

Add the auto-open effect after bootstrap state is declared:

```tsx
const currentSetupPresence = useMemo(
  () => setupPresence(devices, candidates),
  [devices, candidates],
);

useEffect(() => {
  if (!loaded) return;
  const next = reconcileSetupSession(setupSeenRef.current, currentSetupPresence);
  setupSeenRef.current = next.seen;
  if (!setupOpen && next.autoTargetId) {
    setSetupTargetId(next.autoTargetId);
    setSetupOpen(true);
  }
}, [currentSetupPresence, loaded, setupOpen]);
```

Use this manual opener so closing never clears cycle suppression:

```tsx
const openSetup = useCallback((targetId: string | null = null) => {
  if (targetId) setupSeenRef.current.add(targetId);
  setSetupTargetId(targetId);
  setSetupOpen(true);
}, []);
```

Add authoritative callbacks:

```tsx
const retrySetupCandidate = useCallback(async (deviceId: string) => {
  const snapshot = await invoke<AppSnapshot>("retry_candidate", { deviceId });
  if (mountedRef.current) applySnapshot(snapshot, true);
}, [applySnapshot]);

const createDeviceProfile = useCallback(async (request: CreateDeviceProfileRequest) => {
  await autosave.flush();
  const snapshot = await invoke<AppSnapshot>("create_device_profile", { request });
  if (mountedRef.current) applySnapshot(snapshot, true);
  return snapshot;
}, [applySnapshot, autosave]);

const completeDeviceSetup = async (
  deviceId: string,
  name: string,
  assignment: RuntimeAssignment,
) => {
  await autosave.flush();
  const completed = await invoke<AppSnapshot>("complete_device_setup", { deviceId, name, assignment });
  if (!mountedRef.current) return;
  applySnapshot(completed, true);
  if (completed.editorProfile !== assignment.device_profile_id) {
    await saveSettings(assignment.device_profile_id, language);
  }
  setHardwareEditorTarget({
    deviceId,
    deviceProfileId: assignment.device_profile_id,
    hardwareProfileId: assignment.hardware_profile_id,
  });
  setSetupOpen(false);
  setView("hardware");
};
```

Define `completeDeviceSetup` after the existing `saveSettings` function so it uses the current Editor Profile and language. Do not optimistically set `editorProfile`; the optional `saveSettings` call applies a second authoritative snapshot for navigation preference only.

- [ ] **Step 6: Render both creation entry points**

First add the integration props to `DeviceManagementProps` so this task remains buildable before Task 8 renders the new buttons:

```tsx
onOpenSetup(targetId: string | null): void;
onRetryCandidate(deviceId: string): void | Promise<void>;
```

Add `onOpenSetup: vi.fn()` and `onRetryCandidate: vi.fn()` to `renderManagement`'s default props. App may now pass both callbacks; Task 8 destructures and renders them.

In the Configuration Files heading, add a real command button:

```tsx
<div className="content-heading">
  <div><h2>{t(language, "nav.data")}</h2></div>
  <button className="primary-button" type="button" onClick={() => setProfileCreatorOpen(true)}>
    <Plus size={16} />{t(language, "profile.create")}
  </button>
</div>
```

Render this independent dialog at App root. Successful creation closes only this dialog and keeps `view === "data"`:

```tsx
{profileCreatorOpen && (
  <div className="modal-backdrop" role="presentation">
    <section className="device-setup-dialog profile-create-dialog" role="dialog" aria-modal="true" aria-labelledby="profile-create-title">
      <header className="device-setup-header">
        <h2 id="profile-create-title">{t(language, "profile.create")}</h2>
        <button className="icon-button" type="button" aria-label={t(language, "common.close")} onClick={() => setProfileCreatorOpen(false)}><X size={17} /></button>
      </header>
      <div className="device-setup-body">
        <CreateDeviceProfileForm
          language={language}
          boardProfiles={boardProfiles}
          deviceProfiles={deviceProfiles}
          onCreate={async (request) => {
            await createDeviceProfile(request);
            setProfileCreatorOpen(false);
          }}
          onCancel={() => setProfileCreatorOpen(false)}
        />
      </div>
    </section>
  </div>
)}
```

Render `DeviceSetupWizard` once at App root:

```tsx
<DeviceSetupWizard
  open={setupOpen}
  targetId={setupTargetId}
  language={language}
  devices={devices}
  candidates={candidates}
  boardProfiles={boardProfiles}
  deviceProfiles={deviceProfiles}
  onTargetChange={setSetupTargetId}
  onRetryCandidate={retrySetupCandidate}
  onCreateProfile={createDeviceProfile}
  onComplete={completeDeviceSetup}
  onClose={() => setSetupOpen(false)}
/>
```

Pass `onOpenSetup={openSetup}` and `onRetryCandidate={retrySetupCandidate}` into `DeviceManagement`; Task 8 adds their visible controls.

- [ ] **Step 7: Verify GREEN**

Run: `npm test -- src/deviceSetupSession.test.ts src/App.test.tsx && npm run build`

Expected: PASS; polling cannot reopen a dismissed insertion, Candidate-to-Device stays in one wizard, independent creation needs no Device, and completion targets one ID.

- [ ] **Step 8: Commit**

```bash
git add src/deviceSetupSession.ts src/deviceSetupSession.test.ts src/App.tsx src/App.test.tsx src/DeviceManagement.tsx src/DeviceManagement.test.tsx
git commit -m "feat: orchestrate guided device onboarding"
```

### Task 8: Replace the Device-Management Dead End with Friendly Actions

**Files:**
- Modify: `src/DeviceManagement.tsx`
- Modify: `src/DeviceManagement.test.tsx`
- Modify: `src/HomeDashboard.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/styles/views.css`
- Modify: `src/styles/app.css`

- [ ] **Step 1: Write failing Device Management tests**

Replace the current Candidate constant with this helper and use `candidates: [candidate()]` in `renderManagement`; Task 7 already added the callback mocks:

```tsx
function candidate(overrides: Partial<CandidateStatus> = {}): CandidateStatus {
  return {
    key: "candidate:esp32-pad:bad-serial:/dev/cu.bad",
    deviceId: null,
    mode: "bootloader",
    identity: "invalid_identity",
    issue: "invalid_identity",
    rawSerial: "BAD-001",
    port: "/dev/cu.bad",
    controllerFamilyId: "esp32s3",
    boardProfileId: "esp32-pad",
    latestError: "identity rejected",
    ...overrides,
  };
}
```

Then add:

```tsx
test("removes communication ports from rows and reveals them only in technical details", async () => {
  const user = userEvent.setup();
  renderManagement({ candidates: [candidate({ issue: "firmware_not_responding" })] });

  expect(screen.queryByText("端口", { selector: ".device-table-head span" })).toBeNull();
  expect(screen.getByText("/dev/cu.rp-a")).not.toBeVisible();
  await user.click(screen.getByRole("button", { name: /BAD-001/ }));
  expect(screen.getByText("/dev/cu.bad")).not.toBeVisible();
  await user.click(screen.getByText("查看技术详情"));
  expect(screen.getByText("/dev/cu.bad")).toBeInTheDocument();
  expect(screen.getByText("系统通信端口")).toBeInTheDocument();
});

test("shows friendly firmware recovery and retries only the selected Candidate", async () => {
  const user = userEvent.setup();
  const onRetryCandidate = vi.fn().mockResolvedValue(undefined);
  renderManagement({
    candidates: [candidate({ deviceId: "candidate-rp", issue: "firmware_not_responding", latestError: "serial_handshake_timeout" })],
    onRetryCandidate,
  });
  await user.click(screen.getByRole("button", { name: /BAD-001/ }));

  expect(screen.getByText(/Kivo 固件未响应/)).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "重新检测" }));
  expect(onRetryCandidate).toHaveBeenCalledWith("candidate-rp");
});

test("opens centralized setup from the page header and an unassigned Device", async () => {
  const user = userEvent.setup();
  const onOpenSetup = vi.fn();
  renderManagement({
    devices: [device({ assignment: "unassigned", runtimeAssignment: null, runtime: "inactive" })],
    candidates: [],
    onOpenSetup,
  });

  await user.click(screen.getByRole("button", { name: "添加键盘" }));
  expect(onOpenSetup).toHaveBeenCalledWith(null);
  await user.click(screen.getByRole("button", { name: "继续设置" }));
  expect(onOpenSetup).toHaveBeenCalledWith("rp-a");
});

test("identity conflicts never expose retry or assignment actions", async () => {
  const user = userEvent.setup();
  renderManagement({ candidates: [candidate({ deviceId: "conflict", issue: "duplicate_identity", identity: "duplicate_identity" })] });
  await user.click(screen.getByRole("button", { name: /BAD-001/ }));
  expect(screen.queryByRole("button", { name: "重新检测" })).toBeNull();
  expect(screen.queryByRole("button", { name: "保存运行分配" })).toBeNull();
  expect(screen.getByText(/多个设备声明了相同身份/)).toBeInTheDocument();
});

test("home connection status names the keyboard without exposing its system port", async () => {
  render(<App />);
  await screen.findByRole("heading", { name: "按键概览" });
  expect(screen.getByText("前台键盘")).toBeInTheDocument();
  expect(screen.queryByText("/dev/cu.test")).toBeNull();
});
```

Place the last test in `src/App.test.tsx`, where `App`, `baseSnapshot`, and the default invoke mock already exist. In the existing bootstrap-retry test, replace `expect(screen.getByText("/dev/cu.test")).toBeInTheDocument()` with `expect(screen.getByText("前台键盘")).toBeInTheDocument()`.

- [ ] **Step 2: Run the tests to verify RED**

Run: `npm test -- src/DeviceManagement.test.tsx src/App.test.tsx`

Expected: FAIL because the port column/home port are still visible and the setup/retry controls are not rendered.

- [ ] **Step 3: Destructure setup props and render structured issues**

Destructure the `onOpenSetup` and `onRetryCandidate` props added in Task 7. Import `Plus` and `RefreshCw` from `lucide-react`, `candidateSetupId` from `deviceSetupSession`, and `CandidateIssue`/`MessageKey` types. Add a `Plus` command to `.device-list-header`:

```tsx
<button className="primary-button device-list-command" type="button" onClick={() => onOpenSetup(null)}>
  <Plus size={16} />{t(language, "setup.addKeyboard")}
</button>
```

For online valid unassigned Devices, render a “继续设置” command before the advanced assignment controls:

```tsx
{selectedDevice.connection === "online" &&
  selectedDevice.mode === "runtime" &&
  selectedDevice.identity === "valid" &&
  selectedDevice.assignment === "unassigned" && (
    <button className="primary-button setup-command" type="button" onClick={() => onOpenSetup(selectedDevice.deviceId)}>
      {t(language, "setup.continue")}
    </button>
  )}
```

Define a typed key map at module scope and derive Candidate copy exclusively from `selectedCandidate.issue`:

```tsx
const candidateMessages: Record<CandidateIssue, { title: MessageKey; body: MessageKey }> = {
  validating: { title: "candidate.validating.title", body: "candidate.validating.body" },
  firmware_not_responding: { title: "candidate.firmware_not_responding.title", body: "candidate.firmware_not_responding.body" },
  firmware_incompatible: { title: "candidate.firmware_incompatible.title", body: "candidate.firmware_incompatible.body" },
  bootloader: { title: "candidate.bootloader.title", body: "candidate.bootloader.body" },
  port_unavailable: { title: "candidate.port_unavailable.title", body: "candidate.port_unavailable.body" },
  invalid_identity: { title: "candidate.invalid_identity.title", body: "candidate.invalid_identity.body" },
  duplicate_identity: { title: "candidate.duplicate_identity.title", body: "candidate.duplicate_identity.body" },
  unknown: { title: "candidate.unknown.title", body: "candidate.unknown.body" },
};

const messages = candidateMessages[selectedCandidate.issue];
const issueTitle = t(language, messages.title);
const issueBody = t(language, messages.body);
const canRetry = selectedCandidate.deviceId !== null && [
  "validating",
  "firmware_not_responding",
  "firmware_incompatible",
  "port_unavailable",
  "unknown",
].includes(selectedCandidate.issue);
```

Render title/body and these exact actions:

```tsx
<div className="candidate-actions">
  {canRetry && (
    <button type="button" onClick={() => selectedCandidate.deviceId && void onRetryCandidate(selectedCandidate.deviceId)}>
      <RefreshCw size={16} />{t(language, "setup.retry")}
    </button>
  )}
  <button className="primary-button" type="button" onClick={() => onOpenSetup(candidateSetupId(selectedCandidate))}>
    {t(language, "setup.continue")}
  </button>
</div>
```

Move every technical field into:

```tsx
<details className="device-technical-details">
  <summary>{t(language, "setup.technicalDetails")}</summary>
  <Detail label={t(language, "devices.serial")} value={selectedCandidate.rawSerial ?? "-"} />
  <Detail label={t(language, "devices.id")} value={selectedCandidate.deviceId ?? "-"} />
  <Detail label={t(language, "devices.board")} value={boards.get(selectedCandidate.boardProfileId)?.displayName ?? selectedCandidate.boardProfileId} />
  <Detail label={t(language, "devices.controller")} value={selectedCandidate.controllerFamilyId} />
  <Detail label={t(language, "devices.mode")} value={selectedCandidate.mode} />
  <Detail label={t(language, "setup.systemPort")} value={selectedCandidate.port ?? "-"} />
  <Detail label={t(language, "devices.error")} value={selectedCandidate.latestError ?? "-"} />
</details>
```

Put formal Device ID, port, controller, mode, firmware, capabilities, and raw error for registered Devices in the same collapsed technical-details pattern. Keep board, assignment, status, and user actions visible.

- [ ] **Step 4: Remove the port column without removing searchability**

Keep `candidate.port` and `device.port` in the search arrays, but remove the port header/cell from both Device and Candidate rows. Change CSS to four stable tracks:

```css
.device-table-head,
.device-row {
  display: grid;
  grid-template-columns: minmax(110px, 1.1fr) minmax(120px, 1fr) 92px minmax(140px, 1.2fr);
  gap: 10px;
  align-items: center;
}
```

In `src/HomeDashboard.tsx`, replace the port `<code>` with the friendly Device name:

```tsx
<div className={connectedDevice ? "home-device is-connected" : "home-device"}>
  <Activity size={15} />
  <span>{t(language, connectedDevice ? "connection.connected" : "connection.searching")}</span>
  {connectedDevice && <strong>{connectedDevice.name}</strong>}
</div>
```

Replace `.home-device code` with this rule:

```css
.home-device strong { max-width: 190px; overflow: hidden; text-overflow: ellipsis; color: var(--gray-11); white-space: nowrap; font-size: var(--text-11); }
```

- [ ] **Step 5: Add focused styles**

Use the exact Candidate translations introduced by Task 6. Add styles using existing tokens, with card radius no larger than 8px:

```css
.device-list-command { min-height: 32px; flex: 0 0 auto; }
.candidate-issue { display: grid; gap: var(--space-8); padding: var(--space-12) 0; }
.candidate-issue h3 { margin: 0; color: var(--gray-12); font-size: var(--text-14); }
.candidate-issue p { margin: 0; color: var(--gray-10); font-size: var(--text-12); line-height: 1.55; }
.candidate-actions { display: flex; flex-wrap: wrap; gap: var(--space-8); }
.candidate-actions button,
.setup-command { min-height: 34px; display: inline-flex; align-items: center; justify-content: center; gap: 6px; border: 1px solid var(--border-strong); border-radius: var(--radius-8); padding: 6px 12px; background: var(--bg-surface); }
.device-technical-details { margin-top: var(--space-12); border-top: 1px solid var(--border-default); }
.device-technical-details summary { padding: var(--space-12) 0; color: var(--gray-10); cursor: pointer; font-size: var(--text-12); font-weight: var(--weight-semibold); }
```

Add this complete wizard/form block in `views.css`:

```css
.device-setup-dialog {
  width: min(640px, calc(100vw - 32px));
  max-height: calc(100dvh - 32px);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-8);
  color: var(--gray-12);
  background: var(--bg-raised);
  box-shadow: var(--shadow-3);
}
.device-setup-header {
  min-height: 52px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-12);
  padding: 10px 12px 10px 16px;
  border-bottom: 1px solid var(--border-subtle);
}
.device-setup-header h2,
.device-setup-body h3 { margin: 0; font-size: var(--text-17); }
.device-setup-body { min-height: 0; overflow: auto; padding: var(--space-16); }
.candidate-setup,
.setup-profile-choice,
.setup-confirmation,
.setup-targets,
.setup-empty { display: grid; gap: var(--space-12); }
.candidate-setup > p,
.setup-profile-choice > p,
.setup-empty > p { margin: 0; color: var(--gray-10); font-size: var(--text-13); line-height: 1.55; }
.setup-targets > button { min-height: 42px; border: 1px solid var(--border-default); border-radius: var(--radius-8); padding: 8px 12px; color: var(--gray-11); background: var(--bg-surface); text-align: left; }
.setup-profile-choice > label,
.setup-confirmation > label,
.profile-create-field { min-width: 0; display: grid; gap: 6px; color: var(--gray-10); font-size: var(--text-12); }
.setup-profile-choice select,
.setup-confirmation input,
.profile-create-field input,
.profile-create-field select { min-width: 0; width: 100%; height: 36px; padding: 0 9px; }
.setup-profile-choice select:focus-visible,
.setup-confirmation input:focus-visible,
.profile-create-field input:focus-visible,
.profile-create-field select:focus-visible { outline: 2px solid var(--green-7); outline-offset: 1px; }
.device-setup-actions,
.profile-create-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: var(--space-8); padding-top: var(--space-8); }
.device-setup-actions button,
.profile-create-actions button { min-height: 36px; border: 1px solid var(--border-strong); border-radius: var(--radius-8); padding: 6px 12px; }
.profile-create-form { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-12); }
.profile-create-mode { grid-column: 1 / -1; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-8); margin: 0; border: 0; padding: 0; }
.profile-create-mode legend { margin-bottom: 6px; color: var(--gray-10); font-size: var(--text-12); }
.profile-create-mode label { min-height: 36px; display: flex; align-items: center; gap: 7px; border: 1px solid var(--border-default); border-radius: var(--radius-6); padding: 7px 9px; color: var(--gray-11); background: var(--bg-surface); }
.profile-create-mode input { margin: 0; }
.profile-create-form > .field-error,
.profile-create-actions { grid-column: 1 / -1; }
.device-technical-details dl,
.setup-confirmation dl { display: grid; grid-template-columns: 140px minmax(0, 1fr); gap: 1px var(--space-12); margin: 0; }
.device-technical-details dt,
.device-technical-details dd,
.setup-confirmation dt,
.setup-confirmation dd { margin: 0; border-bottom: 1px solid var(--gray-3); padding: 7px 0; overflow-wrap: anywhere; font-size: var(--text-12); }
.device-technical-details dt,
.setup-confirmation dt { color: var(--gray-9); }
.device-technical-details dd,
.setup-confirmation dd { color: var(--gray-12); }

@media (max-width: 680px) {
  .device-setup-dialog { width: calc(100vw - 20px); max-height: calc(100dvh - 20px); }
  .profile-create-form,
  .profile-create-mode { grid-template-columns: 1fr; }
  .profile-create-mode,
  .profile-create-form > .field-error,
  .profile-create-actions { grid-column: 1; }
  .device-technical-details dl,
  .setup-confirmation dl { grid-template-columns: 1fr; gap: 0; }
  .device-technical-details dt,
  .setup-confirmation dt { border-bottom: 0; padding-bottom: 0; }
}
```

In `app.css`, extend the existing shared command-button selector with `.candidate-actions button`, `.device-setup-actions button`, `.profile-create-actions button`, `.device-list-command`, and `.setup-command`; extend the matching hover selector with the same non-primary controls. Do not add gradients, decorative cards, or nested cards.

- [ ] **Step 6: Verify GREEN and regressions**

Run: `npm test -- src/DeviceManagement.test.tsx src/DeviceSetupWizard.test.tsx src/App.test.tsx && npm run build`

Expected: PASS; `/dev/cu.*` appears only after expanding technical details, friendly actions remain accessible, and advanced reassignment tests still pass.

- [ ] **Step 7: Commit**

```bash
git add src/DeviceManagement.tsx src/DeviceManagement.test.tsx src/HomeDashboard.tsx src/App.test.tsx src/styles/views.css src/styles/app.css
git commit -m "feat: guide pending device recovery"
```

### Task 9: Full Verification and Physical RP2040 Acceptance

**Files:**
- Modify only files required to fix failures found by the commands below.

- [ ] **Step 1: Run all automated tests**

Run:

```bash
npm test
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all React and Rust tests PASS, including existing autosave, learning, backup/restore, multi-device, and runtime protocol tests.

- [ ] **Step 2: Run compile and repository checks**

Run:

```bash
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git diff --check
```

Expected: every command exits 0 with no TypeScript, Rust formatting, Clippy, or whitespace failures.

- [ ] **Step 3: Start the app without adding firmware flashing**

Run: `npm run tauri dev`

Expected: Kivo opens and recognizes the connected RP2040 observation. Do not invoke or install any firmware tool during this task.

- [ ] **Step 4: Verify the current RP2040 failure path**

With serial `50031519384E811C` connected, verify:

1. One RP2040 keyboard target appears; `/dev/cu.usbmodem1101` is not a second device.
2. The setup wizard automatically opens only once for the insertion cycle.
3. If HELLO v3 does not validate, the UI reports the matching firmware/port/identity category and offers “重新检测”, “先新建配置”, and “稍后处理” where allowed.
4. `/dev/cu.usbmodem1101` is absent from the main list and appears only after “查看技术详情”, labelled “系统通信端口”.
5. “先新建配置” creates a durable clone or valid blank RP2040 profile; closing and reopening Kivo preserves it.
6. No screen offers firmware flashing, UF2 selection, or `picotool` execution.

Expected: all six checks pass. If the firmware is still invalid, record the structured category and raw technical error; this is an acceptable physical result for this iteration.

- [ ] **Step 5: Verify the success path only if compatible external firmware is already present**

Without modifying firmware in Kivo, verify that a HELLO v3-valid device with the same stable Device ID advances within the open wizard, lists only exact-board profiles, and completes one Device assignment. Confirm a same-board sibling remains unchanged and the app navigates to the selected Hardware Profile.

Expected: PASS when compatible firmware is externally available. If it is not available, report this step as `BLOCKED: compatible HELLO v3 firmware not present`; do not fail the implementation or expand scope into flashing.

- [ ] **Step 6: Inspect the final diff for scope and text leaks**

Run:

```bash
git diff --stat 7828d62
rg -n "picotool|platformio|\.uf2|刷写固件|Flash firmware" src src-tauri/src
rg -n "/dev/cu" src --glob '!*.test.*' --glob '!preview.ts'
git status --short
```

Expected: the firmware-tool search has no new product UI actions; `/dev/cu` is not hard-coded in production UI; only intended source/test/plan files are modified.

- [ ] **Step 7: Commit final verification fixes**

If verification required code changes, rerun Steps 1-2 and commit them:

```bash
git add src src-tauri/src
git commit -m "test: verify guided device onboarding"
```

If no files changed, do not create an empty commit.
