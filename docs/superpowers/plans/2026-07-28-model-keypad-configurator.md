# Model Keypad Configurator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the GPIO list editor with a model-aware keypad wireframe that separates model IO wiring from reusable paste and hotkey actions.

**Architecture:** Development-time JSON files define normalized keypad groups for each model. Tauri loads those layouts plus user YAML containing the manually selected model, per-model GPIO-to-button maps, and global button actions; ESP32 continues to emit GPIO presses and executes compact `PASTE`, `HOTKEY`, or `SKIP` replies over USB serial/HID.

**Tech Stack:** React 19, TypeScript 7, Vite 8, Vitest/Testing Library, Tauri 2, Rust 2024, serde JSON/YAML, ESP32-S3 Arduino USB HID, PlatformIO/Unity.

## Global Constraints

- Prefix every shell command with `rtk`.
- Add no runtime or development dependency.
- The application must not upload photographs or perform runtime image recognition.
- Model selection is manual; no firmware model handshake is added.
- A layout is ordered groups with equal-size buttons per group; do not store free coordinates, rotation, or per-button size.
- `BACK/OUT` is one button with ID `BACK_OUT`.
- IO maps are isolated by model; actions are global by semantic button ID.
- First release supports press only and exactly two actions: pasted text and one shortcut containing zero or more modifiers plus one non-modifier key.
- ESP32 executes both paste and shortcut HID reports.
- Existing `buttons: GPIO -> text` YAML must remain readable and must not lose unresolved entries.
- Persist every changed file via temporary write plus rename before replacing runtime state.

---

## File Structure

- Create `models/red-phone-v1.json`: generated normalized layout for the first telephone model.
- Create `src-tauri/src/storage.rs`: shared atomic text-file write used by config and model files.
- Create `src-tauri/src/model.rs`: model layout types, validation, seeding, loading, and saving.
- Modify `src-tauri/src/config.rs`: model selection, per-model IO maps, global actions, validation, migration, and runtime resolution.
- Modify `src-tauri/src/protocol.rs`: convert resolved actions into `PASTE`, `HOTKEY`, or `SKIP` replies.
- Modify `src-tauri/src/device.rs`: emit structured GPIO presses and suppress actions during one-shot IO capture.
- Modify `src-tauri/src/lib.rs`: application snapshot/state and Tauri save/capture commands.
- Modify `lib/gpio_trigger/src/TriggerProtocol.{h,cpp}`: parse `HOTKEY` replies.
- Modify `lib/gpio_trigger/src/GpioTriggerController.{h,cpp}`: generalize a matched response from paste-only to execute-or-clear.
- Modify `src/main.cpp`: send raw HID reports for shortcuts.
- Modify `test/test_gpio_trigger/test_main.cpp`: firmware protocol and pending-event checks.
- Modify `src/types.ts`: shared frontend snapshot, model, action, and mode types.
- Create `src/Keypad.tsx`: grouped wireframe, hover summaries, selection, and anchored position calculation.
- Create `src/ButtonPopover.tsx`: IO and behavior forms.
- Create `src/hotkey.ts`: browser keyboard-event normalization.
- Create `src/LayoutEditor.tsx`: developer model-layout editing dialog.
- Modify `src/App.tsx`: state orchestration, mode/model selection, capture, save, and activity.
- Modify `src/App.css`: compact desktop layout, wireframe, tooltip, popover, and dialog styles.
- Modify `src/App.test.tsx`: end-to-end component behavior.
- Create `src/hotkey.test.ts`: shortcut normalization checks.

---

### Task 1: Add the ESP32 HOTKEY response

**Files:**
- Modify: `lib/gpio_trigger/src/TriggerProtocol.h`
- Modify: `lib/gpio_trigger/src/TriggerProtocol.cpp`
- Modify: `lib/gpio_trigger/src/GpioTriggerController.h`
- Modify: `lib/gpio_trigger/src/GpioTriggerController.cpp`
- Modify: `src/main.cpp`
- Test: `test/test_gpio_trigger/test_main.cpp`

**Interfaces:**
- Consumes: existing `PRESS <event_id> <gpio>` request and current pending-event guard.
- Produces: `HelperResponse { kind, eventId, modifierMask, keycode }`, supporting `HOTKEY <event_id> <modifier_mask> <hid_keycode>`.

- [ ] **Step 1: Write failing parser and execution-gate tests**

Add these assertions to `test/test_gpio_trigger/test_main.cpp`:

```cpp
void test_parses_hotkey_response() {
  const auto response = parseHelperResponse("HOTKEY 42 10 14\n");
  TEST_ASSERT_TRUE(response.has_value());
  TEST_ASSERT_EQUAL(HelperResponseKind::Hotkey, response->kind);
  TEST_ASSERT_EQUAL_UINT32(42, response->eventId);
  TEST_ASSERT_EQUAL_UINT8(10, response->modifierMask);
  TEST_ASSERT_EQUAL_UINT8(14, response->keycode);
}

void test_rejects_malformed_hotkey_response() {
  TEST_ASSERT_FALSE(parseHelperResponse("HOTKEY 42 10\n").has_value());
  TEST_ASSERT_FALSE(parseHelperResponse("HOTKEY 42 256 14\n").has_value());
  TEST_ASSERT_FALSE(parseHelperResponse("HOTKEY 42 10 0\n").has_value());
  TEST_ASSERT_FALSE(parseHelperResponse("HOTKEY 42 10 165\n").has_value());
}

void test_matching_hotkey_response_requests_execution() {
  GpioTriggerController controller(0);
  controller.updatePin(6, false, 0);
  const auto event = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(event.has_value());
  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.handleResponse(event->id, true));
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}
```

