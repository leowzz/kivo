# Model Keypad Configurator Design

Date: 2026-07-28
Status: Approved for implementation planning

## Context

Vibe Tool currently maps supported GPIO numbers directly to pasted text. The new interface must represent the physical keypad visually while separating hardware wiring from user behavior:

1. A developer creates a keypad layout for each device model from a photograph.
2. Each model stores its own GPIO-to-button mapping.
3. Users configure button-to-action mappings independently of GPIO wiring.

The first supported model is the red telephone shown in `assets/tel.jpg`.

## Goals

- Render a clean keypad wireframe without showing the source photograph.
- Let users manually choose the active device model.
- Provide separate IO mapping and behavior configuration modes.
- Bind a visual button by listening for the next physical button press.
- Support pasted text and one keyboard shortcut per button.
- Keep model layout, model IO mapping, and reusable button behavior separate.
- Preserve existing GPIO-to-text configuration during migration.

## Non-goals

- Runtime photo upload or image recognition.
- Pixel-perfect reproduction of the source photograph.
- Arbitrary button rotation or per-button visual sizing.
- Long press, double-click, release, macros, app launching, shell commands, or media control.
- Automatic device-model detection.
- Persisting configuration to device firmware.

## Model Layouts

Photo recognition happens during development. A developer gives the device photograph to Codex or another image-capable development tool, which produces an initial model layout file. The application ships no vision model or image-processing dependency.

Each model has a JSON layout file under `models/`. A layout contains ordered groups; each group declares its column count and ordered buttons. Buttons within a group are equal-sized and aligned. Groups may use different column counts, so the keypad does not need to fit one global rectangular grid.

Example:

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

The layout editor is a developer tool, not a third configuration mode. It supports adding and removing groups or buttons, editing display labels, changing group column counts, and reordering groups or buttons. Button IDs remain stable after creation because IO and action configuration refer to them.

## User Configuration

The main YAML configuration stores the manually selected model, model-specific IO maps, and global button actions:

```yaml
active_model: red-phone-v1

io_maps:
  red-phone-v1:
    6: DIGIT_2
    7: DIGIT_3

actions:
  DIGIT_2:
    type: hotkey
    keys: [cmd, shift, k]
  DIGIT_3:
    type: paste
    text: 你好
```

Button IDs are semantic and global. If two models both expose `DIGIT_2`, they reuse the same behavior. Their GPIO mappings remain separate.

Supported actions are a tagged union:

```text
paste(text)
hotkey(modifiers[], key)
```

A hotkey contains zero or more modifiers and exactly one non-modifier key. A plain key is represented as a hotkey with no modifiers.

## Interface

The top toolbar contains:

- A model selector.
- An `IO mapping / Behavior` segmented control.
- A layout-editor button.
- Existing connection state and activity information.
- Revert and Save controls when configuration is dirty.

The center workspace renders only the normalized keypad wireframe. Each group is laid out independently using its declared column count. Buttons do not preserve photograph rotation, contour, or small size differences.

Hovering a button shows a small read-only summary:

- IO mode: button label and mapped GPIO, or `Unmapped`.
- Behavior mode: pasted-text preview or recorded shortcut, or `No action`.

Clicking a button opens an anchored popover that remains open until applied, cancelled, or replaced by another button. The popover automatically flips sides near a viewport edge.

### IO Mapping Mode

Opening a button starts capture. The next physical press fills its GPIO automatically. The form also provides a manual GPIO selector for disconnected-device use and corrections.

While capture is active, the backend replies `SKIP` to physical presses so an existing behavior cannot execute accidentally. Capture stops when the form is applied, cancelled, closed, or replaced.

The form rejects a GPIO already assigned to another button in the active model and links to the conflicting button. Applying updates staged state; Save persists it.

### Behavior Mode

The form chooses either pasted text or a shortcut.

For pasted text, it exposes the existing multiline editor. For shortcuts, clicking Record captures keyboard events inside the application, prevents the recorded shortcut from reaching the application, and displays a normalized shortcut such as `Command + Shift + K`. Modifier-only shortcuts and shortcuts containing multiple non-modifier keys are rejected.

Behavior may be configured before IO mapping. Such a button shows `Unmapped` but retains its action for later binding.

## Runtime Flow

The device continues to report physical presses:

```text
PRESS <event_id> <gpio>
```

The Tauri backend resolves the active model's GPIO map, then resolves the resulting button ID's action. It replies with one of:

```text
PASTE <event_id>
HOTKEY <event_id> <modifier_mask> <hid_keycode>
SKIP <event_id>
```

For paste, the backend writes text to the clipboard before returning `PASTE`; the ESP32 sends Command+V as it does today. For a hotkey, the backend converts the human-readable configuration to a USB HID modifier mask and keycode. The ESP32 presses the modifier mask and key, waits briefly, then releases all keys.

The firmware accepts a response only for its current pending `event_id`. Invalid, stale, or unsupported commands do nothing. Any backend action-resolution or clipboard error returns `SKIP`.

The runtime event payload gains structured press information so the UI can capture GPIO without parsing log text.

## Persistence and Migration

Model layout files are validated independently from user configuration. All writes use the existing temporary-file-and-rename approach so a failed save preserves the previous file.

The legacy schema remains readable:

```yaml
buttons:
  6: hello
```

When the active model maps GPIO 6 to a button, the loader creates a missing `paste` action for that button. Legacy GPIO entries that cannot yet resolve through the model IO map remain preserved in the file until they can be mapped. Existing explicit actions win over migrated legacy text.

## Error Handling

- An invalid active model shows an error and no keypad; other valid models remain selectable.
- Duplicate GPIO assignments are rejected before save and identify both buttons.
- A disconnected device disables automatic capture but leaves manual GPIO selection available.
- Invalid layout, action, or hotkey data never replaces the last valid runtime configuration.
- Removing a button from one model does not delete its global action because another model may use the same button ID.
- Switching models while edits are dirty requires Save or Revert first.

## Verification

Focused frontend coverage verifies:

- Manual model selection and the grouped wireframe.
- Mode switching, hover summaries, and anchored click popovers.
- Physical GPIO capture, manual fallback, and duplicate detection.
- Paste editing, shortcut recording, and save payloads.

Focused Rust coverage verifies:

- Model and configuration validation.
- Model-specific GPIO resolution and global action resolution.
- Legacy migration without data loss.
- Capture mode suppressing action execution.
- `PASTE`, `HOTKEY`, and `SKIP` response generation.

Focused firmware coverage verifies:

- Parsing all three response commands.
- Ignoring malformed or stale responses.
- Pressing and releasing the requested shortcut exactly once.

The final implementation must also pass the existing frontend, Rust, and PlatformIO test suites, the production frontend build, and a desktop visual check of the keypad and edge-positioned popovers.

## Acceptance Criteria

- A user can manually select a model and see its normalized keypad wireframe.
- The red-phone layout renders `BACK/OUT` as one button.
- Hover shows mode-specific information without opening an editor.
- Clicking opens a stable, anchored configuration popover.
- A physical press can bind the selected visual button without executing its old action.
- IO mappings are isolated by model.
- Button behaviors are shared by semantic button ID.
- Paste and recorded shortcuts execute through the ESP32 USB HID path.
- Invalid or failed saves preserve the last working configuration.
