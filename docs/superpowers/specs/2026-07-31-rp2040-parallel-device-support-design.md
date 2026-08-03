# RP2040 Parallel Device Support Design

Date: 2026-07-31
Status: Approved

## Goal

Add VCC-GND YD-RP2040 firmware and desktop support while preserving the
existing ESP32-S3 input behavior. Kivo must identify every physical Device, including
multiple Devices using the same Board Profile, and operate them at the same
time. Each Device keeps an independent serial session, protocol state, action
queue, timeout, and Runtime Assignment. Adding another board on a supported MCU
extends Board Profiles; adding another MCU such as ESP32-C3 extends Controller
Families as well. Neither change reshapes Device Management or session
orchestration.

## Confirmed Hardware

The target is the VCC-GND YD-RP2040 shown in `refer/rp2040`.

- The board uses an RP2040 with a native USB-C connection.
- The Arduino-Pico and PlatformIO board identifier is
  `vccgnd_yd_rp2040`.
- The board exposes GPIO0 through GPIO22 and GPIO26 through GPIO29.
- GPIO23 drives the onboard WS2812, GPIO24 reads the user button, and GPIO25
  drives the onboard LED.
- The confirmed first Kivo Board Profile exposes exactly GPIO0 through GPIO22.
  GPIO23 through GPIO25 are reserved by onboard hardware. GPIO26 through GPIO29
  stay outside the first profile because they are labelled and routed as ADC
  pins; they can be added later through an explicit Board Profile revision.
- The connected board was observed in RP2040 ROM boot mode as `RP2 Boot`, USB
  VID/PID `2e8a:0003`, serial `E0C9125B0D9B`, with no serial port. That absence
  is expected in bootloader mode.

The YD-RP2040 schematic is the authority for board-level pin use. The existing
ESP32-S3 allowlist remains unchanged.

## USB Identities

Bootloader and running firmware are different device states and must never be
treated as interchangeable.

| Board and state | VID:PID | Host behavior |
|---|---|---|
| LuatOS ESP32S3-AIO running Kivo | `303a:4002` | Open its CDC port and require `HELLO 3 esp32s3 luatos-esp32s3-aio <build> ...` |
| YD-RP2040 ROM bootloader | `2e8a:0003` | Offer only as an UF2 upload target; never open as a runtime device |
| YD-RP2040 running Kivo | `2e8a:102e` | Open its CDC port and require `HELLO 3 rp2040 vccgnd-yd-rp2040 <build> ...` |

`2e8a:102e` is the Raspberry Pi allocation for VCC-GND YD-RP2040. The RP2040
firmware uses manufacturer `VCC-GND`, product `Kivo Keyboard RP2040`, and the
board's stable hardware serial. Desktop discovery matches the explicit VID/PID
table first, then confirms the controller and protocol with `HELLO`. Product
names are diagnostic only and are not discovery keys.

Runtime discovery separates Controller Family, Board Profile, and Device
identity. VID/PID selects a candidate Board Profile, `HELLO` confirms its
Controller Family, Board Profile, protocol, and capabilities, and a stable
Device ID distinguishes one physical unit from another.

The runtime handshake is protocol v3 only:

```text
HELLO 3 <controller-family-id> <board-profile-id> <firmware-build-id> <pin-count> <pins...>
```

Both firmware targets send this shape spontaneously and in response to
`HELLO`. The desktop accepts protocol version `3` only and verifies that both
reported IDs match the Board Profile selected by VID/PID. `HELLO 2` and older
settings or profile formats are explicitly unsupported: Kivo has not been
formally released, so this design keeps no backward-compatibility parser,
migration path, or fallback identity rule.

`firmware-build-id` is a required non-whitespace token such as
`0.1.0+gabc1234`. It identifies the exact firmware build for diagnostics and
post-upload verification but does not participate in Device ID or introduce
implicit semantic-version compatibility ranges. Compatibility remains an
explicit result of protocol version, Board Profile identity, and reported
capabilities.

The Board Profile safe-pin set is the desktop's maximum trust boundary. A
`HELLO` capability list must be non-empty, unique, in range, and a subset of
that set. An empty, duplicate, out-of-range, or extra pin rejects the handshake
as `capability_mismatch`; firmware cannot expand safe hardware capabilities by
claiming them. A valid subset may connect and enroll. Runtime configuration
then requires every bound pin to exist in both the Board Profile set and that
Device's reported subset.