Register all three with `RUN_TEST(...)` in `main()` and rename existing `ResponseAction::Paste` assertions to `ResponseAction::Execute`.

- [ ] **Step 2: Run the native firmware test and confirm RED**

Run: `rtk uv run pio test -e native`

Expected: compile failure because `HelperResponseKind::Hotkey` and `ResponseAction::Execute` do not exist.

- [ ] **Step 3: Generalize the pending-response gate**

In `GpioTriggerController.h`, replace the result enum and keep the boolean input as the minimal execute/clear decision:

```cpp
enum class ResponseAction {
  Ignored,
  Cleared,
  Execute,
};

ResponseAction handleResponse(std::uint32_t eventId, bool execute);
```

In `GpioTriggerController.cpp`, replace `handleResponse` with:

```cpp
ResponseAction GpioTriggerController::handleResponse(std::uint32_t eventId,
                                                     bool execute) {
  if (!pendingEvent_.has_value() || pendingEvent_->id != eventId) {
    return ResponseAction::Ignored;
  }
  pendingEvent_.reset();
  return execute ? ResponseAction::Execute : ResponseAction::Cleared;
}
```

- [ ] **Step 4: Parse the compact HOTKEY command**

Replace the response declarations in `TriggerProtocol.h` with:

```cpp
enum class HelperResponseKind { Paste, Hotkey, Skip };

struct HelperResponse {
  HelperResponseKind kind;
  std::uint32_t eventId;
  std::uint8_t modifierMask = 0;
  std::uint8_t keycode = 0;
};
```

In `TriggerProtocol.cpp`, add a decimal parser and parse exact token counts:

```cpp
namespace {
std::optional<std::uint32_t> parseNumber(std::string_view value) {
  if (value.empty()) return std::nullopt;
  std::uint32_t result = 0;
  for (const char character : value) {
    if (character < '0' || character > '9') return std::nullopt;
    const auto digit = static_cast<std::uint32_t>(character - '0');
    if (result > (std::numeric_limits<std::uint32_t>::max() - digit) / 10U)
      return std::nullopt;
    result = result * 10U + digit;
  }
  return result;
}

std::optional<std::string_view> takeToken(std::string_view &line) {
  while (!line.empty() && line.front() == ' ') line.remove_prefix(1);
  if (line.empty()) return std::nullopt;
  const auto separator = line.find(' ');
  const auto token = line.substr(0, separator);
  line = separator == std::string_view::npos ? std::string_view{} : line.substr(separator + 1);
  return token;
}
}  // namespace
```

Implement `parseHelperResponse` so `PASTE` and `SKIP` accept one numeric argument, while `HOTKEY` accepts event ID, mask `0..255`, and raw HID keycode `1..164`; reject remaining tokens. Return zero mask/keycode for non-hotkey commands.

```cpp
std::optional<HelperResponse> parseHelperResponse(std::string_view line) {
  while (!line.empty() && (line.back() == '\n' || line.back() == '\r')) {
    line.remove_suffix(1);
  }
  const auto kindToken = takeToken(line);
  const auto eventToken = takeToken(line);
  if (!kindToken.has_value() || !eventToken.has_value()) return std::nullopt;
  const auto eventId = parseNumber(*eventToken);
  if (!eventId.has_value()) return std::nullopt;

  if (*kindToken == "PASTE" || *kindToken == "SKIP") {
    if (takeToken(line).has_value()) return std::nullopt;
    return HelperResponse{
        *kindToken == "PASTE" ? HelperResponseKind::Paste
                              : HelperResponseKind::Skip,
        *eventId,
    };
  }
  if (*kindToken != "HOTKEY") return std::nullopt;

  const auto maskToken = takeToken(line);
  const auto keyToken = takeToken(line);
  if (!maskToken.has_value() || !keyToken.has_value() ||
      takeToken(line).has_value()) {
    return std::nullopt;
  }
  const auto mask = parseNumber(*maskToken);
  const auto keycode = parseNumber(*keyToken);
  if (!mask.has_value() || *mask > 255 || !keycode.has_value() ||
      *keycode == 0 || *keycode > 164) {
    return std::nullopt;
  }
  return HelperResponse{HelperResponseKind::Hotkey, *eventId,
                        static_cast<std::uint8_t>(*mask),
                        static_cast<std::uint8_t>(*keycode)};
}
```

- [ ] **Step 5: Execute a raw HID keyboard report**

In `src/main.cpp`, add:

```cpp
void sendHotkey(std::uint8_t modifierMask, std::uint8_t keycode) {
  KeyReport report{};
  report.modifiers = modifierMask;
  report.keys[0] = keycode;
  keyboard.sendReport(&report);
  delay(10);
  keyboard.releaseAll();
}
```

Replace the response action block in `handleResponseLine()` with:

```cpp
const bool execute = response->kind != HelperResponseKind::Skip;
if (controller.handleResponse(response->eventId, execute) !=
    ResponseAction::Execute) {
  return;
}
if (response->kind == HelperResponseKind::Paste) {
  pasteClipboard();
} else if (response->kind == HelperResponseKind::Hotkey) {
  sendHotkey(response->modifierMask, response->keycode);
}
```

- [ ] **Step 6: Verify firmware tests and production compilation**

