# Kivo Product Configuration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Kivo's staged global GPIO/action editor with Chinese-first, automatically saved, self-contained model configurations, ordered actions, product data management, and ESP32-S3 direct/contact-matrix support.

**Architecture:** Rust owns versioned YAML workspace transactions and resolves physical input signatures to logical buttons. The ESP32-S3 receives an atomic runtime topology, reports direct/contact events, and acknowledges every HID action step. React owns localized three-column editing and one serialized 400 ms auto-save queue.

**Tech Stack:** React 19, TypeScript 7, Vitest, Tauri 2, Rust 2024, serde/serde_yaml_ng, official Tauri dialog plugin, PlatformIO Arduino ESP32-S3, native Unity tests.

## Global Constraints

- Work directly on the current `feat/prod_config` branch; do not create a worktree.
- Prefix every shell command with `rtk`.
- Preserve the user's uncommitted `Makefile` change and never include it in feature commits.
- Default locale is `zh-CN`; `en-US` must cover every product string without adding an i18n framework.
- Implement only `direct` and `contact_matrix` input sources and only ESP32-S3 firmware.
- Keep imported future-controller model files editable but runtime-inactive.
- Use the existing atomic temporary-file-and-rename helper for individual file writes.
- Do not invent a `tel001` electrical mapping from `assets/tel.jpg`.
- Every production behavior change follows red-green-refactor.

---

### Task 1: Self-Contained Model And Workspace Storage

**Files:**
- Create: `src-tauri/src/workspace.rs`
- Modify: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/model.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: inline `#[cfg(test)]` modules in the same Rust files

**Interfaces:**
- Produces `AppError`, `Language`, `SettingsDocument`, `HardwareConfig`, `InputSource`, `ModelConfig`, `BackupDocument`, `Workspace`, `WorkspaceSnapshot`, `ImportPreview`, and `BackupPreview`.
- Produces `Workspace::{load, save_model, save_settings, import_model, export_model, delete_model, export_backup, restore_backup}`.
- Preserves `ModelLayout`, `ButtonGroup`, and `ButtonDefinition` as the layout representation.
- Changes `ButtonAction` consumers from one action to `BTreeMap<String, Vec<ButtonAction>>`.

- [ ] **Step 1: Write failing model-format and validation tests**

Add tests that construct the wished-for API:

```rust
#[test]
fn model_config_round_trips_unicode_and_action_order() {
    let config = model_config(vec![
        ButtonAction::Paste { text: "你好\n".into() },
        ButtonAction::Hotkey { keys: vec!["enter".into()] },
    ]);
    let yaml = serde_yaml_ng::to_string(&config).unwrap();
    let loaded: ModelConfig = serde_yaml_ng::from_str(&yaml).unwrap();
    assert_eq!(loaded, config);
}

#[test]
fn rejects_non_bipartite_contact_graph() {
    let mut config = model_config(Vec::new());
    config.hardware.inputs = vec![InputSource::ContactMatrix {
        id: "keys".into(),
        pins: vec![1, 2, 3],
        keys: BTreeMap::from([
            ("A".into(), [1, 2]),
            ("B".into(), [2, 3]),
            ("C".into(), [3, 1]),
        ]),
    }];
    assert!(config.validate().unwrap_err().code == "matrix_not_bipartite");
}
```

