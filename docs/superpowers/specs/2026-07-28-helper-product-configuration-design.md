# Kivo Product Configuration Design

Date: 2026-07-28
Status: Awaiting written spec review

## Context

Kivo currently treats GPIO mapping as a first-class editing mode and stores all
model IO maps plus one global button-action map in `config.yaml`. Each button can
run only one action. Model layouts live in separate JSON files, and the helper
has no product-level import, export, deletion, backup, restore, localization, or
automatic-save workflow.

The product workflow is the inverse: users mainly configure button behavior.
GPIO and contact wiring are development-time hardware details. A production
helper ships validated model configurations, while an advanced adapter can map
new direct-wired or carbon-contact keypads.

## Goals

- Make button behavior the primary workflow and move hardware mapping into an
  advanced model-level area.
- Store layout, hardware topology, GPIO/contact mapping, and ordered button
  actions in one self-contained file per device model.
- Support atomic single-model import and replacement, single-model export,
  deletion, complete backup, and complete restore.
- Auto-save valid changes without a global Save or Revert operation.
- Let each key run an ordered list containing repeated paste and key/hotkey
  actions.
- Make Simplified Chinese and English complete, switchable interface languages;
  default to Simplified Chinese.
- Support direct GPIO inputs and sparse carbon-contact matrices, including a
  mixture of both in one model.
- Keep an advanced, explicit GPIO/contact learning flow for development-time
  device adaptation.
- Implement the feature on ESP32-S3 while keeping configuration and serial
  protocol boundaries extensible to a later RP2040 firmware port.

## Non-goals

- Automatically opening a hardware-learning wizard on first launch.
- Automatically scanning every GPIO on a board.
- Deriving the electrical topology of `assets/tel.jpg` from its front photo.
- Implementing RP2040 USB CDC/HID firmware in this project phase.
- Adding delay, app launch, shell command, script, media, long-press,
  double-click, or release-trigger actions.
- Supporting arbitrary input-driver plugins. The first release implements only
  `direct` and `contact_matrix` sources.
- Persisting user action text or action sequences in device flash.

## Product Interface

The desktop window uses a three-column model workspace.

### Top Bar

The top bar shows the Kivo brand, connection state, connected port, auto-save
state, and language switch. Auto-save state is one of `正在保存`, `已保存`, or
`保存失败`; the English equivalents are `Saving`, `Saved`, and `Save failed`.
A failed save exposes a retry icon button. There is no global Save or Revert
button.

### Model Sidebar

The left sidebar contains the model selector and these destinations:

- Button behavior / 按键行为
- Hardware mapping / 硬件映射
- Key layout / 按键布局
- Import model / 导入型号
- Export model / 导出型号
- Full backup / 全量备份
- Delete model / 删除型号

Button behavior is the default destination. Hardware mapping and key layout are
secondary tools, not peer modes in a segmented control.

### Button Behavior Workspace

The center column renders the active model keypad. Each key shows its label and
action count. GPIO numbers or contact pairs appear only as subdued metadata.
Selecting a key opens its persistent action editor in the right column.

The editor renders actions in execution order. Users can add paste or key
actions, edit one action, delete one action, and reorder actions by drag handle
or accessible move-up/move-down icon buttons. Duplicate action types and values
are allowed. An empty action list means the key does nothing.

Paste accepts any non-empty UTF-8 string, including whitespace and multiline
content. A key action contains zero or more modifiers and exactly one ordinary
key, so a plain key and a shortcut share one representation.

### Empty Workspace

Deleting the final model is allowed. The empty workspace gives Import model and
Restore backup equal primary prominence. Adapt new device remains a secondary
advanced action. No bundled model is silently recreated after a user deletes
all models.

## Localization

The first release includes complete `zh-CN` and `en-US` translation resources.
The first launch defaults to `zh-CN`; switching language updates the interface
immediately and persists the preference.

UI labels, dialogs, validation messages, import previews, empty states, runtime
activity, and save errors are translated. Model names, button labels, action
text, and imported filenames preserve UTF-8 exactly. Stable model, group,
button, input-source, error, and event IDs remain ASCII identifiers.

Rust sends structured error and activity payloads with a stable code, named
parameters, and an optional technical detail. React translates the code and
parameters; backend English sentences are not a user-visible contract. A small
local TypeScript translation dictionary is sufficient for two languages; no
internationalization framework is added.

