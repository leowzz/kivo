# Manual Hotkey Editor Design

## Goal

Allow a hotkey action to be configured without pressing the shortcut, because another application may intercept it before Kivo receives the keyboard event.

## Interface

Each hotkey action keeps its current formatted output and **Record key** button. Below the output, add four independent modifier checkboxes for Ctrl, Cmd, Alt, and Shift, plus one native select for the ordinary key.

Changing any checkbox or the select updates the action immediately through the existing `onChange` and autosave path. Recording a shortcut updates the same controls and remains available as the faster path when the shortcut reaches Kivo.

The ordinary-key select contains exactly the keys accepted by the backend:

- letters A-Z
- digits 0-9
- Enter, Escape, Backspace, Tab, Space, and Delete
- Arrow Up, Arrow Down, Arrow Left, and Arrow Right
- Home, End, Page Up, and Page Down

`Fn` is not included because the device sends standard USB HID keyboard reports, which do not provide an Fn modifier bit.

## Data Flow

No schema, protocol, firmware, or backend changes are required. The editor continues to store `{ type: "hotkey", keys: string[] }`.

Modifier values use the existing backend names `cmd`, `ctrl`, `alt`, and `shift`. The final ordinary key remains the last item. The editor writes modifiers in the same canonical order as keyboard recording, followed by the selected ordinary key.

## Validation

The UI always supplies exactly one ordinary key, so it cannot create the malformed modifier-only or multiple-ordinary-key combinations rejected by `encode_hotkey`. Existing imported models remain subject to the current backend validation.

## Verification

Add one UI regression test that selects multiple modifiers and a letter, then verifies the saved action payload. Keep the existing window-recording regression test to prove both input paths update the same action.

## Out of Scope

- Fn or macOS Globe-key injection
- multiple ordinary keys in one hotkey action
- new hotkey schema or firmware protocol
- free-form key names