- [ ] **Step 2: Run Rust tests and verify RED**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml model_config`

Expected: compilation fails because `ModelConfig`, `HardwareConfig`, and `InputSource` do not exist.

- [ ] **Step 3: Implement the model document and pure validation**

Use these exact public shapes in `config.rs`:

```rust
pub const MODEL_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub params: BTreeMap<String, String>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ButtonAction {
    Paste { text: String },
    Hotkey { keys: Vec<String> },
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputSource {
    Direct { id: String, keys: BTreeMap<String, u8> },
    ContactMatrix { id: String, pins: Vec<u8>, keys: BTreeMap<String, [u8; 2]> },
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct HardwareConfig {
    pub controller: String,
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u16,
    #[serde(default)]
    pub inputs: Vec<InputSource>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ModelConfig {
    pub schema_version: u16,
    pub model: ModelLayout,
    pub hardware: HardwareConfig,
    #[serde(default)]
    pub actions: BTreeMap<String, Vec<ButtonAction>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy: Option<LegacyConfig>,
}
```

Validation must implement every rule in the approved spec, including ASCII IDs,
unique GPIO ownership, unique button binding, matrix pair uniqueness, bipartite
coloring, non-empty untrimmed paste, and existing hotkey validation.

- [ ] **Step 4: Write failing workspace transaction and migration tests**

```rust
#[test]
fn deleting_last_model_persists_an_empty_workspace() {
    let directory = TestDirectory::new();
    let mut workspace = Workspace::create(directory.path(), vec![model_config(Vec::new())]).unwrap();
    workspace.delete_model("red-phone-v1").unwrap();
    let reloaded = Workspace::load(directory.path()).unwrap();
    assert!(reloaded.models.is_empty());
    assert_eq!(reloaded.settings.active_model, None);
}

#[test]
fn legacy_global_action_is_copied_per_model() {
    let migrated = migrate_legacy(legacy_fixture()).unwrap();
    assert_eq!(migrated.models["red-phone-v1"].actions["DIGIT_2"].len(), 1);
    assert_eq!(migrated.models["other-model"].actions["DIGIT_2"].len(), 1);
}
```

- [ ] **Step 5: Run workspace tests and verify RED**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml workspace`

Expected: compilation fails because `Workspace` and `migrate_legacy` do not exist.

- [ ] **Step 6: Implement workspace persistence and operations**

`workspace.rs` owns `data/settings.yaml`, `data/models/*.yaml`, the 10 MiB read
limit, deterministic serialization, first-run production preset copy, staged
legacy migration, same-ID atomic import replacement, delete-to-empty, and full
backup directory swap with rollback. Use these command-facing signatures:

```rust
impl Workspace {
    pub fn load(config_dir: &Path, bundled_models: &Path, legacy: LegacyPaths<'_>) -> Result<Self, AppError>;
    pub fn snapshot(&self) -> WorkspaceSnapshot;
    pub fn save_model(&mut self, model: ModelConfig) -> Result<(), AppError>;
    pub fn save_settings(&mut self, settings: SettingsDocument) -> Result<(), AppError>;
    pub fn preview_model(&self, path: &Path) -> Result<ImportPreview, AppError>;
    pub fn import_model(&mut self, path: &Path) -> Result<(), AppError>;
    pub fn export_model(&self, id: &str, path: &Path) -> Result<(), AppError>;
    pub fn delete_model(&mut self, id: &str) -> Result<(), AppError>;
    pub fn preview_backup(&self, path: &Path) -> Result<BackupPreview, AppError>;
    pub fn export_backup(&self, path: &Path) -> Result<(), AppError>;
    pub fn restore_backup(&mut self, path: &Path) -> Result<(), AppError>;
}
```

- [ ] **Step 7: Run Task 1 tests and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml config workspace model`

Expected: all focused Rust tests pass.

Commit:

```bash
rtk git add src-tauri/src/config.rs src-tauri/src/model.rs src-tauri/src/workspace.rs src-tauri/src/lib.rs
rtk git commit -m "feat: store self-contained model configurations"
```

---

### Task 2: ESP32-S3 Runtime Topology And Learning Protocol

**Files:**
- Create: `lib/gpio_trigger/src/InputTopology.h`
- Create: `lib/gpio_trigger/src/InputTopology.cpp`
- Modify: `lib/gpio_trigger/src/GpioTriggerController.h`
- Modify: `lib/gpio_trigger/src/GpioTriggerController.cpp`
- Modify: `lib/gpio_trigger/src/TriggerProtocol.h`
- Modify: `lib/gpio_trigger/src/TriggerProtocol.cpp`
- Modify: `src/main.cpp`
- Modify: `test/test_gpio_trigger/test_main.cpp`

**Interfaces:**
- Produces `TopologyBuilder`, `RuntimeTopology`, `PhysicalInput`, and revisioned config commit results.
- Produces `HelperCommand` parsing for config, learning, ordered paste/hotkey, and skip messages.
- Produces direct/contact `STATE`, `HELLO`, `CONFIG_OK`, `CONFIG_ERROR`, `LEARN_*`, and `DONE` lines.

- [ ] **Step 1: Write failing native tests for atomic topology and contact events**

```cpp
void test_commits_complete_matrix_topology_atomically() {
  TopologyBuilder builder;
  TEST_ASSERT_TRUE(builder.begin(7, 30));
  TEST_ASSERT_TRUE(builder.addMatrix(7, 0, {1, 2}, {12, 13}));
  const auto topology = builder.commit(7);
  TEST_ASSERT_TRUE(topology.has_value());
  TEST_ASSERT_EQUAL_UINT32(7, topology->revision);
  TEST_ASSERT_EQUAL_UINT8(2, topology->matrices[0].rows.size());
}

void test_contact_edge_reports_unordered_pair_once_after_debounce() {
  GpioTriggerController controller;
  controller.configure(topology_fixture(), 0);
  TEST_ASSERT_FALSE(controller.updateContact(0, 1, 12, true, 10).has_value());
  const auto event = controller.updateContact(0, 12, 1, true, 40);
  TEST_ASSERT_TRUE(event.has_value());
  TEST_ASSERT_EQUAL_UINT8(1, event->input.pinA);
  TEST_ASSERT_EQUAL_UINT8(12, event->input.pinB);
}
```

- [ ] **Step 2: Run native tests and verify RED**

Run: `rtk uv run pio test -e native`

Expected: compilation fails because topology types and contact updates do not exist.

- [ ] **Step 3: Implement topology state and pure debounce/pending logic**

Use vectors only for small runtime config collections. Normalize contact pairs
to `min(pin), max(pin)`. A pending action stores `eventId`, `nextStep`, `total`,
and refreshed timeout. `acceptStep(event, step, total, execute, nowMs)` rejects
stale or out-of-order steps and clears only on skip or the final accepted step.

- [ ] **Step 4: Write failing parser tests for the complete serial grammar**

Cover exact parsing and serialization of:

```text
CONFIG_BEGIN 3 30
CONFIG_DIRECT 3 0 2 6 7
CONFIG_MATRIX 3 1 2 1 2 2 12 13
CONFIG_COMMIT 3
LEARN_BEGIN 4 4 1 2 12 13
LEARN_END 4
PASTE 9 1 2
HOTKEY 9 2 2 0 40
SKIP 9
```

Verify malformed counts, duplicate pins, mismatched revisions, invalid steps,
and overlong lines are rejected without mutating the active topology.

- [ ] **Step 5: Implement protocol and Arduino scanning**

`main.cpp` sends `HELLO 2 esp32s3 17 0 1 2 3 4 5 6 7 8 9 12 13 14 15 16 17 18`
after USB startup. It applies config only after commit, scans direct pins with
pull-ups, scans one matrix row low at a time with columns pulled up, reports
normalized contact pairs, suppresses ambiguous ghost additions, and restores
the committed topology after learning ends. Each executed action writes
`DONE <event_id> <step>` after releasing HID keys.

- [ ] **Step 6: Run Task 2 tests and commit**

Run: `rtk uv run pio test -e native`

Expected: every native Unity test passes.

Commit:

```bash
rtk git add lib/gpio_trigger/src/InputTopology.h lib/gpio_trigger/src/InputTopology.cpp lib/gpio_trigger/src/GpioTriggerController.h lib/gpio_trigger/src/GpioTriggerController.cpp lib/gpio_trigger/src/TriggerProtocol.h lib/gpio_trigger/src/TriggerProtocol.cpp src/main.cpp test/test_gpio_trigger/test_main.cpp
rtk git commit -m "feat: configure direct and matrix inputs at runtime"
```

---

### Task 3: Host Protocol, Ordered Actions, And Structured Runtime Events

**Files:**
- Modify: `src-tauri/src/protocol.rs`
- Modify: `src-tauri/src/device.rs`
- Modify: `src-tauri/src/config.rs`
- Test: inline Rust modules in those files

**Interfaces:**
- Produces `PhysicalInput::{Direct, Contact}`, `DeviceMessage`, `TopologyCommand`, `ActionSequence`, and structured `RuntimeEvent`.
- Consumes Task 1's active `ModelConfig` and Task 2's protocol version `2`.
- Resolves physical signatures with `ModelConfig::button_for(&PhysicalInput) -> Option<&str>`.

- [ ] **Step 1: Write failing protocol and sequence tests**

```rust
#[test]
fn parses_contact_state_and_done() {
    assert_eq!(parse_device("STATE 9 CONTACT 1 12 1 DOWN\n"), Some(DeviceMessage::State {
        event_id: 9,
        input: PhysicalInput::Contact { source: 1, pin_a: 1, pin_b: 12 },
        state: InputState::Down,
    }));
    assert_eq!(parse_device("DONE 9 2\n"), Some(DeviceMessage::Done { event_id: 9, step: 2 }));
}

#[test]
fn waits_for_done_before_copying_the_next_paste() {
    let mut sequence = ActionSequence::new(9, 6, vec![paste("first"), paste("second")]);
    assert_eq!(sequence.next_step().unwrap().step, 1);
    assert!(sequence.next_step().is_none());
    sequence.acknowledge(9, 1).unwrap();
    assert_eq!(sequence.next_step().unwrap().step, 2);
}
```

- [ ] **Step 2: Run focused Rust tests and verify RED**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml protocol`

Expected: compilation fails because the new message and sequence types do not exist.

- [ ] **Step 3: Implement parsing, topology encoding, and sequence state**

Use a line parser with exact token counts. `topology_commands(model, revision)`
derives matrix partitions from validated contact pairs and emits begin, source,
and commit lines. `ActionSequence` copies clipboard text only when emitting that
step, emits one line, blocks until matching `DONE`, and stops on the first error.

- [ ] **Step 4: Refactor the device worker into one serial state machine**

The worker must continue parsing `STATE` while awaiting `DONE`, queue button-down
events, execute one sequence at a time, emit structured `{ code, params, detail }`
activity, configure topology after `HELLO`, and reject input before `CONFIG_OK`.
Connection loss aborts the active sequence and clears the queue.

- [ ] **Step 5: Run Task 3 tests and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml protocol device config`

Expected: all focused Rust tests pass.

Commit:

```bash
rtk git add src-tauri/src/protocol.rs src-tauri/src/device.rs src-tauri/src/config.rs
rtk git commit -m "feat: execute ordered button actions"
```

---

### Task 4: Tauri Product Commands And Native File Dialogs

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `src-tauri/tauri.conf.json`
- Test: inline Rust tests in `src-tauri/src/lib.rs`

**Interfaces:**
- Produces camelCase `AppSnapshot { models, active_model, language, supported_gpios, connection, runtime_error }`.
- Produces Tauri commands `save_model`, `save_settings`, `preview_model_import`, `import_model`, `export_model`, `delete_model`, `preview_backup`, `export_backup`, `restore_backup`, `begin_learning`, and `end_learning`.
- Uses the official Tauri 2 dialog plugin only for native path selection; Rust commands own file IO.

- [ ] **Step 1: Install the official dialog plugin**

Run:

```bash
rtk npm install @tauri-apps/plugin-dialog
rtk cargo add --manifest-path src-tauri/Cargo.toml tauri-plugin-dialog
```

Expected: package and Cargo lockfiles contain compatible Tauri 2 dialog versions.

- [ ] **Step 2: Write failing command-level transaction tests**

Cover save-model atomicity, offline save success, same-ID import replacement,
delete-to-empty, backup restore, settings language validation, and learning pin
allowlist rejection through the non-Tauri inner functions.

- [ ] **Step 3: Run command tests and verify RED**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml workspace_command`

Expected: tests fail because the command inner functions do not exist.

- [ ] **Step 4: Implement state and command surface**

`AppState` owns `Arc<RwLock<Workspace>>`, device state, paths, and learning state.
Each command performs one backend operation, then returns a fresh snapshot. Add
all commands to `generate_handler!` and initialize the dialog plugin in the
Tauri builder. Package complete `../models/prod/*.yaml` files as production
presets and retain `../models/prod/*.json` as explicit legacy migration
resources; startup never treats layout-only JSON as an electrically complete
preset.

- [ ] **Step 5: Run Task 4 tests and commit**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml`

Expected: all Rust tests pass.

Commit:

```bash
rtk git add src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock package.json package-lock.json src-tauri/tauri.conf.json
rtk git commit -m "feat: manage model configuration files"
```

---

### Task 5: Localization And Serialized Auto-save

**Files:**
- Create: `src/i18n.ts`
- Create: `src/i18n.test.ts`
- Create: `src/useAutosave.ts`
- Create: `src/useAutosave.test.tsx`
- Modify: `src/types.ts`
- Modify: `src/App.tsx`

**Interfaces:**
- Produces `Language = "zh-CN" | "en-US"`, `MessageKey`, `t(language, key, params)`.
- Produces `SerializedSaveQueue.enqueue(task)` and `useAutosave<T>({ value, valid, delayMs, save, queue })` with status `idle | saving | saved | error`, `retry()`, and `flush()`.
- Mirrors all Task 1 model/config types in TypeScript.

- [ ] **Step 1: Write failing i18n and auto-save tests**

```tsx
test("defaults to complete Chinese product labels", () => {
  expect(t("zh-CN", "nav.behavior")).toBe("按键行为");
  expect(t("zh-CN", "save.failed")).toBe("保存失败");
  expect(t("en-US", "nav.behavior")).toBe("Button behavior");
});

test("serializes saves and persists the newest revision", async () => {
  const first = deferred<void>();
  const save = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValue(undefined);
  const queue = new SerializedSaveQueue();
  const { rerender } = renderHook(({ value }) => useAutosave({ value, valid: true, delayMs: 400, save, queue }), { initialProps: { value: 1 } });
  await vi.advanceTimersByTimeAsync(400);
  rerender({ value: 2 });
  await vi.advanceTimersByTimeAsync(400);
  expect(save).toHaveBeenCalledTimes(1);
  first.resolve();
  await waitFor(() => expect(save).toHaveBeenLastCalledWith(2));
});
```

- [ ] **Step 2: Run frontend tests and verify RED**

Run: `rtk npm test -- src/i18n.test.ts src/useAutosave.test.tsx`

Expected: modules do not exist.

- [ ] **Step 3: Implement the dictionary, frontend types, and save queue**

Use one typed dictionary object and string interpolation for named parameters.
`SerializedSaveQueue` owns the only promise chain. The hook debounces valid
model revisions for 400 ms, never marks a newer revision saved from an older
completion, retains errors, and exposes retry/flush. `App` creates one queue;
model auto-save and immediate settings operations both enqueue work on that same
instance.

- [ ] **Step 4: Run Task 5 tests and commit**

Run: `rtk npm test -- src/i18n.test.ts src/useAutosave.test.tsx`

Expected: focused frontend tests pass.

Commit:

```bash
rtk git add src/i18n.ts src/i18n.test.ts src/useAutosave.ts src/useAutosave.test.tsx src/types.ts src/App.tsx
rtk git commit -m "feat: localize and auto-save configuration"
```

---

### Task 6: Three-Column Product Workspace And Ordered Action Editor

**Files:**
- Create: `src/ModelSidebar.tsx`
- Create: `src/ActionEditor.tsx`
- Create: `src/HardwareMapping.tsx`
- Create: `src/ConfirmDialog.tsx`
- Modify: `src/Keypad.tsx`
- Modify: `src/LayoutEditor.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.css`
- Replace focused expectations in: `src/App.test.tsx`

**Interfaces:**
- `ModelSidebar` selects models/views and emits import/export/backup/delete commands.
- `ActionEditor` consumes `ButtonAction[]` and emits a complete valid ordered list.
- `HardwareMapping` edits direct/contact sources and exposes learning only under an advanced action.
- `ConfirmDialog` is the one accessible modal used for replacement, restore, and delete confirmations.

- [ ] **Step 1: Replace old staged-workspace tests with failing product-flow tests**

Add coverage for Chinese default, English switch, three columns, action counts,
adding two actions, editing paste text, recording a key, moving actions up/down,
auto-save invocation, save-error retry, import preview confirmation, full restore,
delete-to-empty, and the advanced learning entry remaining closed by default.

One core action test must assert the exact backend payload:

```tsx
expect(invoke).toHaveBeenCalledWith("save_model", {
  model: expect.objectContaining({
    actions: {
      DIGIT_2: [
        { type: "paste", text: "你好" },
        { type: "hotkey", keys: ["enter"] },
      ],
    },
  }),
});
```

- [ ] **Step 2: Run App tests and verify RED**

Run: `rtk npm test -- src/App.test.tsx`

Expected: failures show the old IO/Behavior segmented UI, global Save, and single-action editor.

- [ ] **Step 3: Implement the three-column shell and behavior editor**

Remove the global Save/Revert controls and IO/Behavior switch. The left sidebar
is 184 px, center uses the remaining width, and the right action editor is
360 px with responsive stacking below 900 px. Use existing Lucide icons with
tooltips, 6 px-or-smaller radii, stable keypad dimensions, Chinese default copy,
and no instructional marketing text.

- [ ] **Step 4: Implement hardware/data management states**

Hardware mapping edits direct keys and contact pairs without occupying the
default flow. Learning requires safety confirmation, board profile, and explicit
candidate pin selection. Native file dialogs call preview commands before
showing localized replacement/restore/delete confirmations. The empty workspace
shows Import model and Restore backup as primary commands.

- [ ] **Step 5: Run frontend tests and build, then commit**

Run:

```bash
rtk npm test
rtk npm run build
```

Expected: all frontend tests and the production TypeScript/Vite build pass.

Commit:

```bash
rtk git add src/App.tsx src/App.css src/App.test.tsx src/Keypad.tsx src/LayoutEditor.tsx src/ModelSidebar.tsx src/ActionEditor.tsx src/HardwareMapping.tsx src/ConfirmDialog.tsx
rtk git commit -m "feat: build product configuration workspace"
```

---

### Task 7: Presets, Full Verification, And Visual QA

**Files:**
- Create or migrate only measured complete files under `models/prod/*.yaml`
- Modify: `test/test_release.sh` only if packaging assertions require it
- Modify: `docs/superpowers/plans/2026-07-28-helper-product-configuration.md` checkbox statuses

**Interfaces:**
- Consumes all prior task interfaces.
- Produces one verified desktop application and one requirement-by-requirement completion record.

- [ ] **Step 1: Verify production preset policy**

Do not fabricate contact pairs. Convert a production model only when its current
repository data contains a complete measured topology; otherwise leave the
layout-only JSON as migration input and verify an empty production YAML catalog
is handled correctly.

- [ ] **Step 2: Run the complete automated suite**

Run:

```bash
rtk make test
rtk npm run build
rtk cargo build --manifest-path src-tauri/Cargo.toml
rtk git diff --check
```

Expected: every command exits `0`.

- [ ] **Step 3: Run desktop visual QA**

Start `rtk make helper`, then use Playwright at `1120x760`, `900x700`, and
`760x560`. Capture screenshots and verify: nonblank window, Chinese default,
three columns on desktop, responsive right-editor stacking, no overlapping text,
working model/action selection, save states, English switch, dialogs, and empty
workspace. Inspect screenshots with `view_image`.

- [ ] **Step 4: Audit every acceptance criterion**

For each criterion in
`docs/superpowers/specs/2026-07-28-helper-product-configuration-design.md`, cite
the exact test, source path, screenshot, build output, or explicitly unavailable
physical evidence. Do not claim the two real ESP32-S3 HID sequences were verified
unless a physical device was connected and observed.

- [ ] **Step 5: Commit final packaging or test adjustments**

```bash
rtk git add models/prod test/test_release.sh docs/superpowers/plans/2026-07-28-helper-product-configuration.md
rtk git commit -m "test: verify product configuration workflow"
```

Skip this commit when those paths have no changes; never create an empty commit.