## Storage Layout

The Tauri application configuration directory contains one swappable data
directory:

```text
data/
  settings.yaml
  models/
    red-phone-v1.yaml
    another-model.yaml
```

`settings.yaml` has this shape:

```yaml
schema_version: 1
active_model: red-phone-v1
language: zh-CN
```

`active_model` is `null` when the workspace has no models. `language` is exactly
`zh-CN` or `en-US`.

Each model file is self-contained:

```yaml
schema_version: 1
model:
  id: red-phone-v1
  name: 红色电话机
  groups:
    - id: digits
      columns: 3
      buttons:
        - id: DIGIT_1
          label: "1"
        - id: DIGIT_2
          label: "2"

hardware:
  controller: esp32s3
  debounce_ms: 30
  inputs:
    - id: main-keypad
      type: contact_matrix
      pins: [1, 2, 3, 4, 5, 6, 12, 13, 14, 15]
      keys:
        DIGIT_1: [1, 12]
        DIGIT_2: [1, 13]
    - id: extra-buttons
      type: direct
      keys:
        DEL: 16

actions:
  DIGIT_2:
    - type: paste
      text: 你好
    - type: hotkey
      keys: [enter]
```

The persisted file under `data/models/` is always named `<model.id>.yaml`. The
same document is the single-model import/export format; an imported source file
may have any filename because `model.id` is authoritative. Kivo does not
maintain a second export DTO. Suggested exported filenames use
`<model-id>.kivo-model.yaml`.

Migrated files may also contain this migration-only field, which new model
creation omits:

```yaml
legacy:
  unresolved_gpio_text:
    7: preserved text
```

## Model Validation

Kivo validates the whole model before it can replace runtime state:

- `schema_version` must be `1`.
- Model and group IDs use ASCII letters, digits, hyphens, or underscores.
- Button IDs are non-empty and unique within the model. Unicode display names
  and labels are valid.
- `hardware.controller` is a non-empty ASCII platform ID. Only `esp32s3` can be
  activated in this release; future-platform files remain importable and
  editable.
- `debounce_ms` is an integer from `1` through `1000`; the default is `30`.
- Input-source IDs are unique. A GPIO may belong to only one source.
- A direct binding references one integer GPIO from `0` through `255` and one
  existing button.
- A contact matrix lists at least two distinct integer GPIOs from `0` through
  `255`. Each key references two different listed pins, and each unordered
  contact pair is unique.
- A button has at most one hardware binding across all sources. Unmapped
  buttons are valid.
- Contact-pair edges must form a bipartite graph. Kivo derives row and column
  partitions from that graph before sending the topology to firmware; sparse
  matrices and disconnected components are valid.
- Action keys reference existing buttons. Action list order is preserved.
- Paste text must not be an empty string; it is not trimmed. Hotkeys pass the
  existing modifier and ordinary-key validation.

The helper validates GPIOs against the allowlist reported by connected firmware
before applying a topology. Imported configurations remain editable while the
device is disconnected, but an unsupported controller or GPIO prevents runtime
activation and produces a localized error.

## Bundled Production Models

`models/prod/*.yaml` becomes the production preset catalog. Every shipped file
uses the same self-contained model format and must pass full validation before
packaging. Production files are copied into the user data directory only when
creating a new workspace. They never overwrite existing user copies and are not
reseeded after the user deletes all models.

The current layout-only JSON files remain migration inputs; they are not treated
as electrically complete presets. In particular, Kivo does not invent a
`tel001` contact matrix from `assets/tel.jpg`. Promoting that model to a complete
production YAML requires measured keypad lines and key contact pairs.

## Legacy Migration

Migration runs only when `data/settings.yaml` does not exist and the legacy
model/config files do exist.

1. Load and validate all legacy model layouts and `config.yaml` without changing
   them.
2. Create one new model document per layout.
3. Invert each model's GPIO-to-button map into a `direct` source.
4. Copy every global action referenced by that model into a one-element action
   list owned by the model.
5. Put unresolved legacy GPIO-to-text entries under the active model's
   `legacy.unresolved_gpio_text` field so migration never drops them.
6. Write all new files into a staged `data.next` directory.
7. Rename `data.next` to `data` only after every file succeeds.