Device ID is the canonical pair `Board Profile + hardware serial`, encoded
without a port path. The USB runtime descriptor must expose a non-empty stable
hardware serial, and RP2040 runtime firmware must expose the same hardware
serial used by its ROM bootloader. Replugging, port renumbering, rebooting, and
application restart do not change Device ID.

The serial-port path is ephemeral connection metadata only. A candidate with a
missing hardware serial is shown as `invalid_identity`, cannot receive a
Runtime Assignment, and never executes actions. If two connected candidates
claim the same Device ID, both are quarantined as `duplicate_identity` until
the collision is removed; Kivo does not guess which one owns the stored Runtime
Assignment.

Runtime and bootloader USB identities are two modes of the same physical
Device when their Board Profile and hardware serial match. If a known Device
re-enumerates under its Board Profile's bootloader VID/PID, Kivo keeps the same
Device record, name, and Runtime Assignment, stops only its runtime worker, and
shows Device Mode `bootloader`. Returning with a valid protocol v3 runtime
handshake changes that same row back to `runtime` and activates its assignment.

A bootloader identity whose Device ID is not already known remains an ephemeral
upload candidate. It cannot be enrolled because it has not completed a runtime
`HELLO`; missing or duplicate bootloader serials are diagnostic candidates only.

## Firmware Architecture

Keep one shared protocol and input implementation with two compile-time
platform adapters:

```text
src/
  main.cpp
  platform/
    Platform.h
    esp32s3.cpp
    rp2040.cpp

lib/gpio_trigger/
platformio.ini
```

`main.cpp` owns setup/loop control, topology commands, scanning, learning, and
action acknowledgement. It depends only on the small `Platform` API for USB
initialization, CDC input/output, HID reports, connection state, delay, and a
`BoardProfile`.

`BoardProfile` contains the Controller Family ID, Board Profile ID, and safe pin
set. The selected profile is passed into topology building, command parsing,
learning, pin-mode setup, and the protocol v3 `HELLO` response. A compile-time
firmware build ID is included in that response for both targets. Shared code no
longer references an ESP32-specific pin constant.

Each PlatformIO firmware environment compiles exactly one platform adapter.
No virtual factory or general-purpose HAL is introduced.

### ESP32-S3 adapter

The adapter wraps the existing Arduino-ESP32 `USB`, `USBCDC`, and
`USBHIDKeyboard` behavior. It preserves VID/PID `303a:4002`, Controller Family
`esp32s3`, Board Profile `luatos-esp32s3-aio`, the existing GPIO allowlist, and
current HID timing.

### RP2040 adapter

The adapter uses Arduino-Pico with Adafruit TinyUSB enabled. It exposes a
single CDC interface plus a keyboard HID interface in one composite device.
It implements paste and hotkey reports with the same protocol semantics and
acknowledgement timing as ESP32-S3. Its `HELLO` is generated from the `rp2040`
Controller Family, `vccgnd-yd-rp2040` Board Profile, and GPIO0-22 allowlist.

The PlatformIO environment uses the current Arduino-Pico integration:

```ini
platform = https://github.com/maxgerhardt/platform-raspberrypi.git#aa70b802be8851668053d4f09734e4089fe41932
board = vccgnd_yd_rp2040
framework = arduino
board_build.core = earlephilhower
```

The environment enables TinyUSB and sets the approved USB descriptor values.
The platform integration is pinned to the commit shown above, and its resolved
Arduino-Pico/toolchain packages provide a reproducible dependency set.

## Build And Upload Workflow

ESP32-S3 and RP2040 use explicit build and upload targets so a command cannot
accidentally reset or flash another Board Profile.

- RP2040 build produces `.pio/build/rp2040/firmware.uf2`.
- RP2040 upload accepts only an attached `2e8a:0003` bootloader and requires an
  explicit Device ID or hardware serial target even when only one candidate is
  present. Multiple bootloaders are never resolved by enumeration order.
- An attached `303a:4002` ESP32-S3 is ignored by the RP2040 uploader.
- ESP32-S3 download mode and upload also require an explicit Device target,
  retain their Board Profile checks, and never select an RP2040 runtime or
  bootloader.
- After UF2 transfer, verification waits for `2e8a:102e`, opens its CDC port,
  sends `HELLO`, and requires
  `HELLO 3 rp2040 vccgnd-yd-rp2040 <expected-build> ...` before reporting
  success.