Run: `rtk uv run pio test -e native`

Expected: all Unity tests pass.

Run: `rtk uv run pio run -e esp32s3`

Expected: firmware builds without errors.

- [ ] **Step 7: Commit the firmware protocol**

```bash
rtk git add lib/gpio_trigger/src src/main.cpp test/test_gpio_trigger/test_main.cpp
rtk git commit -m "feat: execute configured HID shortcuts"
```

---

### Task 2: Load and validate model layout files

**Files:**
- Create: `models/red-phone-v1.json`
- Create: `src-tauri/src/storage.rs`
- Create: `src-tauri/src/model.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: JSON generated during development.
- Produces: `ModelLayout`, `ButtonGroup`, `ButtonDefinition`, `load_all(&Path)`, and `save(&Path, &ModelLayout)`.

- [ ] **Step 1: Add failing model validation tests**

Create `src-tauri/src/model.rs` with type declarations and tests first:

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ButtonDefinition { pub id: String, pub label: String }

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ButtonGroup {
    pub id: String,
    pub columns: usize,
    pub buttons: Vec<ButtonDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ModelLayout {
    pub id: String,
    pub name: String,
    pub groups: Vec<ButtonGroup>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_red_phone_has_one_back_out_button() {
        let model: ModelLayout = serde_json::from_str(include_str!(
            "../../models/red-phone-v1.json"
        )).unwrap();
        let ids = model.groups.iter().flat_map(|group| &group.buttons)
            .map(|button| button.id.as_str()).collect::<Vec<_>>();
        assert!(ids.contains(&"BACK_OUT"));
        assert!(!ids.contains(&"BACK"));
        assert!(!ids.contains(&"OUT"));
    }

    #[test]
    fn rejects_duplicate_button_ids() {
        let model = ModelLayout {
            id: "test".into(), name: "Test".into(),
            groups: vec![ButtonGroup {
                id: "row".into(), columns: 2,
                buttons: vec![
                    ButtonDefinition { id: "A".into(), label: "A".into() },
                    ButtonDefinition { id: "A".into(), label: "Again".into() },
                ],
            }],
        };
        assert!(model.validate().unwrap_err().contains("duplicate button A"));
    }
}
```

Also add `mod model;` to `src-tauri/src/lib.rs` in this step so the new unit tests are compiled.

- [ ] **Step 2: Run the focused Rust test and confirm RED**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml model::tests -- --nocapture`

Expected: compile failure because the module, layout file, and `validate` do not yet exist.

- [ ] **Step 3: Add the normalized red-phone layout**

Create `models/red-phone-v1.json`:

```json
{
  "id": "red-phone-v1",
  "name": "Red Phone v1",
  "groups": [
    {
      "id": "top",
      "columns": 4,
      "buttons": [
        { "id": "UP", "label": "UP" },
        { "id": "DOWN", "label": "DOWN" },
        { "id": "BACK_OUT", "label": "BACK/OUT" },
        { "id": "DEL", "label": "DEL" }
      ]
    },
    {
      "id": "digits",
      "columns": 3,
      "buttons": [
        { "id": "DIGIT_1", "label": "1" },
        { "id": "DIGIT_2", "label": "2" },
        { "id": "DIGIT_3", "label": "3" },
        { "id": "DIGIT_4", "label": "4" },
        { "id": "DIGIT_5", "label": "5" },
        { "id": "DIGIT_6", "label": "6" },
        { "id": "DIGIT_7", "label": "7" },
        { "id": "DIGIT_8", "label": "8" },
        { "id": "DIGIT_9", "label": "9" },
        { "id": "STAR", "label": "*" },
        { "id": "DIGIT_0", "label": "0" },
        { "id": "HASH", "label": "#" }
      ]
    },
    {
      "id": "bottom",
      "columns": 5,
      "buttons": [
        { "id": "R", "label": "R" },
        { "id": "VOL", "label": "VOL" },
        { "id": "FL_SET", "label": "FL/SET" },
        { "id": "RD_PA", "label": "RD/PA" },
        { "id": "SPEAKER", "label": "SPK" }
      ]
    }
  ]
}
```

- [ ] **Step 4: Implement model validation and persistence**

Implement `ModelLayout::validate()` with these exact checks:

```rust
pub fn validate(&self) -> Result<(), String> {
    if self.id.trim().is_empty() || self.name.trim().is_empty() {
        return Err("model id and name are required".into());
    }
    let mut groups = std::collections::BTreeSet::new();
    let mut buttons = std::collections::BTreeSet::new();
    for group in &self.groups {
        if group.id.trim().is_empty() || !groups.insert(group.id.as_str()) {
            return Err(format!("invalid or duplicate group {}", group.id));
        }
        if group.columns == 0 || group.buttons.is_empty() {
            return Err(format!("group {} must have columns and buttons", group.id));
        }
        for button in &group.buttons {
            if button.id.trim().is_empty() || button.label.trim().is_empty() {
                return Err("button id and label are required".into());
            }
            if !buttons.insert(button.id.as_str()) {
                return Err(format!("duplicate button {}", button.id));
            }
        }
    }
    Ok(())
}
```

Create `storage.rs` with `pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), String>` by moving the existing temporary-file, `sync_all`, rename, and cleanup logic from `config.rs` unchanged.

Implement in `model.rs`:

```rust
pub fn load_all(directory: &Path) -> (Vec<ModelLayout>, Vec<String>);
pub fn save(directory: &Path, layout: &ModelLayout) -> Result<(), String>;
pub fn seed_default(directory: &Path) -> Result<(), String>;
```

`seed_default` writes embedded `models/red-phone-v1.json` only when absent. `load_all` sorts valid layouts by name and collects one error string per invalid JSON file without hiding other valid models. `save` validates before atomically writing pretty JSON to `<directory>/<id>.json`.

Declare `mod model; mod storage;` in `src-tauri/src/lib.rs`.

- [ ] **Step 5: Verify the model tests**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml model::tests -- --nocapture`

