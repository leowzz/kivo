# Action Triggers And Device Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-Action press/release/long-press/double-press behavior, six-key left/right-aware hotkey chords, protocol-v6 execution, and a device-centered configuration workspace while preserving schema and firmware compatibility.

**Architecture:** Device Profile schema v3 stores trigger settings and four ordered Action groups per button. A pure host-side gesture engine turns debounced firmware edges into trigger occurrences, and `DeviceSession` serializes those occurrences into host-created protocol-v6 Action runs while keeping the protocol 3-5 input-event response path intact. React edits reusable profile drafts independently of physical-device assignments, uses a modal only for one Action or compact configuration settings, and embeds I/O Mapping and Key Layout as Device Management tabs.

**Tech Stack:** React 19, TypeScript 7, Vitest/Testing Library, Tauri 2, Rust 2024, serde YAML, C++17/Arduino, PlatformIO Unity, USB HID, Playwright CLI for visual verification.

---

## File Map

- Create `src-tauri/src/trigger.rs`: pure trigger state machine, deadlines, trigger occurrences, and deterministic fake-clock tests.
- Create `src/ActionDialog.tsx` and `src/ActionDialog.test.tsx`: one-Action draft lifecycle, trigger/type fields, validation, Save/Cancel/Delete.
- Create `src/HotkeyPicker.tsx` and `src/HotkeyPicker.test.tsx`: categorized searchable chord selection and physical-key recording.
- Create `src/ConfigurationSettingsDialog.tsx` and `src/ConfigurationSettingsDialog.test.tsx`: timing controls and duplicate-for-device command.
- Modify `src-tauri/src/profile.rs`: schema-v3 trigger types/settings/action groups, validation, protocol requirements.
- Modify `src-tauri/src/workspace.rs`: schema-1/schema-2 migration and atomic clone-and-assign transaction.
- Modify `src-tauri/src/protocol.rs`: six-key chord encoder, protocol-v6 command formatting, host-created run identifiers.
- Modify `src-tauri/src/device.rs`: gesture engine integration, run queueing, v3-v5 fallback, v6 capability gating.
- Modify `src-tauri/src/coordinator.rs`: timer polling and update-required status propagation.
- Modify `src-tauri/src/lib.rs`: clone-and-assign Tauri command and runtime snapshot fan-out.
- Create `lib/gpio_trigger/src/ActionRunController.{h,cpp}`: one protocol-v6 host-created active run independent of input event IDs.
- Modify `lib/gpio_trigger/src/GpioTriggerController.{h,cpp}`: emit debounced v6 Down/Up edges without firmware-side pending Action responses.
- Modify `lib/gpio_trigger/src/TriggerProtocol.{h,cpp}`: `CHORD` parsing and v6 helper command representation.
- Modify `src/main.cpp`: protocol-v6 run dispatch, delays, acknowledgements, and reset behavior.
- Modify `src/platform/Platform.h`, `src/platform/rp2040.cpp`, and `src/platform/esp32s3.cpp`: six-slot keyboard reports including modifier-only reports.
- Modify `test/test_gpio_trigger/test_main.cpp`: native parser, run controller, HID report, delay, and scan regression tests.
- Modify `scripts/smoke_runtime_protocol.py` and `test/test_runtime_smoke.py`: v6 handshake/CHORD smoke contract.
- Modify `models/prod/key9.yaml`: bundled schema-v3 profile.
- Modify `src/types.ts`, `src/hotkey.ts`, and their tests: shared frontend trigger/chord types and validation.
- Modify `src/App.tsx`, `src/App.test.tsx`, and `src/App.preview.test.tsx`: independent profile drafts/autosave, navigation, page ownership, preview states.
- Modify `src/ActionEditor.tsx` and `src/ActionEditor.test.tsx`: compact trigger-group summaries and dialog launch points.
- Modify `src/DeviceManagement.tsx` and `src/DeviceManagement.test.tsx`: device/profile selector, tabs, shared warning, inline hardware resolution.
- Modify `src/HardwareMapping.tsx`, `src/HardwareMapping.test.tsx`, and `src/LayoutEditor.tsx`: embedded editors controlled by the selected device/profile.
- Modify `src/Keypad.tsx` and `src/Keypad.test.tsx`: trigger-group Action counts.
- Modify `src/preview.ts`, `src/i18n.ts`, `src/i18n.test.ts`, and `src/styles/views.css`: v3 fixtures, labels, responsive dialog/page styling.

### Task 1: Device Profile Schema V3 And Migration

**Files:**

- Modify: `src-tauri/src/profile.rs:8-125,259-410,480-760`
- Modify: `src-tauri/src/workspace.rs:106-135,234-359,830-963,1196-1237,1390-1970`
- Modify: `models/prod/key9.yaml:1-42`

- [x] **Step 1: Write failing schema, validation, and migration tests**

Add tests that deserialize missing groups as empty, omit empty groups during serialization, reject timing outside the approved bounds, migrate every schema-2 Action to `press`, and migrate schema 1 directly to schema 3 without changing IDs or Action order:

```rust
#[test]
fn schema_v3_defaults_trigger_settings_and_omits_empty_groups() {
    let profile: DeviceProfile = serde_yaml_ng::from_str(
        "schema_version: 3\nprofile:\n  id: pad\n  name: Pad\n  groups:\n    - id: keys\n      columns: 1\n      buttons:\n        - { id: A, label: A }\ntrigger_settings: {}\nhardware_profiles: []\nactions:\n  A:\n    press:\n      - { type: delay, duration_ms: 10 }\n",
    ).unwrap();
    assert_eq!(profile.trigger_settings, TriggerSettings::default());
    assert_eq!(profile.actions["A"].press.len(), 1);
    assert!(profile.actions["A"].release.is_empty());
    let yaml = serde_yaml_ng::to_string(&profile).unwrap();
    assert!(!yaml.contains("release:"));
}

#[test]
fn trigger_timing_bounds_are_enforced() {
    let mut profile = valid_profile();
    profile.trigger_settings.long_press_ms = 99;
    assert_eq!(profile.validate().unwrap_err().code, "invalid_long_press_ms");
    profile.trigger_settings.long_press_ms = 500;
    profile.trigger_settings.double_press_ms = 1001;
    assert_eq!(profile.validate().unwrap_err().code, "invalid_double_press_ms");
}

#[test]
fn rejects_removed_or_unknown_trigger_names() {
    let yaml = valid_profile_yaml().replace("actions: {}", "actions:\n  A:\n    short_press: []");
    assert!(serde_yaml_ng::from_str::<DeviceProfile>(&yaml).is_err());
}

#[test]
fn load_migrates_schema_v2_actions_to_press_without_reordering() {
    let workspace = load_workspace_with_profile_yaml(r#"
schema_version: 2
profile: { id: phone, name: Phone, groups: [] }
hardware_profiles: []
actions:
  HANDSET:
    - { type: open, target: Phone.app }
    - { type: media, command: play_pause }
"#);
    let actions = &workspace.profiles["phone"].actions["HANDSET"];
    assert_eq!(workspace.profiles["phone"].schema_version, 3);
    assert!(matches!(actions.press[0], ButtonAction::Open { .. }));
    assert!(matches!(actions.press[1], ButtonAction::Media { .. }));
}
```

- [x] **Step 2: Run focused Rust tests and verify the red state**

Run: `cargo test --manifest-path src-tauri/Cargo.toml schema_v3 -- --nocapture`

Expected: FAIL because `TriggerSettings`, `TriggerActions`, and schema 2 profile migration do not exist.

- [x] **Step 3: Add the v3 types, defaults, validation, and version-aware loader**

Use these canonical types in `profile.rs`:

```rust
pub const PROFILE_SCHEMA_VERSION: u16 = 3;
pub const DEFAULT_LONG_PRESS_MS: u32 = 500;
pub const DEFAULT_DOUBLE_PRESS_MS: u32 = 300;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionTrigger { Press, Release, LongPress, DoublePress }

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TriggerSettings {
    #[serde(default = "default_long_press_ms")]
    pub long_press_ms: u32,
    #[serde(default = "default_double_press_ms")]
    pub double_press_ms: u32,
}

impl Default for TriggerSettings {
    fn default() -> Self {
        Self { long_press_ms: DEFAULT_LONG_PRESS_MS, double_press_ms: DEFAULT_DOUBLE_PRESS_MS }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct TriggerActions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub press: Vec<ButtonAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub release: Vec<ButtonAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub long_press: Vec<ButtonAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub double_press: Vec<ButtonAction>,
}

impl TriggerActions {
    pub fn get(&self, trigger: ActionTrigger) -> &[ButtonAction] {
        match trigger {
            ActionTrigger::Press => &self.press,
            ActionTrigger::Release => &self.release,
            ActionTrigger::LongPress => &self.long_press,
            ActionTrigger::DoublePress => &self.double_press,
        }
    }
}
```

Add `trigger_settings: TriggerSettings` to `DeviceProfile`, change `actions` to `BTreeMap<String, TriggerActions>`, and validate `100..=5000` plus `100..=1000`. In `workspace.rs`, add one `read_device_profile(path, allow_schema_2)` function: inspect `SchemaHeader`, deserialize schema 3 directly, or deserialize schema 2 into a private `LegacyDeviceProfileV2` and map every `Vec<ButtonAction>` to `TriggerActions { press, ..Default::default() }`. Use it from workspace load, import preview, and import commit; bundled profiles accept schema 3 only. Atomically rewrite migrated workspace files, make `migrate_schema_v1_model` use the same grouping rule, and keep imported IDs unchanged. Change `action_count` to sum all four lists. Update `models/prod/key9.yaml` to `schema_version: 3` and add default `trigger_settings`.

- [x] **Step 4: Run all profile/workspace tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml profile`

Expected: PASS, including schema 1 and schema 2 migration tests.

Run: `cargo test --manifest-path src-tauri/Cargo.toml workspace`

Expected: PASS with migrated files serialized as schema 3.

- [x] **Step 5: Commit the schema boundary**

```bash
git add src-tauri/src/profile.rs src-tauri/src/workspace.rs models/prod/key9.yaml
git commit -m "feat: add triggered action profile schema"
```

### Task 2: Canonical Six-Key Chord Encoding

**Files:**

- Modify: `src-tauri/src/protocol.rs:490-620,680-930`
- Modify: `src-tauri/src/profile.rs:350-410,570-640`
- Modify: `src/hotkey.ts:1-110`
- Modify: `src/hotkey.test.ts:1-180`

- [x] **Step 1: Write failing Rust and TypeScript chord tests**

Cover all eight modifier bits, legacy aliases, F13-F24, six distinct ordinary keys, modifier-only chords, duplicate HID usages, and a seventh key:

```rust
#[test]
fn encodes_sided_modifiers_and_six_ordinary_keys() {
    let chord = encode_hotkey(&[
        "left_cmd", "right_cmd", "a", "b", "c", "d", "e", "f",
    ].map(str::to_owned)).unwrap();
    assert_eq!(chord.modifier_mask, 0x88);
    assert_eq!(chord.keycodes, vec![0x04, 0x05, 0x06, 0x07, 0x08, 0x09]);
}

#[test]
fn accepts_modifier_only_and_rejects_duplicate_usage_or_seventh_key() {
    assert_eq!(encode_hotkey(&["right_alt".into()]).unwrap().keycodes, vec![]);
    assert!(encode_hotkey(&["a".into(), "A".into()]).is_err());
    assert!(encode_hotkey(&["a", "b", "c", "d", "e", "f", "g"].map(str::to_owned)).is_err());
}
```

```ts
test("maps physical modifier codes without collapsing sides", () => {
  expect(keyboardCodeToToken("MetaLeft")).toBe("left_cmd");
  expect(keyboardCodeToToken("MetaRight")).toBe("right_cmd");
  expect(keyboardCodeToToken("AltRight")).toBe("right_alt");
});

test("validates modifier-only and six-key chords", () => {
  expect(validateHotkey(["right_cmd"])).toBeNull();
  expect(validateHotkey(["a", "b", "c", "d", "e", "f"])).toBeNull();
  expect(validateHotkey(["a", "b", "c", "d", "e", "f", "g"])).toBe("too_many_keys");
});
```

- [x] **Step 2: Verify both focused suites fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml encode_hotkey -- --nocapture`

Expected: FAIL because `encode_hotkey` returns one keycode and rejects modifier-only input.

Run: `npm test -- src/hotkey.test.ts`

Expected: FAIL because physical side tokens and six-key validation are missing.

- [x] **Step 3: Implement one shared token vocabulary in Rust and TypeScript**

Change Rust encoding to:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedChord {
    pub modifier_mask: u8,
    pub keycodes: Vec<u8>,
}