Firmware build and upload remain developer-command workflows in this phase.
Device Management shows the exact Device ID, hardware serial, Board Profile,
and Runtime/Bootloader mode but exposes no flash or update action. It observes
the target's post-upload re-enumeration, validates protocol v3, and resumes a
valid persisted Runtime Assignment automatically. Board Profiles still own the
build/upload strategy contract so a later in-app updater can reuse target
validation without changing Device identity or session orchestration.

## Desktop Multi-Device Runtime

Replace the single serial worker with a coordinator plus one worker per
runtime device.

The coordinator periodically enumerates serial ports, asks the Board Profile
registry to classify candidates, and reconciles them with a registry keyed by
Device ID. It starts a worker only for a newly observed Device and removes that
worker after disconnection. A failed port open or handshake is isolated to
that Device and does not interrupt another connected Device.

Two compiled registries separate shared MCU behavior from concrete hardware:

- A Controller Family entry owns its stable family ID and shared firmware or
  desktop protocol behavior.
- A Board Profile entry owns its stable board ID, Controller Family reference,
  accepted runtime and bootloader USB identities, GPIO capabilities, firmware
  target, display name, and hardware-serial extraction rules.

Neither registry is an external plugin or user-editable data file. The
coordinator, Device registry, Runtime Assignment store, Device Management UI,
event routing, and worker lifecycle branch on neither known family nor board
names. Adding a board to an existing Controller Family adds one Board Profile
and firmware target. Adding an MCU family adds its shared adapter plus at least
one Board Profile. Neither extension changes Device records, Runtime
Assignments, snapshots, events, or Device Management.

Every worker owns its own:

- serial reader/writer;
- `DeviceSession` and `HELLO` capabilities;
- topology revision and readiness state;
- queued input actions and acknowledgement deadline;
- learning/control command queue;
- connection and last-error state.

The clipboard remains a host-global resource. A shared paste coordinator
serializes the complete transaction from writing the clipboard, through sending
that device's `PASTE`, until the matching `DONE` or timeout. A second device's
paste waits for that transaction to finish, so it cannot overwrite clipboard
content before the first host paste report consumes it. Hotkey-only actions
remain independent per device.

The coordinator assigns a monotonic host receive sequence and runs paste
transactions in global FIFO order. It never coalesces repeated requests, drops
an older request in favor of a newer one, or orders by Device type or port. A
timeout records an error against only the originating Device, releases the
global paste slot, and allows the next queued transaction to proceed. Other
Devices and non-clipboard hotkeys remain operational.

Each Device's action list is still ordered as one logical sequence. If a button
runs `Paste -> Enter`, its Enter hotkey waits for that paste transaction to
finish before the Device advances, even while independent hotkeys on another
Device execute concurrently.

Runtime events include the Device ID, Controller Family, Board Profile, port,
and both IDs from the Runtime Assignment. Metrics persist the Device ID and
Device Profile ID that handled the event, not whichever Device Profile is
currently visible in the editor or assigned later.

Metric aggregates are keyed by Device Profile, Device, and button, with the day
dimension where applicable. The Home Dashboard sums all Device contributions
for the Editor Profile by default. Device Management can filter the same data
to its selected Device. Activity history also carries Device ID and the
event-time Device name snapshot and Runtime Assignment for diagnosis. Changing
or clearing an assignment affects future events only and never rewrites
historical attribution.

## Editor Profile And Per-Device Runtime Assignment

Each Device Profile contains its visible layout, button actions, and one or
more Hardware Profiles. A Hardware Profile has its own stable ID, display name,
target Board Profile, debounce setting, input topology, and button bindings. A
Device Profile may contain multiple Hardware Profiles for one Board Profile as
well as profiles for different Board Profiles and Controller Families.

A Runtime Assignment selects exactly one Device Profile and one of its Hardware
Profiles for one Device. The Hardware Profile's Board Profile must exactly match
the Device's Board Profile. Sharing a Controller Family is insufficient because
two boards may expose different pins, USB identities, or onboard hardware.

Settings advance to schema version 2, which stores one Runtime Assignment per
Device while retaining one Editor Profile:

```text
editor_profile  Device Profile currently shown in the editor
devices         Device ID -> name, Board Profile, Runtime Assignment

Runtime Assignment:
  device_profile_id
  hardware_profile_id
```