Legacy files remain untouched. When a later hardware binding resolves an
unresolved GPIO, Kivo creates a missing paste action for that button and removes
that legacy entry. Explicit actions always win.

## Auto-save

Valid editor changes are debounced for 400 milliseconds. The frontend owns one
serial save queue. If state changes while a request is running, the queue saves
the newest revision after the current request finishes; an older completion can
never replace a newer local revision.

Existing action fields use local drafts. An invalid or incomplete draft shows a
field error and does not replace the last valid model. Reorder, delete, model
selection, and language selection are complete operations and enter the save
queue immediately. Switching models waits for the active model's queued save to
finish.

The backend atomically replaces individual YAML files using the existing
temporary-file-and-rename helper. A successful file write updates the saved
workspace even while no device is connected. Runtime input keeps its last
working topology until a connected device acknowledges a changed topology. A
failed file save keeps the edited frontend revision, shows Save failed, and can
be retried; a firmware activation failure keeps the saved edit but marks runtime
input inactive with a localized device error.

## Import, Export, Delete, and Backup

Native Tauri open/save dialogs select all import and export paths.

### Single Model

Import reads and validates one `.kivo-model.yaml` document before presenting a
preview with model name, button count, hardware-binding count, and action count.
If `model.id` already exists, the dialog states that layout, hardware mapping,
and actions will all be replaced. Confirmation atomically replaces only that
model file. Failure leaves the existing file and runtime state unchanged.

Export writes the active model document unchanged apart from deterministic YAML
serialization. Deleting a model requires a summary confirmation. The backend
removes the model only after any required active-model settings update can
succeed; ordinary failures restore the prior state.

### Full Backup

The suggested filename is `kivo-backup-YYYY-MM-DD.yaml`. Its shape is:

```yaml
schema_version: 1
settings:
  schema_version: 1
  active_model: red-phone-v1
  language: zh-CN
models:
  - schema_version: 1
    model:
      id: red-phone-v1
      name: 红色电话机
      groups:
        - id: digits
          columns: 1
          buttons:
            - id: DIGIT_1
              label: "1"
    hardware:
      controller: esp32s3
      debounce_ms: 30
      inputs: []
    actions: {}
```

Restore validates the complete document and shows model, button, hardware
binding, and action totals. Confirmation replaces the complete snapshot; it is
not a merge. Kivo stages a complete `data.next` directory, swaps it with `data`,
and retains a rollback directory until settings and all models reload
successfully. An ordinary restore failure rolls back to the old directory.

## Runtime Hardware Configuration

On connection, ESP32-S3 firmware reports protocol version, platform, and its
board-specific safe GPIO allowlist. Kivo rejects a mismatched protocol,
controller, or GPIO before activating input.

Kivo sends the selected model topology as a revisioned transaction:

```text
CONFIG_BEGIN <revision> <debounce_ms>
CONFIG_DIRECT <revision> <source_index> <pin_count> <pins...>
CONFIG_MATRIX <revision> <source_index> <pin_count> <pins...>
CONFIG_COMMIT <revision>
CONFIG_OK <revision>
CONFIG_ERROR <revision> <code>
```

`source_index` is a temporary zero-based index assigned by the helper. Firmware
applies no partial topology: it validates the complete pending revision and
switches scanners only on `CONFIG_COMMIT`. Input remains disabled until
`CONFIG_OK`.

Direct events contain one GPIO. Matrix events contain the source index and an
unordered contact pair:

```text
STATE <event_id> DIRECT <gpio> DOWN|UP
STATE <event_id> CONTACT <source_index> <pin_a> <pin_b> DOWN|UP
```

The helper resolves the physical signature to a logical button through the
active model. A contact matrix scans one derived partition as outputs and the
other as pulled-up inputs. The first release targets single-key telephone use;
non-ambiguous simultaneous presses may be reported, while a newly ambiguous
ghost combination is suppressed and logged.

## Advanced Contact Learning

Contact learning is available from Hardware mapping as a secondary Adapt new
device action. It is never shown automatically.

The user first selects the ESP32-S3 board profile and explicitly checks only the
GPIOs physically wired to the keypad. Kivo validates those pins against the
firmware allowlist. Flash, PSRAM, USB, boot-strapping, and other board-reserved
pins are never added automatically.