Expected: model tests pass, including `BACK_OUT` validation.

- [ ] **Step 6: Commit model layout support**

```bash
rtk git add models src-tauri/src/model.rs src-tauri/src/storage.rs src-tauri/src/lib.rs
rtk git commit -m "feat: add model keypad layouts"
```

---

### Task 3: Replace GPIO text mappings with typed user configuration

**Files:**
- Modify: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/protocol.rs`

**Interfaces:**
- Consumes: `ModelLayout` catalog and legacy `buttons` YAML.
- Produces: `MappingConfig { active_model, io_maps, actions }`, `ButtonAction`, `resolved_action(gpio)`, and migration-safe save.

- [ ] **Step 1: Replace config tests with failing model-aware cases**

Add focused tests for this exact configuration:

```rust
#[test]
fn resolves_model_gpio_to_global_action() {
    let config = MappingConfig {
        active_model: "red-phone-v1".into(),
        io_maps: BTreeMap::from([(
            "red-phone-v1".into(), BTreeMap::from([(6, "DIGIT_2".into())])
        )]),
        actions: BTreeMap::from([(
            "DIGIT_2".into(), ButtonAction::Hotkey {
                keys: vec!["cmd".into(), "shift".into(), "k".into()],
            }
        )]),
        legacy_buttons: BTreeMap::new(),
    };
    assert!(matches!(config.resolved_action(6), Some(ButtonAction::Hotkey { .. })));
}

#[test]
fn migrates_only_legacy_gpio_entries_known_to_active_model() {
    let mut config = MappingConfig {
        active_model: "red-phone-v1".into(),
        io_maps: BTreeMap::from([(
            "red-phone-v1".into(), BTreeMap::from([(6, "DIGIT_2".into())])
        )]),
        actions: BTreeMap::new(),
        legacy_buttons: BTreeMap::from([(6, "hello".into()), (7, "keep".into())]),
    };
    config.migrate_legacy();
    assert_eq!(config.actions["DIGIT_2"], ButtonAction::Paste { text: "hello".into() });
    assert_eq!(config.legacy_buttons, BTreeMap::from([(7, "keep".into())]));
}
```

Also test rejection of unsupported GPIO, the same button assigned to two GPIOs in one model, empty paste text, and malformed hotkeys.

- [ ] **Step 2: Run config tests and confirm RED**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml config::tests -- --nocapture`

Expected: compile failures for `ButtonAction`, `io_maps`, and `resolved_action`.

- [ ] **Step 3: Implement the tagged action and config schema**

Use these exact public types:

```rust
pub type IoMaps = BTreeMap<String, BTreeMap<u8, String>>;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ButtonAction {
    Paste { text: String },
    Hotkey { keys: Vec<String> },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct MappingConfig {
    pub active_model: String,
    pub io_maps: IoMaps,
    pub actions: BTreeMap<String, ButtonAction>,
    #[serde(skip)]
    pub legacy_buttons: BTreeMap<u8, String>,
}

#[derive(Default, Deserialize, Serialize)]
struct ConfigDocument {
    #[serde(default)] active_model: String,
    #[serde(default)] io_maps: IoMaps,
    #[serde(default)] actions: BTreeMap<String, ButtonAction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    buttons: BTreeMap<u8, String>,
}
```

Implement:

```rust
pub fn resolved_button(&self, gpio: u8) -> Option<&str>;
pub fn resolved_action(&self, gpio: u8) -> Option<ButtonAction>;
pub fn migrate_legacy(&mut self);
pub fn validate(&self, models: &[ModelLayout]) -> Result<(), String>;
```

Validation allows actions for IDs absent from the active model, because global actions may belong to another model. It validates each model IO map against that model's button IDs and rejects a button value repeated under two GPIO keys.

- [ ] **Step 4: Preserve legacy runtime behavior and atomic saving**

`resolved_action` first resolves the active model's GPIO to a global action. If no new action exists, return `legacy_buttons[gpio]` as `ButtonAction::Paste`. `save` clones the config, runs `migrate_legacy`, validates it, serializes unresolved legacy entries back under `buttons`, and calls `storage::atomic_write`.

- [ ] **Step 5: Generate all three protocol replies**

In `src-tauri/src/protocol.rs`, add:

```rust
pub fn encode_hotkey(keys: &[String]) -> Result<(u8, u8), String>;
pub fn reply(
    press: Press,
    action: Option<ButtonAction>,
    copy: impl FnOnce(&str) -> Result<(), String>,
) -> Reply;
```

`encode_hotkey` maps `ctrl=0x01`, `shift=0x02`, `alt/option=0x04`, and `cmd=0x08`; letters use HID codes `a=0x04` through `z=0x1d`, digits use `1=0x1e` through `9=0x26` and `0=0x27`, and named keys use USB HID codes for Enter, Escape, Backspace, Tab, Space, arrows, Delete, Home, End, PageUp, and PageDown. It rejects duplicate modifiers, zero or multiple ordinary keys, and unknown names.