Selecting a Device Profile in the ordinary workspace changes only the Editor
Profile. Device Management assigns a compatible Device Profile and Hardware
Profile to a specific Device. Changing one Device's Runtime Assignment never
changes another Device, including a second Device from the same Controller
Family. Pre-release schema version 1 settings are not migrated; development
data is reset or recreated in the version 2 structure.

Assignment uses the Device's exact Board Profile to filter Hardware Profiles. A
Device Profile with no compatible Hardware Profile cannot be assigned. With
exactly one compatible Hardware Profile, Device Management preselects it and
still requires the user to confirm. With multiple compatible Hardware Profiles,
the user must explicitly select one before saving. Kivo never guesses based on
pin overlap, name, creation order, or another Device's assignment.

Each persisted Device record contains its Device ID, user-editable name,
Board Profile, and optional Runtime Assignment with both profile IDs. Controller
Family is derived from the built-in Board Profile registry. The connection
port, firmware build ID, capabilities, readiness, and errors are live state and
are not persisted as identity or configuration.

A Device with no Runtime Assignment completes USB and protocol validation but
does not receive topology configuration. It reports `no_runtime_assignment`
with its Device ID while other Devices continue normally.

If either referenced profile is deleted or the selected Hardware Profile
changes to an incompatible Board Profile, Kivo retains the exact Runtime
Assignment as `invalid_assignment` so Device Management can show what must be
repaired. The Device stops immediately and receives no fallback topology. Only
an explicit assignment edit or clear operation resolves that state.

A persisted valid Runtime Assignment activates automatically on application
startup and whenever its Device reconnects. The worker validates identity,
capabilities, and both profile references. If every bound pin is present, it
sends the selected topology and marks the Device `ready` only after its matching
`CONFIG_OK`. A missing unreferenced safe pin does not prevent readiness; a
missing bound pin stops only that Device as `capability_mismatch`. Input before
readiness is skipped. Validation or configuration failure remains visible in
Device Management. Clearing the Runtime Assignment is the explicit way to
disable a Device; this phase adds no separate pause toggle or per-launch enable
step.

## Live Profile Updates

A successfully persisted Device Profile edit becomes the new runtime source of
truth automatically; users do not reconnect Devices or manually reapply an
assignment. Validation or persistence failure leaves the prior runtime state in
place.

Layout and action-only edits atomically replace the host-side mapping for every
Device assigned to that Device Profile without sending a new topology. An
action transaction already in progress completes against the old immutable
snapshot; the next accepted input resolves against the new snapshot.

Topology, binding, or debounce edits affect only Devices whose Runtime
Assignment references the changed Hardware Profile. Each connected affected
Device stops accepting new input, lets its current action transaction settle,
enters `configuring`, and receives the new topology. It returns to `ready` only
after a `CONFIG_OK` carrying the matching configuration revision. A stale
acknowledgement cannot activate a newer revision. Disconnected Devices simply
use the new configuration on their next connection.

Reconfiguration is isolated per Device. One Device may return to `ready` while
another remains in a Device-specific configuration error. Such a failure does
not roll back the persisted edit, another Device's successful configuration,
or an unrelated session.

## Snapshot, Device Management, And Learning

The application snapshot exposes a list of structured Device Status records,
the Editor Profile, and per-Device Runtime Assignments instead of the overloaded
version 1 `active_model` and one global connection. Each Device Status preserves
five independent dimensions:

- connection: `online` or `offline`;
- mode while online: `runtime` or `bootloader`;
- identity: `validating`, `valid`, or an identity problem such as
  `invalid_identity` or `duplicate_identity`;
- assignment: `unassigned`, `valid`, or `invalid_assignment`;
- runtime: `inactive`, `configuring`, `learning`, `ready`, or `runtime_error`.

The backend returns these source dimensions rather than one expanding status
enum. The frontend derives a primary row label for scanning while keeping the
underlying reason and error detail available. Attention states such as an
identity problem, invalid assignment, runtime error, or connected unassigned
Device take priority over ordinary online/offline presentation.

The top bar summarizes the whole registry instead of one port, for example
`2 ready · 1 needs attention · 1 offline`. `ready` counts only Devices whose
runtime state is `ready`; `needs attention` counts each Device or candidate with
an actionable identity, assignment, mode, or runtime problem; `offline` counts
the remaining known disconnected Devices. A bootloader-mode Device or upload
candidate needs attention and never contributes to Ready. Validation and
configuration in progress are shown separately from failures and do not make a
Device ready.