The user then selects one logical key and presses and releases its physical key.
Firmware keeps all candidate lines as pulled-up high-impedance inputs, then
drives only one candidate low at a time while reading the rest. A stable single
line to ground becomes a direct binding; a stable unordered pair becomes a
contact-matrix binding. The GUI shows the captured signature and conflicts
before applying it. The flow repeats only when the developer selects another
key.

Learning requires the keypad to be isolated from the original telephone ASIC
and any external voltage. The UI presents this safety confirmation before it
enables active scanning. Series protection resistors remain a hardware assembly
recommendation, not a software substitute for isolation.

## Ordered Action Execution

For a button-down event, Kivo resolves one ordered action list and executes only
one list at a time. Other button-down events remain queued without interleaving
their steps.

Each step uses a device acknowledgement:

```text
PASTE <event_id> <step> <total>
HOTKEY <event_id> <step> <total> <modifier_mask> <keycode>
DONE <event_id> <step>
SKIP <event_id>
```

`step` is one-based. Before a paste step, Kivo writes that step's text to the
clipboard, then sends the OS-appropriate paste response (`PASTE` on macOS or the
equivalent `HOTKEY` on Windows). ESP32-S3 validates the event ID, expected step,
and total, performs the HID operation, and returns `DONE`. Kivo does not prepare
or send the next step until it receives that acknowledgement. A valid step
refreshes the firmware's pending-event timeout; the final step closes it.

Empty or unmapped action lists receive `SKIP`. Clipboard failure, serial write
failure, disconnect, invalid acknowledgement, or acknowledgement timeout stops
the remaining list and sends `SKIP` when the connection still permits it. The
activity event identifies the model, button, failed step, and structured error.

## Error Handling

- Invalid startup data does not replace the last valid runtime model.
- A model with an unsupported controller or GPIO remains editable but inactive.
- Import and restore parse no more than 10 MiB before rejecting the file.
- Import replacement, delete, migration, and restore update frontend/runtime
  state only after their backend operation succeeds.
- A serial disconnect clears pressed-key feedback and aborts the active action
  sequence; reconnect resends the active topology before accepting presses.
- Learning stops and restores the last runtime topology when cancelled,
  disconnected, or closed.
- Changing models while an action list is running aborts that list before the
  new topology is configured.

## Verification

Focused Rust tests cover model validation, Unicode round trips, legacy
migration, ordered action lists, deterministic model export, same-ID import
replacement, delete-to-empty, full snapshot restore, rollback on write failure,
structured errors, topology resolution, serial configuration transactions, and
step acknowledgement/failure behavior.

Focused React tests cover the Chinese default, English switching, three-column
layout, action add/edit/delete/reorder, accessible move controls, serialized
auto-save, failed-save retry, model import preview, destructive confirmations,
full restore, empty workspace, and the non-default advanced learning entry.

Focused native firmware tests cover direct scanning, sparse contact-matrix
scanning, debounce calibration, ghost suppression, configuration transactions,
contact-learning signatures, stale and malformed events, ordered step
acknowledgements, and timeout cleanup.

Final verification also runs the complete frontend, Rust, and PlatformIO test
suites, the production frontend/Tauri build, desktop and narrow-window visual
checks, and two ESP32-S3 physical checks:

- Paste Chinese text, then press Enter.
- Paste two different texts in one action list and confirm strict clipboard/HID
  order.

## Acceptance Criteria

- Kivo opens in Simplified Chinese on first launch and can switch completely to
  English.
- The default page prioritizes button behavior; hardware mapping is secondary.
- A key can own an ordered list of repeated paste and key/hotkey actions.
- Valid edits save automatically without a global Save/Revert control, and a
  failed save remains retryable without losing the edit.
- A device model exports and imports as one self-contained YAML file.
- Same-ID model import previews and atomically replaces only that model.
- Full backup restore replaces the entire snapshot and rolls back on ordinary
  failure.
- The final model can be deleted without being recreated on restart.
- One model can combine direct GPIO inputs and a sparse contact matrix.
- ESP32-S3 receives and acknowledges the active topology before input becomes
  live.
- Advanced learning captures an explicitly selected direct GPIO or contact pair
  without scanning unapproved pins.
- Existing model layouts, IO maps, actions, and unresolved legacy text survive
  migration without invented hardware data.