Expected reply examples:

```rust
assert_eq!(reply(Press { event_id: 12, gpio: 6 }, hotkey, |_| Ok(())).line,
           "HOTKEY 12 10 14\n");
assert_eq!(reply(Press { event_id: 12, gpio: 6 }, None, |_| Ok(())).line,
           "SKIP 12\n");
```

- [ ] **Step 6: Verify config and protocol tests**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml config::tests -- --nocapture`

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml protocol::tests -- --nocapture`

Expected: all config and protocol tests pass.

- [ ] **Step 7: Commit typed model configuration**

```bash
rtk git add src-tauri/src/config.rs src-tauri/src/protocol.rs
rtk git commit -m "feat: separate IO maps from button actions"
```

---

### Task 4: Expose model workspace and one-shot IO capture through Tauri

**Files:**
- Modify: `src-tauri/src/device.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: unit tests in both files

**Interfaces:**
- Consumes: model catalog and typed `MappingConfig` from Tasks 2-3.
- Produces: `get_snapshot`, `save_workspace`, `set_io_capture`, and runtime events with optional `gpio`.

- [ ] **Step 1: Write failing backend tests**

Add a device-level pure decision helper and tests:

```rust
#[test]
fn capture_skips_action_and_clears_itself() {
    let capture = AtomicBool::new(true);
    let action = action_for_press(&capture, Some(ButtonAction::Paste { text: "x".into() }));
    assert_eq!(action, None);
    assert!(!capture.load(Ordering::Relaxed));
}
```

Add a `lib.rs` save test that changes `active_model`, an IO map, an action, and one layout label, then asserts both YAML and model JSON were written before the in-memory state changed.

- [ ] **Step 2: Run Rust tests and confirm RED**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml --lib -- --nocapture`

Expected: compile failures for the new snapshot fields and capture helper.

- [ ] **Step 3: Extend runtime state and snapshot**

Use these state fields:

```rust
struct AppState {
    mappings: Arc<RwLock<MappingConfig>>,
    models: Arc<RwLock<Vec<ModelLayout>>>,
    model_directory: PathBuf,
    config_path: PathBuf,
    connection: Arc<RwLock<ConnectionStatus>>,
    config_error: Mutex<Option<String>>,
    capture_next_gpio: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSnapshot {
    models: Vec<ModelLayout>,
    active_model: String,
    io_maps: IoMaps,
    actions: BTreeMap<String, ButtonAction>,
    supported_gpios: Vec<u8>,
    config_path: String,
    connection: ConnectionStatus,
    config_error: Option<String>,
}
```

During setup, create `<app-config>/models`, seed the default model, load all valid models, load user config, and select the first model only when `active_model` is empty. Do not infer a model from the USB device.

- [ ] **Step 4: Add save and capture commands**

Expose:

```rust
#[tauri::command]
fn save_workspace(
    state: tauri::State<'_, AppState>,
    active_model: String,
    io_maps: IoMaps,
    actions: BTreeMap<String, ButtonAction>,
    models: Vec<ModelLayout>,
) -> Result<AppSnapshot, String>;

#[tauri::command]
fn set_io_capture(state: tauri::State<'_, AppState>, enabled: bool) {
    state.capture_next_gpio.store(enabled, Ordering::Relaxed);
}
```

`save_workspace` validates every model and the complete user config first, atomically writes each changed model and then YAML, and only then replaces in-memory models/mappings. Register both commands and remove `save_mappings`.

- [ ] **Step 5: Emit GPIO and suppress capture presses**

Extend `RuntimeEvent` with `pub gpio: Option<u8>`. For parsed presses, emit an event carrying `Some(press.gpio)` before replying. Add:

```rust
fn action_for_press(
    capture_next_gpio: &AtomicBool,
    configured: Option<ButtonAction>,
) -> Option<ButtonAction> {
    if capture_next_gpio.swap(false, Ordering::Relaxed) { None } else { configured }
}
```

When it returns `None` during capture, return `SKIP`; otherwise call `protocol::reply` with the configured action. Status and error events carry `gpio: None`.