Device Management is a first-class workspace destination. It lists known
Devices whether connected or disconnected and shows Device name, Device ID,
Controller Family, Board Profile, connection state, port when connected,
firmware build ID, capabilities, Runtime Assignment, and the latest
Device-specific error. Users can name a Device, assign or clear a compatible
Device Profile and Hardware Profile, and forget a disconnected Device. The page
supports multiple Devices from one Board Profile without collapsing them into
one row or one status.

Its primary view is a dense Device list rather than a card grid. Tabs filter
All, Needs Attention, Ready, and Offline states, and one search field matches
Device name, hardware-serial text, Board Profile, and current port. Each
physical Device occupies one row with name, Board Profile, primary status,
Runtime Assignment, and port. Device ID and Controller Family remain available
in the selected Device's details without making the scanning columns wider.

Selecting one row opens a right-side detail panel for renaming, choosing the
Device Profile and Hardware Profile pair, confirming or clearing the Runtime
Assignment, inspecting reported capabilities and errors, and forgetting an
offline Device. The panel also exposes metrics and activity filtered to the
selected Device. Assignment edits are staged in the panel and saved explicitly
as one pair, so a Device never briefly receives a half-updated assignment.
Invalid-identity candidates have their own Needs Attention section and a
diagnostic-only detail view.

The first phase has no bulk assignment or bulk forget operation. Every
assignment confirmation names the one target Device, even when several Devices
share a Board Profile, preventing an accidental fan-out to same-family or
same-board hardware.

Candidates with invalid or duplicate identities appear in a separate problem
state with their Controller Family, Board Profile, and current port for
diagnosis. They cannot be named, assigned, learned from, or activated until
they present a valid, unique Device ID.

Enrollment is automatic after a previously unknown Device passes VID/PID,
unique hardware-serial, and `HELLO` validation. Kivo persists it immediately
with no Runtime Assignment and a default name formed as
`<Board Profile display name> · <last six serial characters>`. Automatic
Enrollment never activates input. A Device can execute only after the user
explicitly assigns a compatible Device Profile and Hardware Profile in Device
Management.

Forgetting is allowed only while the Device is disconnected. It removes the
Device Management record, user-edited name, and Runtime Assignment but retains
metrics and Activity History under their event-time Device ID and name snapshot.
It does not recalculate Device Profile aggregates. If the forgotten physical
Device reconnects later, it is enrolled again with the deterministic default
name and no Runtime Assignment, and its retained history becomes attributable
to that stable Device ID again. Deleting history would be a separate explicit
future operation, not a side effect of Forget Device.

The hardware editor operates on one Hardware Profile inside the Editor Profile.
Users can add, name, duplicate, and delete Hardware Profiles. Its Board Profile
selector is populated from the compiled Board Profile registry. Changing a
Hardware Profile's Board Profile revalidates its bindings against that Board
Profile and live Device capabilities when present; invalid pins remain visible
as validation errors and cannot be saved silently.

Supported GPIOs shown in the hardware editor come from the connected Device
selected for the operation intersected with the Hardware Profile's Board
Profile. With no connected Device, the editor uses the Board Profile safe set
for display and continues to require a live capability handshake before
learning.

Learning starts from a specific Device selected in Device Management or in the
hardware learning control and writes its result to the selected Hardware
Profile. The Device must be connected, its Board Profile must exactly match the
Hardware Profile, and its capabilities must include every candidate pin.
Learning commands are never broadcast or resolved by Controller Family alone.

A Learning Session is bound to the exact Device ID, Editor Profile ID, Hardware
Profile ID, and editing revision. Only that Device suspends its Runtime
Assignment, stops accepting normal input, enters `learning`, and temporarily
switches to the bounded learning topology. Other Devices, including those
assigned to the same Hardware Profile, continue using the last saved topology.

Captured signatures become editor draft changes only. Ending or cancelling the
session restores the selected Device's last saved runtime topology and does not
save, apply, or broadcast the draft. If the Device disconnects, Kivo ends the
session automatically; any captured draft remains available for explicit user
review or revert but has no runtime effect. Saving the draft invokes the normal
live-update rules and independently reconfigures every connected Device whose
Runtime Assignment references that Hardware Profile.

Physical press animation is applied only when the event's assigned Device
Profile is the Editor Profile. Events from another Runtime Assignment still
execute, update metrics, and appear in activity history without highlighting
the wrong keypad.

