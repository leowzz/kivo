# Action Triggers And Device Workspace Design

**Date:** 2026-08-09

## Goal

Increase the Action editor's information density, make keyboard chords easy to
configure, add per-trigger Action sequences, and consolidate all physical-device
configuration into Device Management without making reusable configuration
files harder to understand or share.

The user-facing model has three concepts:

- A **device** is one physical Kivo controller.
- A **configuration** defines the layout, I/O mapping, trigger timing, and
  Actions that a device uses. Multiple devices may share one configuration.
- A **configuration file** is the import/export representation of a
  configuration. It is a sharing and synchronization carrier, not a device.

## Navigation And Ownership

The sidebar contains Home, Device Management, Button Behavior, and
Configuration Files. The existing Hardware Mapping and Key Layout destinations
are removed as separate navigation items.

### Device Management

Device Management owns physical-device selection and runtime assignment. Its
left region selects the device currently being managed. The selected-device
header contains one compact **Use configuration** select. Changing it changes
the configuration used by that physical device; it does not change the
configuration selected in Button Behavior.

The main Device Management workspace uses page tabs:

- **Overview** shows status, identity, firmware, activity, and assignment.
- **I/O Mapping** contains the current hardware mapping editor and learning
  workflow.
- **Key Layout** contains the current layout editor as page content.

I/O Mapping and Key Layout are not dialogs. They require enough space for
repeated editing, comparison, and learning feedback.

A gear beside **Use configuration** opens a compact **Configuration Settings**
dialog. This dialog contains only:

- long-press threshold;
- double-press interval; and
- the command to duplicate the configuration and assign the duplicate only to
  the selected device.

When more than one device uses the selected configuration, I/O Mapping and Key
Layout show a persistent warning naming the configuration and the number of
affected devices. Configuration Settings shows the same ownership state. The
normal save command is labeled **Save shared configuration**, and the alternate
command is **Duplicate and use only for this device**.

Duplicating from Device Management is one backend transaction: clone the full
configuration, generate unique profile and hardware mapping IDs, and switch the
selected device's runtime assignment. If any step fails, neither the source
configuration nor the existing device assignment changes.

The runtime assignment continues to reference both a Device Profile and a
compatible Hardware Profile internally. The normal UI exposes only the
configuration select:

- Preserve the current compatible Hardware Profile when possible.
- If the new configuration has exactly one compatible Hardware Profile, select
  it automatically.
- If it has none or more than one and no current match, leave the existing
  assignment unchanged and move the user to an inline resolver in I/O Mapping.

### Button Behavior

Button Behavior remains a full page. Its configuration select means **the
configuration I am editing**. It updates the existing editor-profile setting
and has no effect on any device's runtime assignment.

The page retains the keypad and selected-button relationship. The Action region
becomes a compact summary grouped in this fixed order:

1. Press
2. Release
3. Long press
4. Double press

Empty groups are omitted. Each row shows the Action type and a short value
summary, such as `Paste - Hello...`, `Wait - 300 ms`, or
`Key - Cmd + Shift + K`. The row opens the editor for that single Action. Move
commands only reorder Actions inside one trigger group because ordering across
different triggers has no runtime meaning.

One **Add Action** command opens the same dialog in create mode. A new Action
defaults to the Press trigger and the hotkey Action type. The dialog holds a
local draft and only updates the configuration when Save succeeds; Cancel never
leaves an invalid autosave draft. Changing an existing Action's trigger moves it
to the end of the destination trigger group. Delete is available in edit mode.

### Configuration Files

Configuration Files remains the lifecycle and transfer page for configurations:
create, import, export, duplicate, backup, restore, and delete. Remove the
current-editor select from this page. The page may show how many devices use a
configuration, but it does not change an editor target or runtime assignment.

## Trigger Model

Each button has four independent trigger groups:

- **Press** fires immediately on a stable Down edge and is the default.
- **Release** fires immediately on a stable Up edge.
- **Long press** fires once when the input remains Down for the configured
  threshold.
- **Double press** fires on the second stable Down edge when a complete
  Down-Up-Down sequence falls within the configured interval.

There is no Short Press trigger. Press never waits for long-press or
double-press recognition. Edge triggers and gesture triggers are additive:

- A long hold fires Press first, Long Press at the threshold, and Release on Up
  when those groups contain Actions.
- A double press fires Press for each Down, Release for each Up, and Double Press
  after the second Down when those groups contain Actions.
- If the second press remains held, its Long Press may also fire.

A long press invalidates that press as the first half of a future double press.
Debounced duplicate edges do not create trigger occurrences. Trigger groups
produced by one edge are queued Press first and Double Press second. All Action
sequences remain serialized per device; Actions inside one trigger group run in
their configured order.

Trigger timing belongs to the reusable configuration and travels through
import/export:

```yaml
trigger_settings:
  long_press_ms: 500
  double_press_ms: 300
```

The defaults and validation bounds are:

| Setting | Default | Minimum | Maximum |
| --- | ---: | ---: | ---: |
| Long press | 500 ms | 100 ms | 5000 ms |
| Double press | 300 ms | 100 ms | 1000 ms |

Disconnect, runtime assignment change, I/O reconfiguration, learning-mode
entry, and configuration snapshot replacement clear unfinished long-press and
double-press state. A trigger occurrence uses the configuration snapshot that
was active when its originating Down edge was accepted.

## Configuration Schema

Increase the Device Profile schema version from 2 to 3. Store Action sequences
by button and trigger rather than repeating a trigger field on every Action:

```yaml
schema_version: 3
trigger_settings:
  long_press_ms: 500
  double_press_ms: 300
actions:
  HANDSET:
    press:
      - type: open
        target: Phone.app
    release:
      - type: media
        command: play_pause
```

The frontend and backend expose a shared trigger enum with `press`, `release`,
`long_press`, and `double_press`. Missing trigger groups deserialize as empty
lists and empty groups are omitted when serializing.

Schema 2 configurations are migrated on workspace load and import. Every
legacy button Action list becomes that button's Press list. Schema 1 workspace
migration produces schema 3 directly through the same rule. This preserves the
existing Down-edge behavior. Migration retains configuration IDs, layouts,
hardware profiles, Action order, and runtime assignments.

## Hotkey Action Editor

The hotkey editor lives only inside the single-Action dialog. Its closed control
shows selected keys as removable chips and opens a categorized searchable
picker. Categories are Common, Function Keys (F1-F24), Letters, Numbers,
Symbols, Navigation, and Numeric Keypad. Standard USB HID cannot transmit a
physical laptop Fn key; the UI must not present Fn itself as a selectable key.

Modifier selection is multi-select. The compact default choices are Cmd/Ctrl,
Cmd, Ctrl, Option/Alt, and Shift. A **Distinguish left and right** disclosure
shows explicit left and right variants for Ctrl, Shift, Alt/Option, and GUI
(Command on macOS, Win on Windows).

The canonical modifier mask uses all eight USB HID bits:

| Bit | Modifier |
| ---: | --- |
| 0 | Left Ctrl |
| 1 | Left Shift |
| 2 | Left Alt/Option |
| 3 | Left GUI/Command/Win |
| 4 | Right Ctrl |
| 5 | Right Shift |
| 6 | Right Alt/Option |
| 7 | Right GUI/Command/Win |

Legacy `cmd`, `ctrl`, `alt`, and `shift` values canonicalize to the corresponding
left modifier. `primary` remains portable and maps to Left Command on macOS and
Left Ctrl on Windows. A generic choice conflicts only with the exact physical
bit it represents, so Left Command and Right Command may both be selected.

A chord accepts zero to six distinct ordinary HID keys, provided at least one
modifier or ordinary key is present. Duplicate physical keys and combinations
that resolve two aliases to the same modifier bit are rejected. The picker
shows the ordinary-key count and prevents a seventh selection.

Recording listens to keydown and keyup while the dialog owns focus. It tracks
physical `KeyboardEvent.code` values so left/right modifiers and multiple
ordinary keys are preserved. Recording commits once every captured key has been
released. Escape is captured as a key while recording instead of closing the
dialog. More than six ordinary keys produces an inline error and preserves the
previous chord. Manual selection remains available for shortcuts intercepted by
the operating system before Kivo receives them.

## Runtime Trigger Engine

Gesture recognition belongs to the host Device Session because the host owns
the configuration snapshot, timing values, Action queues, and configuration
lifecycle. Firmware continues to debounce and report physical Down and Up
edges.

Maintain one gesture record per physical input. On Down, enqueue Press Actions,
start the long-press deadline, and compare the Down-Up-Down history with the
double-press interval. On Up, enqueue Release Actions and cancel a pending long
deadline that has not fired. Worker polling services elapsed long-press
deadlines even when no serial input arrives.

Every trigger occurrence receives a monotonically ordered host sequence, so
paste coordination and per-device Action ordering remain deterministic for
edge-driven and timer-driven triggers. Input metrics continue to count stable
Down edges only. Runtime activity records the physical edge separately from
the derived trigger occurrence.

## Protocol Version 6

Increase the host and managed firmware protocol version to 6. Version 6
decouples input event IDs from Action run IDs:

- `STATE` continues to report physical input and Down/Up state but creates no
  firmware-side pending Action response.
- The host allocates a nonzero, monotonically increasing Action run ID per
  device.
- The first command for a run must have step 1 and establishes the firmware-side
  run. Subsequent commands retain the existing run ID, step, total, and `DONE`
  acknowledgement contract.