- [ ] **Step 6: Verify the entire Rust crate**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml`

Expected: all Rust tests pass.

- [ ] **Step 7: Commit the Tauri workspace API**

```bash
rtk git add src-tauri/src/device.rs src-tauri/src/lib.rs
rtk git commit -m "feat: expose model workspace and GPIO capture"
```

---

### Task 5: Render the selected model as a grouped wireframe

**Files:**
- Modify: `src/types.ts`
- Create: `src/Keypad.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.css`
- Test: `src/App.test.tsx`

**Interfaces:**
- Consumes: `AppSnapshot.models`, `activeModel`, `ioMaps`, and `actions`.
- Produces: model selector, mode switch, grouped keypad, hover summary, and selected-button anchor.

- [ ] **Step 1: Replace the frontend fixture and add a failing wireframe test**

Define the test snapshot with `red-phone-v1`, top/digits/bottom groups, `ioMaps: { "red-phone-v1": { 6: "DIGIT_2" } }`, and a paste action. Add:

```tsx
test("renders the selected model as normalized groups", async () => {
  render(<App />);
  expect(await screen.findByRole("button", { name: "Configure BACK/OUT" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Configure BACK" })).not.toBeInTheDocument();
  expect(screen.getByTestId("group-top")).toHaveStyle({
    gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
  });
  expect(screen.getByTestId("group-digits")).toHaveStyle({
    gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
  });
});
```

- [ ] **Step 2: Run the frontend test and confirm RED**

Run: `rtk npm test -- src/App.test.tsx`

Expected: failure because the GPIO list is still rendered.

- [ ] **Step 3: Define frontend contracts**

Replace the old `buttons` snapshot types in `src/types.ts` with exact Rust mirrors:

```ts
export type ConfigMode = "io" | "behavior";
export type ButtonAction =
  | { type: "paste"; text: string }
  | { type: "hotkey"; keys: string[] };
export interface ModelButton { id: string; label: string }
export interface ButtonGroup { id: string; columns: number; buttons: ModelButton[] }
export interface ModelLayout { id: string; name: string; groups: ButtonGroup[] }
export interface AppSnapshot {
  models: ModelLayout[];
  activeModel: string;
  ioMaps: Record<string, Record<number, string>>;
  actions: Record<string, ButtonAction>;
  supportedGpios: number[];
  configPath: string;
  connection: ConnectionStatus;
  configError: string | null;
}
```

Add `gpio: number | null` to `RuntimeEvent`.

- [ ] **Step 4: Build the grouped keypad component**

Create `Keypad.tsx` with:

```ts
interface KeypadProps {
  layout: ModelLayout;
  mode: ConfigMode;
  ioMap: Record<number, string>;
  actions: Record<string, ButtonAction>;
  selectedButtonId: string | null;
  onSelect(buttonId: string, anchor: DOMRect): void;
}

export function gpioForButton(ioMap: Record<number, string>, buttonId: string) {
  const entry = Object.entries(ioMap).find(([, value]) => value === buttonId);
  return entry ? Number(entry[0]) : null;
}
```

Render each group as:

```tsx
<div
  className="key-group"
  data-testid={`group-${group.id}`}
  style={{ gridTemplateColumns: `repeat(${group.columns}, minmax(0, 1fr))` }}
>
  {group.buttons.map((button) => (
    <div className="key-shell" key={button.id}>
      <button
        className={selectedButtonId === button.id ? "key is-selected" : "key"}
        aria-label={`Configure ${button.label}`}
        onClick={(event) => onSelect(button.id, event.currentTarget.getBoundingClientRect())}
      >{button.label}</button>
      <span className="key-summary" role="tooltip">{summary}</span>
    </div>
  ))}
</div>
```

Summary is `GPIO N`/`Unmapped` in IO mode and a truncated paste text/normalized shortcut/`No action` in behavior mode.

- [ ] **Step 5: Replace App GPIO-list state with workspace state**

Store `models`, `activeModel`, `ioMaps`, `actions`, saved copies, `mode`, `selectedButtonId`, and anchor rect. Render a native `<select aria-label="Device model">`, a two-button segmented control, and `Keypad` for the selected layout. Disable model selection while dirty. If `activeModel` is absent from the valid catalog, keep the selector enabled, render a disabled `Missing: <id>` option plus every valid model, show `configError`, and render no keypad until the user chooses a valid model.

Keep the connection header and activity log unchanged. The current global Command+S handler invokes the new `save_workspace` payload.

- [ ] **Step 6: Style the normalized workspace**

Replace list/editor rules in `App.css` with a centered `.keypad` and independent `.key-group` grids. Use rectangular buttons with consistent dimensions per group, no rotation, and a visible hover/focus outline. `.key-summary` is hidden by default and appears on `.key-shell:hover` and `.key-shell:focus-within`. Preserve the existing restrained green/gray palette and 8px-or-less radii.

- [ ] **Step 7: Verify the basic wireframe**

Run: `rtk npm test -- src/App.test.tsx`

Expected: wireframe/model tests pass; configuration-form tests are not added yet.

- [ ] **Step 8: Commit the grouped keypad shell**

```bash
rtk git add src/types.ts src/Keypad.tsx src/App.tsx src/App.css src/App.test.tsx
rtk git commit -m "feat: render model keypad wireframes"
```

---

### Task 6: Configure model-specific IO mappings

**Files:**
- Create: `src/ButtonPopover.tsx`
- Modify: `src/Keypad.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.css`
- Test: `src/App.test.tsx`

**Interfaces:**
- Consumes: selected button/anchor, structured runtime `gpio`, `supportedGpios`, and `set_io_capture`.
- Produces: one-shot physical capture, manual GPIO fallback, conflict validation, and staged IO maps.

- [ ] **Step 1: Add failing capture and conflict tests**

Add:

```tsx
test("binds the selected button from the next physical press", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Configure 2" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_io_capture", { enabled: true }));
  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 1, level: "info", message: "GPIO7: captured",
    gpio: 7, connection: { state: "connected", port: "/dev/cu.test" },
  }}));
  expect(screen.getByLabelText("GPIO for 2")).toHaveValue("7");
});

test("rejects a GPIO already assigned to another button", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "Configure 3" }));
  await user.selectOptions(screen.getByLabelText("GPIO for 3"), "6");
  expect(screen.getByRole("alert")).toHaveTextContent("GPIO6 is assigned to 2");
  expect(screen.getByRole("button", { name: "Apply IO mapping" })).toBeDisabled();
});
```

- [ ] **Step 2: Run the focused tests and confirm RED**

Run: `rtk npm test -- src/App.test.tsx`

Expected: no GPIO form or capture invocation exists.

- [ ] **Step 3: Implement deterministic GPIO binding**

Export this pure helper from `ButtonPopover.tsx`:

```ts
export function ioConflict(
  ioMap: Record<number, string>,
  buttonId: string,
  gpio: number,
) {
  const existing = ioMap[gpio];
  return existing && existing !== buttonId ? existing : null;
}