## Error Isolation

- A VID/PID match followed by an invalid or mismatched `HELLO` is rejected for
  that port and logged as a handshake error.
- A malformed capability list or a pin outside the Board Profile safe set is a
  handshake-level `capability_mismatch`; a valid subset connects, but missing
  bound pins stop only that Device before topology transmission.
- A Device/Hardware Profile Board Profile mismatch never sends a topology
  command.
- An invalid Runtime Assignment is retained for diagnosis but never falls back
  to another Hardware Profile.
- Disconnecting one device clears only its capabilities, learning state,
  controls, and pending actions.
- Serial write, parse, topology, or action timeout failures include the device
  identity and do not change other device sessions.
- Duplicate discovery records for the same physical key cannot start duplicate
  workers.
- Missing and duplicate hardware serials cannot consume stored Runtime
  Assignments or execute input actions.
- Bootloader devices never contribute to runtime connection counts.
- Runtime-to-bootloader re-enumeration updates one known Device's mode without
  changing its Device ID, name, or Runtime Assignment; an unknown bootloader is
  not enrolled.
- Adding a Controller Family or Board Profile cannot change existing Device IDs
  or Runtime Assignments.
- An unknown Controller Family or Board Profile cannot be introduced by editing
  settings or installing executable plugin code.

## Verification

### Native firmware tests

- The YD-RP2040 Board Profile accepts GPIO0-22 and rejects GPIO23-29 in topology
  configuration and learning; the ESP32-S3 profile retains its existing exact
  allowlist.
- Protocol parsing, topology ownership, and learning use the supplied profile
  rather than a global ESP32 allowlist.
- `HELLO` generation contains protocol v3, the correct Controller Family and
  Board Profile IDs, the compile-time firmware build ID, and the exact pin
  count.
- Existing debounce, matrix, event ordering, and acknowledgement tests remain
  green for the ESP32-S3 profile.

### Firmware builds

- Build `native`, `esp32s3`, and `rp2040` environments independently.
- Inspect the RP2040 UF2 artifact and flash the observed `RP2 Boot` device.
- Confirm runtime USB `2e8a:102e`, CDC presence,
  `HELLO 3 rp2040 vccgnd-yd-rp2040 <expected-build>`, GPIO capability list, and
  keyboard HID reports on physical hardware.
- Rebuild and hardware-smoke-test ESP32-S3 after the platform extraction.

### Rust tests

- Discovery accepts both runtime VID/PID pairs and rejects `2e8a:0003`.
- Protocol parsing requires a non-empty firmware build ID and exposes it as live
  Device detail without using it in Device ID or assignment compatibility.
- Capability parsing rejects empty, duplicate, out-of-range, and Board
  Profile-external pins, accepts a safe subset, and blocks only assignments that
  reference a missing reported pin.
- Reconciliation starts one worker per Device ID and removes only departed
  Devices.
- Device ID remains stable across port changes and application restarts.
- Runtime and bootloader VID/PID observations with the same Board Profile and
  serial reconcile to one Device record, while unknown bootloaders stay
  ephemeral.
- Missing and duplicate serial identities are quarantined without affecting
  valid Devices.
- A valid unknown Device is enrolled once with a deterministic default name and
  no Runtime Assignment.
- Forgetting an offline Device removes only its management record, name, and
  Runtime Assignment; it retains historical metrics/activity, and reconnecting
  enrolls it unassigned under the same Device ID.
- Two RP2040 Devices and two ESP32-S3 Devices can each configure different
  Device Profile and Hardware Profile pairs concurrently.
- A failure, timeout, or disconnect in one session leaves the other ready.
- Control commands and learning target one explicit Device ID.
- Learning suspends only its exact Device, produces an unapplied editor draft,
  restores the saved topology on end or disconnect, and reconfigures all
  affected Devices only after a successful save.
- Clipboard write, paste command, and acknowledgement transactions run in
  monotonic host-receive FIFO order across sessions, without coalescing or
  latest-wins replacement.
- A paste timeout releases the shared queue, reports only against its source
  Device, and preserves the remaining requests and each Device's action order.
- Action-only edits atomically update every affected host mapping without
  resending topology; an in-flight action finishes against its old snapshot.
- A Hardware Profile edit reconfigures only Devices assigned to that exact
  profile, rejects stale `CONFIG_OK` revisions, and isolates per-Device failure.