pub fn encode_hotkey(keys: &[String]) -> Result<EncodedChord, String> {
    let mut modifier_mask = 0_u8;
    let mut keycodes = Vec::new();
    for token in keys {
        if let Some(bit) = modifier_bit(token) {
            if modifier_mask & bit != 0 { return Err("duplicate_modifier".into()); }
            modifier_mask |= bit;
        } else {
            let usage = keyboard_usage(token).ok_or("unsupported_hotkey")?;
            if keycodes.contains(&usage) { return Err("duplicate_key".into()); }
            if keycodes.len() == 6 { return Err("too_many_keys".into()); }
            keycodes.push(usage);
        }
    }
    if modifier_mask == 0 && keycodes.is_empty() { return Err("empty_hotkey".into()); }
    Ok(EncodedChord { modifier_mask, keycodes })
}
```

Map `ctrl/shift/alt/option/cmd` to left bits, `primary` to left GUI on macOS and left Ctrl otherwise, and explicit `left_*`/`right_*` to bits 0-7. In TypeScript export `HOTKEY_CATEGORIES`, `keyboardCodeToToken`, `isModifierToken`, `validateHotkey`, and `formatHotkey`; keep the same canonical token strings and HID aliases as Rust. Update profile validation to call `encode_hotkey` for every hotkey Action.

- [x] **Step 4: Run chord and profile validation suites**

Run: `cargo test --manifest-path src-tauri/Cargo.toml encode_hotkey`

Expected: PASS.

Run: `npm test -- src/hotkey.test.ts`

Expected: PASS.

- [x] **Step 5: Commit chord encoding**

```bash
git add src-tauri/src/protocol.rs src-tauri/src/profile.rs src/hotkey.ts src/hotkey.test.ts
git commit -m "feat: support six-key sided hotkey chords"
```

### Task 3: Protocol V6 Parsing And Host-Created Runs

**Files:**

- Modify: `src-tauri/src/protocol.rs:1-95,357-488,620-930`
- Modify: `lib/gpio_trigger/src/TriggerProtocol.h:1-90`
- Modify: `lib/gpio_trigger/src/TriggerProtocol.cpp:1-220`
- Create: `lib/gpio_trigger/src/ActionRunController.h`
- Create: `lib/gpio_trigger/src/ActionRunController.cpp`
- Modify: `lib/gpio_trigger/src/GpioTriggerController.h:1-110`
- Modify: `lib/gpio_trigger/src/GpioTriggerController.cpp:110-230`
- Modify: `test/test_gpio_trigger/test_main.cpp:110-380,590-730,796-849`
- Modify: `scripts/smoke_runtime_protocol.py`
- Modify: `test/test_runtime_smoke.py`

- [x] **Step 1: Write failing host and native protocol-v6 tests**

Assert `HOST_PROTOCOL_VERSION == 6`, CHORD formatting, exact key count, unique nonzero supported usages, modifier-only acceptance, active-run creation on step 1, ordered later steps, SKIP, and no pending Action entry on a v6 `STATE` edge:

```rust
#[test]
fn formats_v6_chord_command() {
    let step = ActionStep {
        run_id: 7,
        button: "A".into(),
        trigger: ActionTrigger::Press,
        step: 1,
        total: 1,
        action: ButtonAction::Hotkey {
            keys: vec!["right_cmd".into(), "a".into(), "b".into()],
        },
    };
    assert_eq!(step.command_v6(|_| Ok(())).unwrap(), "CHORD 7 1 1 128 2 4 5\n");
}
```

```cpp
void test_parses_v6_chord_and_rejects_malformed_chords() {
  const auto chord = parseHelperCommand("CHORD 7 1 2 128 2 4 5\n");
  TEST_ASSERT_TRUE(chord.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Chord, chord->kind);
  TEST_ASSERT_EQUAL_UINT8(2, chord->keycodes.size());
  TEST_ASSERT_EQUAL_UINT8(5, chord->keycodes[1]);
  TEST_ASSERT_TRUE(parseHelperCommand("CHORD 8 1 1 128 0\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("CHORD 8 1 1 0 0\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("CHORD 8 1 1 0 2 4 4\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("CHORD 8 1 1 0 1 0\n").has_value());
}

void test_v6_run_starts_on_step_one_and_is_independent_of_input_ids() {
  ActionRunController runs;
  TEST_ASSERT_EQUAL(ResponseAction::Ignored, runs.acceptStep(41, 2, 2, 0));
  TEST_ASSERT_EQUAL(ResponseAction::Execute, runs.acceptStep(41, 1, 2, 1));
  TEST_ASSERT_EQUAL(ResponseAction::Execute, runs.acceptStep(41, 2, 2, 2));
  TEST_ASSERT_FALSE(runs.hasActiveRun());
}
```

- [x] **Step 2: Verify the protocol tests fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml protocol -- --nocapture`

Expected: FAIL because v6 and `command_v6` are absent.

Run: `uv run pio test -e native`

Expected: FAIL because `CHORD` and `ActionRunController` are absent.

- [x] **Step 3: Implement the v6 contract without deleting the legacy path**

Set `HOST_PROTOCOL_VERSION` to 6 and retain `MIN_SUPPORTED_PROTOCOL_VERSION = 3`. Rename `ActionStep.event_id` and `ActionSequence.event_id` to `run_id`; include `trigger: ActionTrigger` for activity attribution. Add `command_v6`, while `command_legacy` continues emitting one-key `HOTKEY` and returns `protocol_update_required` for modifier-only or multi-key chords.

In firmware, add this representation and controller:

```cpp
enum class HelperCommandKind {
  ConfigBegin, ConfigOled, ConfigDirect, ConfigMatrix, ConfigCommit,
  LearnBegin, LearnEnd, Paste, Hotkey, Chord, Delay, Media, Host, Skip
};

struct HelperCommand {
  HelperCommandKind kind;
  std::uint32_t runId = 0;
  std::uint16_t step = 0;
  std::uint16_t total = 0;
  std::uint8_t modifierMask = 0;
  std::vector<std::uint8_t> keycodes;
};

class ActionRunController {
 public:
  ResponseAction acceptStep(std::uint32_t runId, std::uint16_t step,
                            std::uint16_t total, std::uint32_t nowMs);
  ResponseAction cancel(std::uint32_t runId);
  bool keepAlive(std::uint32_t runId, std::uint32_t nowMs);
  void expire(std::uint32_t nowMs);
 void reset();
  bool hasActiveRun() const;
 private:
  struct ActiveRun {
    std::uint32_t id;
    std::uint16_t nextStep;
    std::uint16_t total;
    std::uint32_t refreshedAtMs;
  };
  std::optional<ActiveRun> active_;
};
```

`parseHelperCommand` must verify the CHORD count equals the remaining tokens, count is at most six, usages are unique and supported, mask/key combination is nonempty, and the full line stays within the existing 255-byte bound. Parse `DONE` on the host as `DeviceMessage::Done { run_id, step }`. Remove pending Action ownership from `GpioTriggerController`; protocol-v6 `updateInput` only emits debounced state, while `ActionRunController` accepts host-created runs. Keep protocol 3-5 compatibility entirely in the host's legacy command path because those devices run their existing firmware. Update the runtime smoke script to expect `HELLO 6` and validate a CHORD/DONE exchange.

- [x] **Step 4: Run protocol and native suites**

Run: `cargo test --manifest-path src-tauri/Cargo.toml protocol`

Expected: PASS, including legacy HOTKEY formatting tests.

Run: `uv run pio test -e native`

Expected: PASS, including malformed CHORD and host-created run cases.

Run: `uv run pytest test/test_runtime_smoke.py`

Expected: PASS with protocol-v6 fixture traffic.

- [x] **Step 5: Commit the wire contract**

```bash
git add src-tauri/src/protocol.rs lib/gpio_trigger/src/TriggerProtocol.h lib/gpio_trigger/src/TriggerProtocol.cpp lib/gpio_trigger/src/ActionRunController.h lib/gpio_trigger/src/ActionRunController.cpp lib/gpio_trigger/src/GpioTriggerController.h lib/gpio_trigger/src/GpioTriggerController.cpp test/test_gpio_trigger/test_main.cpp scripts/smoke_runtime_protocol.py test/test_runtime_smoke.py
git commit -m "feat: add protocol v6 action runs and chords"
```

### Task 4: Six-Slot HID Reports On Both Controllers

**Files:**

- Modify: `src/platform/Platform.h:1-25`
- Modify: `src/platform/HidReportTransport.h`
- Modify: `src/platform/rp2040.cpp:60-115`
- Modify: `src/platform/esp32s3.cpp:1-70`
- Modify: `src/main.cpp:1-280`
- Modify: `test/test_gpio_trigger/test_main.cpp:720-849`

- [x] **Step 1: Write failing native HID transport tests**

Extract report construction into a platform-neutral helper and verify six usages, all modifier bits, modifier-only press, empty release, and transport backpressure:

```cpp
void test_keyboard_chord_sends_pressed_then_empty_release_report() {
  std::vector<KeyboardReport> reports;
  HidReportTransport transport(
      [&reports](const KeyboardReport &report) { reports.push_back(report); return true; },
      [] {});
  TEST_ASSERT_TRUE(transport.sendKeyboardChord(0xFF, {4, 5, 6, 7, 8, 9}));
  TEST_ASSERT_EQUAL_UINT8(0xFF, reports[0].modifiers);
  TEST_ASSERT_EQUAL_UINT8(9, reports[0].keys[5]);
  TEST_ASSERT_EQUAL_UINT8(0, reports[1].modifiers);
  TEST_ASSERT_EQUAL_UINT8(0, reports[1].keys[0]);
}

void test_modifier_only_chord_is_not_dropped() {
  auto reports = captureKeyboardReports(0x80, {});
  TEST_ASSERT_EQUAL_UINT8(0x80, reports[0].modifiers);
  TEST_ASSERT_EQUAL_UINT8(0, reports[0].keys[0]);
  TEST_ASSERT_EQUAL_UINT8(0, reports[1].modifiers);
}

void test_input_scanning_continues_while_delay_run_is_active() {
  ActionRunController runs;
  auto controller = directController(0);
  TEST_ASSERT_EQUAL(ResponseAction::Execute, runs.acceptStep(88, 1, 2, 10));
  TEST_ASSERT_TRUE(runs.keepAlive(88, 2000));
  controller.updatePin(6, false, 20);
  TEST_ASSERT_TRUE(controller.updatePin(6, false, 50).has_value());
  TEST_ASSERT_TRUE(runs.hasActiveRun());
}
```

- [x] **Step 2: Verify the native suite fails**

Run: `uv run pio test -e native`

Expected: FAIL because the platform API accepts one `keycode`.

- [x] **Step 3: Implement six-slot reports and wire CHORD dispatch**

Use this platform contract:

```cpp
using KeyboardUsages = std::array<std::uint8_t, 6>;
bool sendHotkey(std::uint8_t modifiers, const KeyboardUsages &keycodes);
```

In RP2040 fill `hid_keyboard_report_t.keycode[0..5]`; in ESP32-S3 fill `KeyReport.keys[0..5]`. Both implementations must send the complete pressed report and then zeroed release report, waiting for both report slots as the current transport does. In `main.cpp`, copy `command->keycodes` into the fixed array for CHORD, keep HOTKEY for v3-v5, acknowledge with `DONE <run_id> <step>`, and clear `ActionRunController` on reconnect, configuration commit, and `SKIP`.

- [x] **Step 4: Run native and both firmware builds**

Run: `uv run pio test -e native`

Expected: PASS.

Run: `make build-esp32s3`

Expected: PASS.

Run: `make build-rp2040`

Expected: PASS.

- [x] **Step 5: Commit the HID implementation**

```bash
git add src/platform/Platform.h src/platform/HidReportTransport.h src/platform/rp2040.cpp src/platform/esp32s3.cpp src/main.cpp test/test_gpio_trigger/test_main.cpp
git commit -m "feat: send six-key HID chord reports"
```

### Task 5: Pure Host Gesture Tracker

**Files:**

- Create: `src-tauri/src/trigger.rs`
- Modify: `src-tauri/src/lib.rs:1-30`

- [x] **Step 1: Write exhaustive fake-clock tests in the new module**

Define tests for Press, Release, Long Press once, Double Press on Down-Up-Down, Press-before-Double ordering, long-press invalidation, second-press long hold, duplicate edges, reset, and independent inputs:

```rust
#[test]
fn long_hold_emits_press_long_press_then_release() {
    let input = PhysicalInput::Direct { gpio: 6 };
    let snapshot = snapshot(500, 300);
    let mut tracker = TriggerTracker::default();
    assert_eq!(tracker.edge(edge(input, InputState::Down, 10, snapshot.clone())), vec![occurrence(ActionTrigger::Press, 10)]);
    assert_eq!(tracker.poll(509), vec![]);
    assert_eq!(tracker.poll(510), vec![occurrence(ActionTrigger::LongPress, 10)]);
    assert_eq!(tracker.poll(900), vec![]);
    assert_eq!(tracker.edge(edge(input, InputState::Up, 901, snapshot)), vec![occurrence(ActionTrigger::Release, 10)]);
}

#[test]
fn complete_second_down_emits_press_before_double_press() {
    let mut tracker = TriggerTracker::default();
    let snapshot = snapshot(500, 300);
    tracker.edge(edge(direct(6), InputState::Down, 0, snapshot.clone()));
    tracker.edge(edge(direct(6), InputState::Up, 40, snapshot.clone()));
    let result = tracker.edge(edge(direct(6), InputState::Down, 200, snapshot));
    assert_eq!(result.iter().map(|item| item.trigger).collect::<Vec<_>>(),
               vec![ActionTrigger::Press, ActionTrigger::DoublePress]);
}
```

- [x] **Step 2: Run the module test and verify it fails to compile**

Run: `cargo test --manifest-path src-tauri/Cargo.toml trigger -- --nocapture`

Expected: FAIL because `trigger.rs` and `TriggerTracker` do not exist.

- [x] **Step 3: Implement deterministic edge and deadline state**

Use these public interfaces:

```rust
#[derive(Clone, Debug)]
pub struct TriggerOccurrence {
    pub sequence: u64,
    pub input: PhysicalInput,
    pub trigger: ActionTrigger,
    pub origin_down_ms: u64,
    pub snapshot: Arc<RuntimeProfileSnapshot>,
    pub context: Option<RuntimeEventContext>,
}

#[derive(Default)]
pub struct TriggerTracker {
    inputs: BTreeMap<PhysicalInput, InputGesture>,
    next_sequence: u64,
}

impl TriggerTracker {
    pub fn edge(&mut self, edge: TriggerEdge) -> Vec<TriggerOccurrence>;
    pub fn poll(&mut self, now_ms: u64) -> Vec<TriggerOccurrence>;
    pub fn next_deadline_ms(&self) -> Option<u64>;
    pub fn reset(&mut self);
}
```

`TriggerEdge` carries the physical input, state, timestamp, snapshot, and context; the test helper `edge(input, state, now_ms, snapshot)` constructs it with `context: None`. Derive `Ord`/`PartialOrd` for `PhysicalInput` so it is a stable `BTreeMap` key. Store the snapshot/context from each accepted Down and use them for its Long/Double occurrence. Increment `next_sequence` with a nonzero wrapping helper. Ignore repeated Down or Up states. A fired Long Press clears that press as a double candidate; Up cancels only an unfired long deadline.

Use this exact test helper beside the module tests:

```rust
fn edge(
    input: PhysicalInput,
    state: InputState,
    now_ms: u64,
    snapshot: Arc<RuntimeProfileSnapshot>,
) -> TriggerEdge {
    TriggerEdge { input, state, now_ms, snapshot, context: None }
}
```

- [x] **Step 4: Run the trigger suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml trigger`

Expected: PASS for all edge, timer, reset, and independent-input cases.

- [x] **Step 5: Commit the trigger engine**

```bash
git add src-tauri/src/trigger.rs src-tauri/src/lib.rs
git commit -m "feat: recognize host-side action triggers"
```

### Task 6: Integrate Triggers And V6 Runs Into DeviceSession

**Files:**

- Modify: `src-tauri/src/device.rs:100-180,390-620,620-1060,1900-3140`
- Modify: `src-tauri/src/coordinator.rs:280-390,620-720,1040-1120,1440-1580,2260-3060`
- Modify: `src-tauri/src/protocol.rs:357-488`

- [x] **Step 1: Write failing DeviceSession integration tests**

Cover v6 Down/Up triggering, timer polling without serial input, host run IDs differing from input IDs, queue serialization, reset events, timeout and malformed-ack isolation, metrics on Down only, and v3-v5 legacy behavior:

```rust
#[test]
fn v6_pickup_and_hangup_queue_distinct_host_runs() {
    let mut session = configured_session_with_protocol(6, profile_with_handset_edges());
    let down = session.on_line_deferred("STATE 91 DIRECT 6 DOWN\n", 1, 100);
    assert!(down.lines[0].starts_with("HOST 1 1 1"));
    session.on_line_deferred("DONE 1 1\n", 2, 101);
    let up = session.on_line_deferred("STATE 92 DIRECT 6 UP\n", 3, 200);
    assert!(up.lines[0].starts_with("MEDIA 2 1 1"));
}

#[test]
fn timer_poll_fires_long_press_and_later_queue_survives_timeout() {
    let mut session = configured_session_with_protocol(6, profile_with_long_press());
    session.on_line_deferred("STATE 5 DIRECT 6 DOWN\n", 1, 0);
    assert!(session.poll(499).lines.is_empty());
    let long = session.poll(500);
    assert_eq!(long.activities[0].params["trigger"], "long_press");
    session.on_action_timeout(2, 2500);
    assert!(session.has_queued_occurrences());
}
```

- [x] **Step 2: Run focused DeviceSession tests and verify the red state**

Run: `cargo test --manifest-path src-tauri/Cargo.toml device::tests -- --nocapture`

Expected: FAIL because sessions queue only Down input IDs and have no timer poll.

- [x] **Step 3: Replace `QueuedInput` with trigger occurrences and branch by negotiated protocol**

Add fields `triggers: TriggerTracker` and `next_run_id: u64` to `DeviceSession`. For v6, pass every stable edge into the tracker, enqueue each returned occurrence in sequence order, allocate a host run ID when an occurrence starts, load Actions from `TriggerActions::get`, and emit `command_v6`. For v3-v5, respond only to Down using the event ID and `press` group via `command_legacy`; always emit `SKIP <event_id>` if there is no compatible Press Action so old firmware clears its pending response.

Expose:

```rust
impl DeviceSession {
    pub fn poll_triggers(&mut self, now_ms: u64) -> SessionOutput;
    pub fn next_trigger_deadline_ms(&self) -> Option<u64>;
    fn clear_gestures(&mut self) { self.triggers.reset(); }
    fn next_run_id(&mut self) -> u64;
}
```

Call `clear_gestures` on disconnect, assignment/snapshot replacement, topology reconfiguration, and learning entry. Record `input_state` for physical edges and `trigger_occurred` with `button`/`trigger` for derived occurrences. Keep `MetricPress` on Down edges only. On a timeout or malformed `DONE`, abort and `SKIP` only the active run, emit structured `action_timeout` or `invalid_action_acknowledgement` activity, and immediately start the next queued occurrence. Update the coordinator worker loop to use `recv_timeout` bounded by `next_trigger_deadline_ms`, then call `poll_triggers(clock.monotonic_ms())` even if no serial line arrives.

- [x] **Step 4: Run runtime, coordinator, and protocol tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml device::tests`

Expected: PASS.

Run: `cargo test --manifest-path src-tauri/Cargo.toml coordinator::tests`

Expected: PASS, including timer-only wakeup and reset cases.

Run: `cargo test --manifest-path src-tauri/Cargo.toml protocol::tests`

Expected: PASS for v3-v6 Action sequencing.

- [x] **Step 5: Commit runtime integration**

```bash
git add src-tauri/src/device.rs src-tauri/src/coordinator.rs src-tauri/src/protocol.rs
git commit -m "feat: execute triggered actions with host run ids"
```

### Task 7: Compatibility Gating And Atomic Duplicate-And-Assign

**Files:**

- Modify: `src-tauri/src/profile.rs:390-430`
- Modify: `src-tauri/src/workspace.rs:400-760,1380-1500,2100-2220`
- Modify: `src-tauri/src/device.rs:640-700,2100-2160`
- Modify: `src-tauri/src/coordinator.rs:1440-1580,1720-1780`
- Modify: `src-tauri/src/lib.rs:320-490,650-830,1010-1060,1880-1980`

- [x] **Step 1: Write failing compatibility and transaction tests**

Test that non-Press groups, multiple ordinary keys, and modifier-only chords require v6; schema-v3 Press with one ordinary key still works on v3-v5; ambiguous mappings do not change assignment; and clone failure leaves disk and assignment untouched:

```rust
#[test]
fn profile_protocol_requirement_tracks_trigger_and_chord_features() {
    assert_eq!(press_only_single_key_profile().minimum_protocol_version(), 3);
    assert_eq!(release_profile().minimum_protocol_version(), 6);
    assert_eq!(multi_key_profile().minimum_protocol_version(), 6);
    assert_eq!(modifier_only_profile().minimum_protocol_version(), 6);
}

#[test]
fn duplicate_and_assign_is_atomic_and_generates_unique_ids() {
    let mut workspace = workspace_shared_by_two_devices();
    let result = workspace.duplicate_profile_for_device(DuplicateProfileForDeviceRequest {
        device_id: device_a(),
        source_profile: edited_phone_profile(),
        name: "Phone copy".into(),
    }).unwrap();
    assert_ne!(result.profile.profile.id, "phone");
    assert_ne!(result.profile.hardware_profiles[0].id, "hardware");
    assert_eq!(workspace.settings.devices[&device_b()].runtime_assignment.as_ref().unwrap().device_profile_id, "phone");
    assert_eq!(workspace.settings.devices[&device_a()].runtime_assignment.as_ref().unwrap().device_profile_id, result.profile.profile.id);
}

fn edited_phone_profile() -> DeviceProfile {
    let mut profile = phone_profile_fixture();
    profile.trigger_settings.long_press_ms = 700;
    profile
}
```

- [x] **Step 2: Verify focused tests fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml minimum_protocol_version -- --nocapture`

Expected: FAIL because protocol requirement calculation does not exist.

Run: `cargo test --manifest-path src-tauri/Cargo.toml duplicate_and_assign -- --nocapture`

Expected: FAIL because the clone-and-assign transaction does not exist.

- [x] **Step 3: Implement capability status, profile resolution, and one transaction command**

Add `DeviceProfile::minimum_protocol_version() -> u16`. Return 6 for any nonempty `release`, `long_press`, or `double_press` group, or any hotkey whose encoded chord has zero or more than one ordinary usages; otherwise preserve existing OLED/media/open gating.

Implement:

```rust
pub struct DuplicateProfileForDeviceRequest {
    pub device_id: DeviceId,
    pub source_profile: DeviceProfile,
    pub name: String,
}

pub fn duplicate_profile_for_device(
    &mut self,
    request: DuplicateProfileForDeviceRequest,
) -> Result<DeviceProfile, AppError>;
```

Require `source_profile.profile.id` to identify an existing profile, but clone the submitted validated draft so unsaved I/O, layout, timing, or Action changes become part of the device-only copy without changing the shared source. Build the cloned profiles/settings in memory, allocate unique IDs with the existing slug/unique helpers, rewrite every cloned Hardware Profile ID and the selected Hardware Profile ID in the assignment, validate the new assignment against the device board, write profile plus settings to a staged data generation, then replace in-memory state only after durable writes succeed. Expose `duplicate_profile_for_device` as a logged Tauri command in `lib.rs`, include it in `generate_handler!`, and fan the changed snapshot to only affected workers. When a selected configuration has no preserved compatible mapping or more than one candidate, return `hardware_resolution_required` without mutating the assignment. Before persisting a runtime assignment for an online device, have `lib.rs` compare the observed firmware protocol with `profile.minimum_protocol_version()` and return `firmware_update_required` if it is too old; offline assignments are validated when the device next connects. Surface a connected incompatibility as update-required status rather than `invalid_assignment`.

- [x] **Step 4: Run workspace, command, and coordinator tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml duplicate_profile_for_device`

Expected: PASS, including injected write failure rollback.

Run: `cargo test --manifest-path src-tauri/Cargo.toml firmware_update_required`

Expected: PASS for protocol 3-5 and protocol 6 fixtures.

- [x] **Step 5: Commit ownership and compatibility operations**

```bash
git add src-tauri/src/profile.rs src-tauri/src/workspace.rs src-tauri/src/device.rs src-tauri/src/coordinator.rs src-tauri/src/lib.rs
git commit -m "feat: gate v6 profiles and clone shared configurations"
```

### Task 8: Frontend V3 Types And Per-Profile Draft Autosave

**Files:**

- Modify: `src/types.ts:1-220`
- Modify: `src/preview.ts`
- Modify: `src/App.tsx:51-250,280-510,760-1140`
- Modify: `src/App.test.tsx`
- Modify: `src/App.preview.test.tsx`
- Modify: `src/Keypad.tsx`
- Modify: `src/Keypad.test.tsx`

- [x] **Step 1: Write failing type-flow and autosave tests**

Add a test that selects profile B in Button Behavior without changing device A's assignment, edits profile B, switches Device Management to profile A, and observes profile B saved through the serialized queue. Add Keypad count coverage across trigger groups:

```ts
test("editor target and device assignment remain independent", async () => {
  render(<App />);
  await user.click(screen.getByRole("button", { name: "Button Behavior" }));
  await user.selectOptions(screen.getByLabelText("Configuration to edit"), "profile-b");
  expect(saveSettings).toHaveBeenLastCalledWith(expect.objectContaining({ editorProfile: "profile-b" }));
  await user.click(screen.getByRole("button", { name: "Device Management" }));
  expect(screen.getByLabelText("Use configuration")).toHaveValue("profile-a");
  expect(saveRuntimeAssignment).not.toHaveBeenCalled();
});

test("counts actions in every trigger group", () => {
  render(<Keypad layout={layout} actions={{ A: {
    press: [{ type: "delay", duration_ms: 1 }], release: [],
    long_press: [{ type: "delay", duration_ms: 2 }], double_press: [],
  } }} selectedButtonId={null} pressedButtonIds={new Set()}
      actionCountLabel={(count) => `${count} actions`} onSelect={() => {}} />);
  expect(screen.getByText("2 actions")).toBeInTheDocument();
});
```

- [x] **Step 2: Verify frontend tests fail**

Run: `npm test -- src/App.test.tsx src/Keypad.test.tsx`

Expected: FAIL because schema-v3 action groups and independent Device Management drafts are unsupported.

- [x] **Step 3: Add canonical frontend types and generalize profile drafts**

Use:

```ts
export type ActionTrigger = "press" | "release" | "long_press" | "double_press";
export type TriggerSettings = { long_press_ms: number; double_press_ms: number };
export type TriggerActions = Record<ActionTrigger, ButtonAction[]>;
export type DeviceProfile = {
  schema_version: 3;
  profile: ModelLayout;
  trigger_settings: TriggerSettings;
  hardware_profiles: HardwareProfile[];
  actions: Record<string, TriggerActions>;
};
```

Replace editor-only draft lookup with `profileDraftsRef` plus `profileById(id)` and `updateProfile(id, updater)`. Feed a single `useAutosave` value containing the currently dirty profile ID/profile and flush before switching either editor profile or managed device. `applySnapshot(snapshot, true)` must merge server profiles with unsaved drafts by profile ID. Add `manualSaveProfileIds`: while a shared configuration is being edited in Device Management I/O Mapping, Key Layout, or Configuration Settings, keep its validated draft locally and suspend autosave until the user chooses Save shared configuration; choosing Duplicate sends that draft to the atomic clone-and-assign command and then discards only the old shared-profile draft. Keep the Button Behavior selector wired to `save_editor_settings`; Device Management assignment remains wired only to runtime assignment commands. Update preview fixtures and Keypad counts for all four lists.

- [x] **Step 4: Run App, autosave, preview, and Keypad tests**

Run: `npm test -- src/App.test.tsx src/App.preview.test.tsx src/useAutosave.test.tsx src/Keypad.test.tsx`

Expected: PASS with no cross-talk between editor selection and device assignment.

- [x] **Step 5: Commit the frontend data flow**

```bash
git add src/types.ts src/preview.ts src/App.tsx src/App.test.tsx src/App.preview.test.tsx src/Keypad.tsx src/Keypad.test.tsx
git commit -m "feat: manage schema v3 profile drafts independently"
```

### Task 9: Categorized Hotkey Picker And Single-Action Dialog

**Files:**

- Create: `src/HotkeyPicker.tsx`
- Create: `src/HotkeyPicker.test.tsx`
- Create: `src/ActionDialog.tsx`
- Create: `src/ActionDialog.test.tsx`
- Modify: `src/hotkey.ts`
- Modify: `src/styles/views.css`
- Modify: `src/i18n.ts`
- Modify: `src/i18n.test.ts`

- [x] **Step 1: Write failing interaction tests**

Test categories/search, multi-select, six-key lockout, removable chips, modifier-only Save, left/right disclosure, recording until all keyup events, Escape capture, intercepted-key manual fallback, local draft Cancel, default `press`/`hotkey`, trigger move, and Delete:

```tsx
test("new Action defaults to Press and commits only on Save", async () => {
  const onSave = vi.fn();
  render(<ActionDialog open mode="create" language="en-US" onSave={onSave} onCancel={vi.fn()} />);
  expect(screen.getByLabelText("Trigger")).toHaveValue("press");
  expect(screen.getByLabelText("Action type")).toHaveValue("hotkey");
  await user.click(screen.getByRole("checkbox", { name: "Command" }));
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(onSave).toHaveBeenCalledWith({ trigger: "press", action: { type: "hotkey", keys: ["cmd"] } });
});

test("recording preserves sides and commits after every key is released", async () => {
  const onChange = vi.fn();
  render(<HotkeyPicker value={[]} onChange={onChange} language="en-US" />);
  await user.click(screen.getByRole("button", { name: "Record shortcut" }));
  fireEvent.keyDown(window, { code: "MetaRight", key: "Meta" });
  fireEvent.keyDown(window, { code: "KeyK", key: "k" });
  fireEvent.keyUp(window, { code: "MetaRight", key: "Meta" });
  expect(onChange).not.toHaveBeenCalled();
  fireEvent.keyUp(window, { code: "KeyK", key: "k" });
  expect(onChange).toHaveBeenCalledWith(["right_cmd", "k"]);
});
```

- [x] **Step 2: Run the new component tests and verify the red state**

Run: `npm test -- src/HotkeyPicker.test.tsx src/ActionDialog.test.tsx`

Expected: FAIL because both modules are absent.

- [x] **Step 3: Implement the picker and dialog as controlled draft components**

`HotkeyPicker` accepts `{ value, onChange, language, error? }`, renders Common, Function Keys F1-F24, Letters, Numbers, Symbols, Navigation, and Numeric Keypad in that order, exposes each token as a checkbox, and derives selection state from canonical HID usage rather than display labels. Do not include laptop Fn. The compact modifier row is `primary`, `cmd`, `ctrl`, `alt`, `shift`; the disclosure offers eight physical modifiers and resolves each generic alias only to its corresponding left bit before checking conflicts, allowing left and right variants together. Disable an unselected ordinary key at count six while leaving selected keys removable. During recording, map `KeyboardEvent.code` on keydown, wait until every captured code receives keyup, treat Escape as an ordinary captured usage, and if the result contains more than six ordinary keys show `too_many_keys` without replacing the previous chord. Manual category selection remains available when the OS intercepts a shortcut.

`ActionDialog` accepts this contract:

```ts
export type ActionDraft = { trigger: ActionTrigger; action: ButtonAction };
type ActionDialogProps = {
  open: boolean;
  language: Language;
  mode: "create" | "edit";
  initial?: ActionDraft;
  onSave(value: ActionDraft): void;
  onDelete?(): void;
  onCancel(): void;
};
```

Initialize create mode to `{ trigger: "press", action: { type: "hotkey", keys: [] } }`. Keep all mutations in local state, validate before calling `onSave`, stop dialog close on Escape while recording, restore normal Escape-to-Cancel otherwise, and expose Delete only in edit mode. Add translated accessible names and CSS with `width: min(680px, calc(100vw - 32px))`, wrapped chips, fixed-height choices, and narrower key-grid columns below 640px.

- [x] **Step 4: Run dialog, picker, hotkey, and i18n suites**

Run: `npm test -- src/HotkeyPicker.test.tsx src/ActionDialog.test.tsx src/hotkey.test.ts src/i18n.test.ts`

Expected: PASS, including keyboard-only operation and six-key announcements.

- [x] **Step 5: Commit Action editing primitives**

```bash
git add src/HotkeyPicker.tsx src/HotkeyPicker.test.tsx src/ActionDialog.tsx src/ActionDialog.test.tsx src/hotkey.ts src/styles/views.css src/i18n.ts src/i18n.test.ts
git commit -m "feat: add compact triggered Action dialog"
```

### Task 10: Compact Trigger-Grouped Button Behavior Page

**Files:**

- Modify: `src/ActionEditor.tsx:1-320`
- Modify: `src/ActionEditor.test.tsx`
- Modify: `src/App.tsx:760-1100`
- Modify: `src/styles/views.css`
- Modify: `src/i18n.ts`

- [x] **Step 1: Write failing summary and ordering tests**

Test fixed trigger order, omitted empty sections, concise content, edit/add launch, trigger move to destination tail, deletion, and movement constrained to one group:

```tsx
test("shows only populated groups in trigger order with compact summaries", () => {
  renderEditor({
    press: [{ type: "paste", text: "Hello from Kivo" }],
    release: [{ type: "media", command: "play_pause" }],
    long_press: [],
    double_press: [{ type: "delay", duration_ms: 300 }],
  });
  expect(screen.getAllByRole("heading", { level: 3 }).map(node => node.textContent))
    .toEqual(["Press", "Release", "Double press"]);
  expect(screen.getByText("Paste - Hello from Kivo")).toBeInTheDocument();
  expect(screen.getByText("Wait - 300 ms")).toBeInTheDocument();
});

test("changing trigger appends the Action to the destination group", async () => {
  const onChange = vi.fn();
  renderEditor(groups, onChange);
  await user.click(screen.getByRole("button", { name: "Edit Paste - A" }));
  await user.selectOptions(screen.getByLabelText("Trigger"), "release");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(onChange.mock.calls[0][0].release.at(-1)).toEqual({ type: "paste", text: "A" });
});
```

- [x] **Step 2: Verify the current editor fails the new behavior**

Run: `npm test -- src/ActionEditor.test.tsx`

Expected: FAIL because the editor renders large inline Action cards and flat Actions.

- [x] **Step 3: Reduce ActionEditor to summaries plus dialog state**

Change its props to `actions: TriggerActions` and `onChange(actions: TriggerActions)`. Render groups using:

```ts
const TRIGGER_ORDER: ActionTrigger[] = ["press", "release", "long_press", "double_press"];
type EditingTarget = { trigger: ActionTrigger; index: number } | "create" | null;
```

Each row is a stable-height button with type icon, `actionSummary(action)`, and icon-only move controls with `aria-label`/`title`. On save, replace in place when the trigger is unchanged; otherwise remove the source and append to destination. Empty group keys remain arrays in memory. `App.tsx` passes `profile.actions[selectedButtonId] ?? emptyTriggerActions()` and removes the old flat list mutation.

- [x] **Step 4: Run Behavior and App regression tests**

Run: `npm test -- src/ActionEditor.test.tsx src/App.test.tsx`

Expected: PASS for create/edit/cancel/delete and independent editor selection.

- [x] **Step 5: Commit the Button Behavior redesign**

```bash
git add src/ActionEditor.tsx src/ActionEditor.test.tsx src/App.tsx src/styles/views.css src/i18n.ts
git commit -m "feat: group button Actions by trigger"
```

### Task 11: Device Management Tabs And Configuration Settings

**Files:**

- Create: `src/ConfigurationSettingsDialog.tsx`
- Create: `src/ConfigurationSettingsDialog.test.tsx`
- Modify: `src/DeviceManagement.tsx:1-930`
- Modify: `src/DeviceManagement.test.tsx`
- Modify: `src/HardwareMapping.tsx:1-720`
- Modify: `src/HardwareMapping.test.tsx`
- Modify: `src/LayoutEditor.tsx:1-142`
- Modify: `src/App.tsx:430-510,760-1140`
- Modify: `src/styles/views.css`
- Modify: `src/i18n.ts`

- [x] **Step 1: Write failing Device Management workflow tests**

Cover device selection, compact configuration assignment, preserved unique hardware mapping, ambiguous inline resolver, Overview/I/O Mapping/Key Layout tabs as page content, disconnect behavior, persistent shared warning, timing save, and atomic duplicate command:

```tsx
test("embeds I/O Mapping and Key Layout instead of opening dialogs", async () => {
  renderDeviceManagement();
  await user.click(screen.getByRole("tab", { name: "I/O Mapping" }));
  expect(screen.getByRole("tabpanel", { name: "I/O Mapping" })).toContainElement(
    screen.getByRole("heading", { name: "I/O Mapping" }),
  );
  await user.click(screen.getByRole("tab", { name: "Key Layout" }));
  expect(screen.getByRole("tabpanel", { name: "Key Layout" })).toContainElement(
    screen.getByRole("button", { name: "Add group" }),
  );
  expect(screen.queryByRole("dialog", { name: "Key Layout" })).not.toBeInTheDocument();
});

test("warns before editing a configuration shared by two devices", () => {
  renderDeviceManagement({ devices: twoDevicesUsing("phone") });
  expect(screen.getByRole("status")).toHaveTextContent("Phone is used by 2 devices");
  expect(screen.getByRole("button", { name: "Save shared configuration" })).toBeInTheDocument();
});

test("duplicate command assigns only the selected device", async () => {
  renderDeviceManagement({ devices: twoDevicesUsing("phone") });
  await user.click(screen.getByRole("button", { name: "Configuration settings" }));
  await user.click(screen.getByRole("button", { name: "Duplicate and use only for this device" }));
  expect(duplicateProfileForDevice).toHaveBeenCalledWith({
    deviceId: "device-a", sourceProfile: expect.objectContaining({ profile: expect.objectContaining({ id: "phone" }) }),
    name: "Phone copy",
  });
});
```

- [x] **Step 2: Verify Device Management tests fail**

Run: `npm test -- src/DeviceManagement.test.tsx src/ConfigurationSettingsDialog.test.tsx src/HardwareMapping.test.tsx`

Expected: FAIL because tabs/settings dialog/shared warning and embedded Layout Editor are absent.

- [x] **Step 3: Consolidate physical-device editing under Device Management**

Give `DeviceManagement` a controlled `selectedDeviceId` and `onSelectedDeviceChange`, plus `onChangeProfile`, `onSaveSharedProfile`, learning callbacks, and `onDuplicateProfileForDevice`. Track `tab: "overview" | "io" | "layout"`. The `Use configuration` select resolves compatible hardware with this exact order: preserve the current mapping if it belongs to the selected profile and matches the board; else assign the only compatible mapping; else keep the stored assignment unchanged, set tab to `io`, and render a required inline hardware select plus Apply.

Render `HardwareMapping` directly in the I/O tab. Convert `LayoutEditor` from a `<dialog>` with Apply/Cancel draft state into an embedded controlled editor with `layout`/`onChange`; profile autosave becomes its persistence path. Disable learning and runtime controls when offline, while keeping all fields editable.

Implement the settings dialog contract:

```ts
type ConfigurationSettingsDialogProps = {
  open: boolean;
  profile: DeviceProfile;
  sharedDeviceCount: number;
  onSave(settings: TriggerSettings): void;
  onDuplicate(name: string): Promise<void>;
  onCancel(): void;
};
```

Use number inputs with `min/max` values `100/5000` and `100/1000`; validate integer values locally. Save updates only `trigger_settings`, labeling the command `Save shared configuration` when `sharedDeviceCount > 1`. Duplicate submits the complete current profile draft to the backend transaction and closes only after success. Keep the dialog limited to timing and duplicate controls. Render the same persistent shared warning above both the I/O Mapping and Key Layout tab content; its Save shared configuration command explicitly flushes the suspended draft, while Duplicate and use only for this device preserves the source and assigns the validated draft clone.

- [x] **Step 4: Run Device Management, mapping, layout, and App tests**

Run: `npm test -- src/DeviceManagement.test.tsx src/ConfigurationSettingsDialog.test.tsx src/HardwareMapping.test.tsx src/App.test.tsx`

Expected: PASS for assignment resolution, shared ownership, offline editing, learning, and embedded pages.

- [x] **Step 5: Commit the device-centered workspace**

```bash
git add src/ConfigurationSettingsDialog.tsx src/ConfigurationSettingsDialog.test.tsx src/DeviceManagement.tsx src/DeviceManagement.test.tsx src/HardwareMapping.tsx src/HardwareMapping.test.tsx src/LayoutEditor.tsx src/App.tsx src/styles/views.css src/i18n.ts
git commit -m "feat: consolidate configuration under device management"
```

### Task 12: Navigation, Configuration Files, Responsive Polish, And Previews

**Files:**

- Modify: `src/App.tsx:1-70,760-1140`
- Modify: `src/App.test.tsx`
- Modify: `src/App.preview.test.tsx`
- Modify: `src/preview.ts`
- Modify: `src/styles/app.css`
- Modify: `src/styles/views.css`
- Modify: `src/i18n.ts`
- Modify: `src/i18n.test.ts`

- [x] **Step 1: Write failing navigation and accessibility tests**

Assert the sidebar has exactly four destinations, Configuration Files has no editor selector, Button Behavior has its own selector, icon controls have names/tooltips, and narrow layouts do not create modal I/O/Layout surfaces:

```tsx
test("shows four destinations and keeps editor selection on Button Behavior", async () => {
  render(<App />);
  expect(screen.getAllByRole("button", { name: /Home|Device Management|Button Behavior|Configuration Files/ })).toHaveLength(4);
  expect(screen.queryByRole("button", { name: "Hardware Mapping" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Key Layout" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Configuration Files" }));
  expect(screen.queryByLabelText("Configuration to edit")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Button Behavior" }));
  expect(screen.getByLabelText("Configuration to edit")).toBeInTheDocument();
});
```

- [x] **Step 2: Verify App and i18n tests fail**

Run: `npm test -- src/App.test.tsx src/App.preview.test.tsx src/i18n.test.ts`

Expected: FAIL because old hardware/layout navigation and the Configuration Files editor selector remain.

- [x] **Step 3: Finish navigation and responsive states**

Reduce `View` to `"home" | "devices" | "behavior" | "data"`; label `data` as Configuration Files. Remove Hardware Mapping and Key Layout sidebar buttons, render the configuration selector in the Button Behavior heading, and remove the `model-picker` editor control from Configuration Files. Render a configuration list with device-usage counts and per-item Export, Duplicate, and Delete commands; retain Create, Import, Backup, and Restore. Duplicate opens the existing clone form with the row's profile as source and never changes a device assignment.

Add preview fixtures for: Button Behavior grouped summary, Action dialog, Device Management I/O tab, Layout tab, shared warning, and Configuration Settings dialog. In CSS, stack the device list above its tab workspace below 760px, keep I/O/Layout in normal page scroll, wrap toolbars without changing icon-button dimensions, and ensure dialogs use `max-height: calc(100vh - 32px); overflow: auto`. Add `title` to every icon-only button and visible focus styles to tabs, checkboxes, chips, and disclosures. Keep letter spacing at `0`.

- [x] **Step 4: Run all frontend tests and production build**

Run: `npm test`

Expected: PASS.

Run: `npm run build`

Expected: PASS with no TypeScript or Vite errors.

- [x] **Step 5: Commit the finished information architecture**

```bash
git add src/App.tsx src/App.test.tsx src/App.preview.test.tsx src/preview.ts src/styles/app.css src/styles/views.css src/i18n.ts src/i18n.test.ts
git commit -m "feat: finish device workspace navigation"
```

### Task 13: Full Verification And Hardware Acceptance

**Files:**

- Modify only if verification exposes a defect: files owned by Tasks 1-12
- Add generated evidence: `docs/verification/screenshots/action-triggers-*.png`

- [x] **Step 1: Run formatting and whitespace checks**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml`

Expected: Rust sources are formatted successfully.

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Expected: PASS.

Run: `git diff --check`

Expected: no output and exit 0.

- [ ] **Step 2: Run every automated gate**

Run: `npm test`

Expected: PASS.

Run: `npm run build`

Expected: PASS.

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: PASS with no warnings.

Run: `uv run pio test -e native`

Expected: PASS.

Run: `make build-esp32s3 && make build-rp2040`

Expected: both managed firmware builds succeed.

Run: `make test`

Expected: the aggregate release, Python, native firmware, Rust, Clippy, frontend, and build gate passes.

Note: The repository-local `.venv/bin/pio` native test and both firmware builds passed. The `make`/`uv run pio` wrappers fail before running because the user-level `/Users/leo/.config/uv/uv.toml` is invalid TOML (`line 4: -e [[index]]`); it was not changed.

- [x] **Step 3: Start the app and capture Playwright evidence**

Run: `npm run dev -- --host 127.0.0.1`

Expected: Vite prints a local URL and stays running.

Using the Playwright skill/CLI, capture these exact files:

```text
docs/verification/screenshots/action-triggers-behavior-1120x760.png
docs/verification/screenshots/action-triggers-action-dialog-1120x760.png
docs/verification/screenshots/action-triggers-device-io-1120x760.png
docs/verification/screenshots/action-triggers-device-layout-1120x760.png
docs/verification/screenshots/action-triggers-shared-warning-1120x760.png
docs/verification/screenshots/action-triggers-settings-1120x760.png
docs/verification/screenshots/action-triggers-behavior-390x844.png
docs/verification/screenshots/action-triggers-action-dialog-390x844.png
docs/verification/screenshots/action-triggers-device-io-390x844.png
docs/verification/screenshots/action-triggers-device-layout-390x844.png
docs/verification/screenshots/action-triggers-shared-warning-390x844.png
docs/verification/screenshots/action-triggers-settings-390x844.png
```

Inspect each image for clipped text, overlap, unintended nested cards, inaccessible offscreen controls, and excess Action-card spacing; repeat until all images pass.

- [ ] **Step 4: Run physical RP2040 and ESP32-S3 smoke checks**

Flash each connected board with an explicit build ID:

```bash
make upload-rp2040 BUILD_ID="0.1.0+trigger-v6"
make upload-esp32s3 BUILD_ID="0.1.0+trigger-v6"
```

On each board verify: handset pickup runs Press; hangup runs Release; holding beyond 500 ms runs Long Press once; Down-Up-Down within 300 ms runs Double Press after the second Press; a six-key chord produces one pressed and one release report; a right Command/right Alt chord preserves right-side modifier bits; modifier-only chord executes; input scanning continues during a 3000 ms Delay. Then connect protocol-5 firmware and verify Press plus one ordinary key remains functional while v6-only profiles show firmware update required.

Note: Physical flashing and board-level acceptance were not run in this session because no RP2040 or ESP32-S3 device was connected.

- [x] **Step 5: Review final scope and commit verification evidence**

Run: `git status --short`

Expected: only intended implementation files and screenshot evidence are modified/untracked.

Run: `git diff --stat`

Expected: changes are limited to Action triggers, protocol/HID runtime, Device Management, Button Behavior, configuration files, tests, and verification images.

```bash
git add docs/verification/screenshots/action-triggers-*.png
git commit -m "test: verify triggered Action workflows"
```