export function bindGpio(
  ioMap: Record<number, string>,
  buttonId: string,
  gpio: number,
) {
  const next = Object.fromEntries(
    Object.entries(ioMap).filter(([, value]) => value !== buttonId),
  ) as Record<number, string>;
  next[gpio] = buttonId;
  return next;
}
```

The IO popover receives the selected button, active IO map, supported GPIOs, captured GPIO, and `onApply`. It renders a native `<select>` and Apply/Cancel buttons. Use labels matching the tests.

- [ ] **Step 4: Start and stop one-shot capture from App**

When an IO popover opens, invoke `set_io_capture({ enabled: true })`. On cancel, apply, mode change, selected-button change, or unmount, invoke it with `false`. Consume `RuntimeEvent.gpio` only while the local selected button is capturing; the backend has already replied `SKIP` and reset its one-shot flag.

- [ ] **Step 5: Anchor and flip the popover**

Add a pure `popoverPosition(anchor, width, height, viewportWidth, viewportHeight)` helper in `Keypad.tsx`. Place the fixed popover 12px to the right when it fits, otherwise 12px left; clamp top to 12px and `viewportHeight - height - 12px`. Test both right and left positions without relying on jsdom layout.

- [ ] **Step 6: Save and verify IO mappings**

On Apply, stage `ioMaps[activeModel]`. On global Save, invoke:

```ts
invoke<AppSnapshot>("save_workspace", {
  activeModel,
  ioMaps,
  actions,
  models,
});
```

Run: `rtk npm test -- src/App.test.tsx`

Expected: capture, manual fallback, conflict, anchoring, and save tests pass.

- [ ] **Step 7: Commit IO mapping UI**

```bash
rtk git add src/ButtonPopover.tsx src/Keypad.tsx src/App.tsx src/App.css src/App.test.tsx
rtk git commit -m "feat: bind keypad buttons to GPIO inputs"
```

---

### Task 7: Configure paste and recorded shortcut actions

**Files:**
- Create: `src/hotkey.ts`
- Create: `src/hotkey.test.ts`
- Modify: `src/ButtonPopover.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.css`
- Test: `src/App.test.tsx`

**Interfaces:**
- Consumes: browser `KeyboardEvent` and global `actions` by button ID.
- Produces: `normalizeHotkey(event)`, paste form, shortcut recorder, and behavior summaries.

- [ ] **Step 1: Write failing shortcut-normalization tests**

Create `src/hotkey.test.ts`:

```ts
import { expect, test } from "vitest";
import { normalizeHotkey } from "./hotkey";

test("normalizes command shift letter", () => {
  expect(normalizeHotkey({ code: "KeyK", metaKey: true, shiftKey: true,
    ctrlKey: false, altKey: false } as KeyboardEvent))
    .toEqual(["cmd", "shift", "k"]);
});

test("waits when only a modifier is pressed", () => {
  expect(normalizeHotkey({ code: "MetaLeft", metaKey: true, shiftKey: false,
    ctrlKey: false, altKey: false } as KeyboardEvent)).toBeNull();
});

test("rejects unsupported keys", () => {
  expect(() => normalizeHotkey({ code: "NumpadAdd", metaKey: false,
    shiftKey: false, ctrlKey: false, altKey: false } as KeyboardEvent))
    .toThrow("Unsupported shortcut key");
});
```

- [ ] **Step 2: Run the unit test and confirm RED**

Run: `rtk npm test -- src/hotkey.test.ts`

Expected: module-not-found failure for `./hotkey`.

- [ ] **Step 3: Implement shortcut normalization**

Create `hotkey.ts` with modifier ordering `cmd`, `ctrl`, `alt`, `shift`; map `KeyA..KeyZ` to lowercase letters, `Digit0..Digit9` to digits, and exact browser codes for Enter, Escape, Backspace, Tab, Space, ArrowUp/Down/Left/Right, Delete, Home, End, PageUp, and PageDown. Return `null` for modifier codes. Throw for all other codes.

```ts
const NAMED_KEYS: Record<string, string> = {
  Enter: "enter",
  Escape: "escape",
  Backspace: "backspace",
  Tab: "tab",
  Space: "space",
  ArrowUp: "arrow_up",
  ArrowDown: "arrow_down",
  ArrowLeft: "arrow_left",
  ArrowRight: "arrow_right",
  Delete: "delete",
  Home: "home",
  End: "end",
  PageUp: "page_up",
  PageDown: "page_down",
};
const MODIFIER_CODES = new Set([
  "MetaLeft", "MetaRight", "ControlLeft", "ControlRight",
  "AltLeft", "AltRight", "ShiftLeft", "ShiftRight",
]);