- Metrics persist event-time Device and Device Profile IDs, aggregate correctly
  across multiple Devices, and retain historical attribution after reassignment.
- Protocol v2 and pre-release settings/profile schemas are rejected rather than
  entering compatibility or migration branches.
- Full backup round-trips Device names, Board Profile IDs, and Runtime
  Assignments, metric aggregates, and bounded Activity History while excluding
  all live connection state; profile export contains no Device records,
  assignments, metrics, or history.
- Full restore switches configuration and metrics atomically and rolls both back
  after a staged activation failure.
- Restore preserves repairable `invalid_assignment` references and atomically
  rejects an unknown Board Profile or malformed or duplicate Device ID.
- A new Board Profile is discoverable and assignable without changing Device
  Management or Runtime Assignment schemas.
- Registry contract tests exercise a second board on an existing Controller
  Family and a synthetic third Controller Family without adding family- or
  board-specific branches to the coordinator or UI.
- Upload target resolution requires an exact Device ID or serial and never
  chooses between bootloaders by port or enumeration order.

### Frontend tests

- Zero, one, and many-Device connection summaries render correctly.
- Structured connection, mode, identity, assignment, and runtime dimensions
  derive stable primary labels and top-bar Ready, Needs Attention, and Offline
  counts.
- A known Device switches between Runtime and Bootloader modes in one stable
  row; an unknown bootloader appears only as an upload candidate.
- Device Management distinguishes multiple Devices from one Board Profile.
- All, Needs Attention, Ready, and Offline filters, search, and stable row
  selection work when Devices connect or disconnect during interaction.
- The detail panel stages and atomically saves one Device Profile/Hardware
  Profile pair for the explicitly selected Device; no bulk mutation is exposed.
- Assigning a Device Profile to one Device does not modify another Device's
  Runtime Assignment.
- Zero, one, and multiple compatible Hardware Profile choices enforce the
  documented disabled, preselected, and explicit-selection states.
- Deleting or making an assigned profile incompatible stops only the affected
  Device and exposes a repairable invalid assignment.
- A valid persisted Runtime Assignment automatically reaches Ready after
  reconnect and matching `CONFIG_OK`, while other Devices remain independent.
- Clearing a Runtime Assignment prevents automatic activation for that Device.
- Live profile edits show each affected Device independently transitioning
  through `configuring`, `ready`, or its own configuration error.
- Supported GPIOs and learning state follow the explicitly selected Device and
  Hardware Profile.
- Offline editing uses the Board Profile safe set, while connected editing and
  assignment validation use its intersection with reported Device capabilities.
- Learning one Device leaves every other Device on its saved topology; saving
  the captured draft then shows per-Device live reconfiguration states.
- Runtime events do not highlight a keypad belonging to another Device Profile.
- The Home Dashboard aggregates the Editor Profile across Devices, while Device
  Management filters metrics and activity to its selected Device.
- Full-backup preview includes known-Device, Runtime-Assignment, metric-row, and
  Activity History counts, while Device Profile import/export exposes no
  physical Device identity or history.

### Physical coexistence acceptance

With multiple ESP32-S3 (`303a:4002`) and YD-RP2040 Devices connected together:

1. Kivo lists every Device separately after each completes `HELLO`.
2. Each receives only the topology from its individually assigned,
   board-compatible Hardware Profile.
3. Alternating and near-simultaneous presses on all Devices execute all
   configured actions without cross-routing or dropped sessions.
4. Unplugging any Device leaves every other Device operational and keeps the
   disconnected Device's management record and Runtime Assignment.
5. Putting one RP2040 into `RP2 Boot` removes only that Device's runtime session;
   its stable Device row changes to Bootloader mode, while all ESP32-S3 and
   other RP2040 Devices remain connected and usable.

## Non-Goals

- QMK or Raw HID transport.
- Moving Paste and Hotkey execution from firmware USB HID to native host input
  injection. That deferred alternative is recorded in
  `docs/superpowers/specs/2026-07-31-serial-only-host-input-alternative.md`.
- Reusing Espressif's VID/PID for RP2040.
- Treating the UF2 bootloader as a serial device.
- Runtime-loaded or user-installable Controller Family or Board Profile
  plugins.
- In-app firmware build, flash, or update controls.
- Enabling YD-RP2040 GPIO23 through GPIO29 in the first profile.
- Reworking Device Profile layout, action semantics, or the general editor
  navigation beyond the Device attribution and filtering defined above.