- `SKIP <run_id>` cancels an active run. Reconnect and reconfiguration also
  clear it.

Version 6 uses the existing PASTE, DELAY, MEDIA, and HOST step shapes and adds:

```text
CHORD <run_id> <step> <total> <modifier_mask> <key_count> [keycode...]
```

`key_count` is 0 through 6 and must equal the number of following keycodes.
Every keycode must be a supported, nonzero USB HID keyboard usage and must be
unique. A zero modifier mask with zero keys is invalid. Protocol lines remain
under the existing 255-byte bound.

Firmware accepts one host-serialized active run. Delay keeps the run alive with
the existing acknowledgement timeout behavior, while input scanning and STATE
reporting continue. No input event remains pending while the host waits for a
gesture threshold, which removes the existing two-second coupling.

Both platform implementations change from one keycode to a six-slot keyboard
report. RP2040 fills `hid_keyboard_report_t.keycode`; ESP32-S3 fills
`KeyReport.keys`. Each sends the complete pressed report followed by an empty
release report. Modifier-only reports are valid.

The host remains able to connect to protocol versions 3 through 5 through the
legacy event-response path. A configuration requires protocol 6 when it uses a
non-Press trigger, more than one ordinary hotkey key, or a modifier-only chord.
Assigning such a configuration to older firmware is rejected with an explicit
firmware-update status; unsupported behavior is never silently skipped.

## Failure Handling

- Invalid trigger timing, trigger names, chord masks, key counts, keycodes, and
  schema versions fail validation before autosave or assignment.
- If a device disconnects while its page is open, configuration editing remains
  available but learning and runtime-only controls are disabled.
- If an Action dialog is canceled or fails validation, the saved configuration
  is unchanged.
- If a shared-configuration clone fails, the original profile and runtime
  assignment remain active.
- Runtime timeouts and malformed acknowledgements cancel only the active Action
  run, preserve later queued trigger occurrences, and emit structured activity.
- A device with incompatible firmware remains visible with an update-required
  status and cannot be placed into a partially active configuration.

## Accessibility And Responsive Behavior

All icon-only commands have accessible names and tooltips. Tabs, dialogs,
selects, disclosures, and key choices remain keyboard operable. The hotkey
picker exposes each selected key through checkbox semantics and announces the
six-key limit.

At narrow widths, Device Management stacks the selected-device list above the
workspace and keeps I/O Mapping and Key Layout in the page scroll region. The
single-Action dialog uses the viewport width, wraps selected chips, and reduces
the key grid column count. Labels, counts, and toolbar controls have stable
dimensions and may wrap without overlapping adjacent content.

## Testing And Verification

Implementation follows test-driven development and includes:

1. Frontend tests for page navigation, separate editor and runtime-assignment
   selectors, compact trigger summaries, Action dialog create/edit/cancel,
   default Press trigger, trigger moves, and within-group ordering.
2. Frontend tests for categorized hotkey selection, six-key enforcement,
   modifier-only chords, left/right modifiers, recording through key release,
   intercepted-key fallback, responsive dialog layout, and accessible names.
3. Device Management tests for device selection, compatible configuration
   assignment, inline I/O resolution, persistent shared warnings, configuration
   timing, atomic duplicate-and-assign, I/O learning, and Key Layout editing.
4. Rust tests for schema 1/2 migration to schema 3, validation bounds, shared
   clone transactions, compatibility gating, Action grouping, and deterministic
   trigger ordering under a fake clock.
5. Trigger-engine tests for Press, Release, Long Press, Double Press, long-press
   invalidation of a double candidate, second-press long hold, disconnect,
   reconfiguration, and independent simultaneous inputs.
6. Protocol tests for version 6 parsing, host-created runs, cancellation,
   malformed CHORD commands, legacy protocol behavior, timeout isolation, and
   Action acknowledgements.
7. Native firmware tests for six-slot keyboard reports, modifier-only reports,
   every left/right modifier bit, release reports, delay keepalive, and input
   scanning during active runs.
8. Full frontend, Rust, Clippy, firmware native, and production-build gates,
   followed by RP2040 and ESP32-S3 hardware smoke tests for pickup/release,
   long press, double press, multi-key chords, and right-side modifiers.
9. Playwright screenshots at desktop and narrow widths for Button Behavior,
   the Action dialog, Device Management I/O Mapping, Key Layout, shared warning,
   and Configuration Settings.

## Out Of Scope

- Per-device override layers on top of a shared configuration.
- A Short Press trigger or tap-versus-hold arbitration.
- More than six simultaneous ordinary keyboard usages or NKRO descriptors.
- Sending a laptop's hardware Fn key.
- Synchronization transport, cloud storage, or conflict resolution beyond
  import/export files.