export function normalizeHotkey(event: KeyboardEvent): string[] | null {
  if (MODIFIER_CODES.has(event.code)) return null;
  let key: string | undefined;
  if (/^Key[A-Z]$/.test(event.code)) key = event.code.slice(3).toLowerCase();
  else if (/^Digit[0-9]$/.test(event.code)) key = event.code.slice(5);
  else key = NAMED_KEYS[event.code];
  if (!key) throw new Error(`Unsupported shortcut key: ${event.code}`);

  const keys: string[] = [];
  if (event.metaKey) keys.push("cmd");
  if (event.ctrlKey) keys.push("ctrl");
  if (event.altKey) keys.push("alt");
  if (event.shiftKey) keys.push("shift");
  keys.push(key);
  return keys;
}
```

- [ ] **Step 4: Add failing behavior-form tests**

Add one test that clicks `Configure 2`, switches action type to Paste, enters multiline Unicode, applies and saves. Add another that selects Shortcut, clicks `Record shortcut`, dispatches `keydown` with Command+Shift+K, and expects `Command + Shift + K` plus the saved `{ type: "hotkey", keys: ["cmd", "shift", "k"] }` payload.

- [ ] **Step 5: Implement the behavior popover**

In `ButtonPopover.tsx`, behavior mode uses a native action-type select. Paste renders a textarea. Shortcut renders a read-only display and a Record button.

While recording, install a capture-phase window listener:

```ts
const handler = (event: KeyboardEvent) => {
  event.preventDefault();
  event.stopImmediatePropagation();
  const keys = normalizeHotkey(event);
  if (keys) {
    setDraftAction({ type: "hotkey", keys });
    setRecording(false);
  }
};
window.addEventListener("keydown", handler, true);
return () => window.removeEventListener("keydown", handler, true);
```

This capture listener must win over the existing Command+S save shortcut. Apply stages `actions[buttonId]`; deleting an action removes only that global action, never an IO mapping.

- [ ] **Step 6: Verify behavior configuration**

Run: `rtk npm test -- src/hotkey.test.ts src/App.test.tsx`

Expected: normalization, paste, recording, summary, and save tests pass.

- [ ] **Step 7: Commit behavior configuration**

```bash
rtk git add src/hotkey.ts src/hotkey.test.ts src/ButtonPopover.tsx src/App.tsx src/App.css src/App.test.tsx
rtk git commit -m "feat: configure paste and shortcut actions"
```

---

### Task 8: Add the developer layout editor and complete verification

**Files:**
- Create: `src/LayoutEditor.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.css`
- Test: `src/App.test.tsx`

**Interfaces:**
- Consumes: selected `ModelLayout`.
- Produces: validated edits for group order, columns, button order, display labels, additions, and removals.

- [ ] **Step 1: Add failing layout-editor tests**

Add a test that opens `Edit layout`, changes the top group columns from 4 to 5, renames the `BACK_OUT` display label without changing its ID, moves it one position, applies, and verifies the staged `models` sent to `save_workspace`. Add a second test proving a duplicate new button ID disables Apply and displays `Button IDs must be unique`.

- [ ] **Step 2: Run the component test and confirm RED**

Run: `rtk npm test -- src/App.test.tsx`

Expected: no `Edit layout` control exists.

- [ ] **Step 3: Implement a native dialog editor**

Create `LayoutEditor.tsx` with props:

```ts
interface LayoutEditorProps {
  layout: ModelLayout | null;
  open: boolean;
  onCancel(): void;
  onApply(layout: ModelLayout): void;
}
```

Use a native `<dialog ref={dialogRef}>` and an effect that calls `dialogRef.current.showModal()` when `open` becomes true and `close()` when false. Handle the dialog `cancel` event through `onCancel`. Each group has a numeric columns input, label-edit inputs, icon buttons for move up/down and delete, and Add button controls. New IDs are normalized to uppercase `[A-Z0-9_]+`; existing IDs are read-only. Apply validates non-empty groups, columns >= 1, non-empty labels, and global button-ID uniqueness.

Use existing lucide icons (`ArrowUp`, `ArrowDown`, `Plus`, `Trash2`, `X`) with tooltips. Do not add drag-and-drop code or a dependency.

- [ ] **Step 4: Wire layout edits into global Save**

Applying the dialog replaces only the active layout in staged `models`. The keypad updates immediately. Revert restores saved models; Save sends all layouts through the existing `save_workspace` command.

- [ ] **Step 5: Run all automated checks**

Run: `rtk npm test`

Expected: all Vitest tests pass.

Run: `rtk npm run build`

Expected: TypeScript and Vite production build pass.

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml`

Expected: all Rust tests pass.

Run: `rtk uv run pio test -e native`

Expected: all Unity tests pass.

Run: `rtk uv run pio run -e esp32s3`

Expected: ESP32-S3 firmware compiles.

Run: `rtk git diff --check`

Expected: no whitespace errors.

- [ ] **Step 6: Perform desktop visual verification**

Run: `rtk npm run tauri dev`

Verify at the configured 1120x760 window and minimum 760x560 window:

- The model selector and two-mode switch remain visible.
- Top group is aligned and `BACK/OUT` appears once.
- Numeric buttons form a clean three-column group.
- Bottom function buttons form one equal-width row.
- Hover summaries do not resize or shift keys.
- Click popovers flip away from viewport edges and do not overlap the selected key.
- IO capture, paste editing, shortcut recording, Save, and Revert remain usable at minimum size.

- [ ] **Step 7: Commit the finished configurator**

```bash
rtk git add src/LayoutEditor.tsx src/App.tsx src/App.css src/App.test.tsx
rtk git commit -m "feat: edit model keypad layouts"
```

---

## Final Review Gate

Run:

```bash
rtk git status --short
rtk git log --oneline -8
```

Expected: clean worktree and one focused commit per task. Compare the finished behavior against `docs/superpowers/specs/2026-07-28-model-keypad-configurator-design.md`, with special attention to manual model selection, global semantic actions, one-shot capture suppression, legacy preservation, and the single `BACK_OUT` button.