## Automatic Device Enrollment

A valid but previously unknown Device is enrolled automatically after its first
successful handshake. Invalid candidates are never enrolled. Enrollment creates
a persistent Device record with a deterministic default name and no Runtime
Assignment.

## Controller Extension Boundary

Controller Families and Board Profiles are compiled into Kivo. Another board
on RP2040 adds a Board Profile and firmware target while reusing the RP2040
family adapter. A future ESP32-C3 integration adds one Controller Family
adapter and at least one Board Profile with its build/upload strategy. Neither
path adds a new Device type, assignment schema, session coordinator, or Device
Management workflow.

## Device Profile Portability Across Boards

A Device Profile owns shared layout and actions plus one or more Hardware
Profiles. Runtime Assignment selects both a Device Profile and a Hardware
Profile. The same Device Profile can therefore run concurrently on different
Board Profiles, Controller Families, or wiring variants of one Board Profile.

Version 2 Device Profile documents store `hardware_profiles` directly. Version
1 documents and imports are rejected rather than migrated because no released
format requires compatibility. Device Profile import, export, and full backup
remain self-contained and include every Hardware Profile.

## Backup And Profile Export Boundary

A full backup contains the complete persistent workspace: Editor Profile,
language and other settings, every Device Profile and Hardware Profile, every
known Device record, user-edited Device names, Board Profile IDs, and exact
Runtime Assignments. It also contains Device/Profile/button/day metric
aggregates and the bounded Activity History retained by Kivo. It excludes an
unbounded raw event stream, ephemeral ports, reported capabilities, connection
states, readiness, and runtime errors.

Full restore validates and atomically replaces that entire persistent snapshot,
including metrics and Activity History; it is not a merge. Configuration and
metrics are staged and switched as one unit, and failure rolls both back so the
application cannot expose restored assignments with old metrics or the reverse.
Historical metrics and Activity History may reference a Device ID whose current
management record was forgotten; that intentional history is not a restore
validation error.
Restored Devices initially appear disconnected and do not become ready until a
physical Device with the same Device ID reconnects and passes the current
protocol v3 validation. A restored valid Runtime Assignment then activates
normally. A deliberately retained assignment whose Device or Hardware Profile
reference is missing remains visible as `invalid_assignment`; restore never
guesses a replacement. An unknown built-in Board Profile ID or a malformed or
duplicate Device ID rejects the whole restore without changing the current
workspace.

Exporting one Device Profile includes that Device Profile and all of its
Hardware Profiles only. It excludes known Devices, hardware serials, Device
names, and Runtime Assignments so the exported profile remains portable. The
full-backup preview reports known-Device, Runtime-Assignment, metric-row, and
Activity History counts in addition to profile totals, so the user can see the
physical mappings and history affected before confirmation. Device Profile
import preview exposes no physical Device identity or history.

## Hardware Profile Selection

When Device Management selects a Device Profile for a Device, zero compatible
Hardware Profiles disables confirmation, one compatible Hardware Profile is
preselected for confirmation, and multiple compatible Hardware Profiles require
an explicit choice. The persisted Runtime Assignment always contains both exact
profile IDs. Missing or incompatible references remain visible but inactive;
Kivo never substitutes another Hardware Profile.

## Runtime Activation

A persisted valid Runtime Assignment activates automatically on application
startup and Device reconnect. The Device reaches Ready only after validation,
topology transmission, and matching `CONFIG_OK`. Clearing the Runtime Assignment
disables automatic activation. There is no separate pause toggle or per-launch
enable action in this phase.

## Controller Family And Board Identity

Controller Family and Board Profile are separate built-in identities. Protocol
v3 reports both, Device ID combines Board Profile with hardware serial, and
Hardware Profile compatibility requires an exact Board Profile match. Another
board on an existing MCU therefore reuses family behavior without inheriting
unsafe GPIO or USB assumptions from a different board.

## References

- Local board pinout and mechanical drawing: `refer/rp2040/YD-2040-PIN.png`
  and `refer/rp2040/YD-RP2040-Metric-SIZE.jpg`.
- Local schematic: `refer/rp2040/YD-2040-2022-V1.1-SCH.pdf`.
- Arduino-Pico PlatformIO and USB configuration:
  <https://arduino-pico.readthedocs.io/en/latest/platformio.html>.
- Raspberry Pi RP2040 USB PID allocations:
  <https://github.com/raspberrypi/usb-pid>.
