# RP2040 Parallel Device Support Design

Date: 2026-07-31
Status: Approved for implementation planning

## Goal

Add VCC-GND YD-RP2040 firmware and desktop support without changing the
existing ESP32-S3 behavior. Kivo must distinguish the two controller families
at the physical-device boundary and operate an ESP32-S3 and an RP2040 at the
same time. Each device keeps an independent serial session, protocol state,
action queue, timeout, and model assignment.

## Confirmed Hardware

The target is the VCC-GND YD-RP2040 shown in `refer/rp2040`.

- The board uses an RP2040 with a native USB-C connection.
- The Arduino-Pico and PlatformIO board identifier is
  `vccgnd_yd_rp2040`.
- The board exposes GPIO0 through GPIO22 and GPIO26 through GPIO29.
- GPIO23 drives the onboard WS2812, GPIO24 reads the user button, and GPIO25
  drives the onboard LED.
- The first Kivo profile conservatively exposes GPIO0 through GPIO22. GPIO23
  through GPIO25 are reserved by onboard hardware. GPIO26 through GPIO29 stay
  outside the first profile because they are labelled and routed as ADC pins;
  they can be added later through an explicit board-profile revision.
- The connected board was observed in RP2040 ROM boot mode as `RP2 Boot`, USB
  VID/PID `2e8a:0003`, serial `E0C9125B0D9B`, with no serial port. That absence
  is expected in bootloader mode.

The YD-RP2040 schematic is the authority for board-level pin use. The existing
ESP32-S3 allowlist remains unchanged.

## USB Identities

Bootloader and running firmware are different device states and must never be
treated as interchangeable.

| Controller and state | VID:PID | Host behavior |
|---|---|---|
| ESP32-S3 running Kivo | `303a:4002` | Open its CDC port and require `HELLO 2 esp32s3 ...` |
| YD-RP2040 ROM bootloader | `2e8a:0003` | Offer only as an UF2 upload target; never open as a runtime device |
| YD-RP2040 running Kivo | `2e8a:102e` | Open its CDC port and require `HELLO 2 rp2040 ...` |

`2e8a:102e` is the Raspberry Pi allocation for VCC-GND YD-RP2040. The RP2040
firmware uses manufacturer `VCC-GND`, product `Kivo Keyboard RP2040`, and the
serial string supplied by the board/core when available. Desktop discovery
matches the explicit VID/PID table first, then confirms the controller and
protocol with `HELLO`. Product names are diagnostic only and are not discovery
keys.

The runtime device key contains controller family, VID, PID, serial when
available, and serial-port path. The port path disambiguates concurrent
devices when a USB serial string is absent. It is not persisted as a long-term
hardware assignment.

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

`BoardProfile` contains the controller ID and safe pin set. The selected
profile is passed into topology building, command parsing, learning, pin-mode
setup, and the `HELLO` response. Shared code no longer references an
ESP32-specific pin constant.

Each PlatformIO firmware environment compiles exactly one platform adapter.
No virtual factory or general-purpose HAL is introduced.

### ESP32-S3 adapter

The adapter wraps the existing Arduino-ESP32 `USB`, `USBCDC`, and
`USBHIDKeyboard` behavior. It preserves VID/PID `303a:4002`, the `esp32s3`
controller ID, the existing GPIO allowlist, and current HID timing.

### RP2040 adapter

The adapter uses Arduino-Pico with Adafruit TinyUSB enabled. It exposes a
single CDC interface plus a keyboard HID interface in one composite device.
It implements paste and hotkey reports with the same protocol semantics and
acknowledgement timing as ESP32-S3. Its `HELLO` is generated from the `rp2040`
profile and GPIO0-22 allowlist.

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

The existing default helper and ESP32-S3 commands remain compatible. RP2040
gets explicit build and upload targets so a command cannot accidentally reset
or flash the other controller family.

- RP2040 build produces `.pio/build/rp2040/firmware.uf2`.
- RP2040 upload accepts only an attached `2e8a:0003` bootloader. If more than
  one RP2040 bootloader is present, the command requires a serial selection and
  exits without writing.
- An attached `303a:4002` ESP32-S3 is ignored by the RP2040 uploader.
- ESP32-S3 download mode and upload continue to use their existing device
  checks and never select an RP2040 runtime or bootloader.
- After UF2 transfer, verification waits for `2e8a:102e`, opens its CDC port,
  sends `HELLO`, and requires platform `rp2040` before reporting success.

## Desktop Multi-Device Runtime

Replace the single serial worker with a coordinator plus one worker per
runtime device.

The coordinator periodically enumerates serial ports, applies the accepted
runtime VID/PID table, and reconciles it with a registry keyed by physical
device identity. It starts a worker only for a newly observed device and
removes that worker after disconnection. A failed port open or handshake is
isolated to that device and does not interrupt another connected device.

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

Runtime events include the device key, controller, port, and assigned model ID.
Metrics are persisted against the model that handled the event, not whichever
model is currently visible in the editor.

## Model Assignment

Controller identity remains part of each model's hardware configuration. A
model with controller `esp32s3` can never configure an `rp2040` session, and
the reverse is also rejected.

Settings advance to schema version 2, which stores an active model per
controller while retaining one editor selection:

```text
active_model                 model currently shown in the editor
active_models_by_controller  controller ID -> runtime model ID
```

Selecting a model in the UI updates both the editor selection and that model's
controller assignment. The other controller assignments remain active. On
migration, the existing `active_model` is assigned to its declared controller;
other controllers remain unassigned until the user selects a compatible
model. This prevents silently choosing the wrong model when multiple models
use the same controller.

A device with no assigned model completes USB and protocol validation but does
not receive topology configuration. It reports `no_active_model` for its
controller while other devices continue normally.

## Snapshot, UI, And Learning

The application snapshot exposes a list of device statuses instead of one
global connection. The top bar shows searching when the list is empty, the
single port for one device, and a connected-device count when multiple devices
are ready. Device-specific detail stays compact and uses the existing runtime
activity surface; this phase does not add a device-management page.

The hardware controller selector exposes both `ESP32-S3` and `RP2040`. Changing
a model's controller revalidates its bindings against that controller's known
profile and the live capabilities when present; invalid pins remain visible as
validation errors and cannot be saved silently.

Supported GPIOs shown in the hardware editor come from the connected device
matching the visible model's controller. With no matching device, the editor
uses the known board profile only for display and continues to require a live
capability handshake before learning.

Learning targets the worker whose controller matches the visible model. If no
matching device exists it returns `device_not_connected`. If multiple devices
of that controller are connected, it returns `ambiguous_controller` instead of
broadcasting electrical-learning commands. Normal runtime actions may still
run independently on multiple devices.

Physical press animation is applied only when the event's assigned model is
the visible model. Events from the other simultaneously active model still
execute, update metrics, and appear in activity history without highlighting
the wrong keypad.

## Error Isolation

- A VID/PID match followed by an invalid or mismatched `HELLO` is rejected for
  that port and logged as a handshake error.
- A controller/model mismatch never sends a topology command.
- Disconnecting one device clears only its capabilities, learning state,
  controls, and pending actions.
- Serial write, parse, topology, or action timeout failures include the device
  identity and do not change other device sessions.
- Duplicate discovery records for the same physical key cannot start duplicate
  workers.
- Bootloader devices never contribute to runtime connection counts.

## Verification

### Native firmware tests

- ESP32-S3 and RP2040 profiles accept their own safe pins and reject reserved
  pins.
- Protocol parsing, topology ownership, and learning use the supplied profile
  rather than a global ESP32 allowlist.
- `HELLO` generation contains the correct platform and exact pin count.
- Existing debounce, matrix, event ordering, and acknowledgement tests remain
  green for the ESP32-S3 profile.

### Firmware builds

- Build `native`, `esp32s3`, and `rp2040` environments independently.
- Inspect the RP2040 UF2 artifact and flash the observed `RP2 Boot` device.
- Confirm runtime USB `2e8a:102e`, CDC presence, `HELLO 2 rp2040`, GPIO
  capability list, and keyboard HID reports on physical hardware.
- Rebuild and hardware-smoke-test ESP32-S3 after the platform extraction.

### Rust tests

- Discovery accepts both runtime VID/PID pairs and rejects `2e8a:0003`.
- Reconciliation starts one worker per physical key and removes only departed
  devices.
- ESP32-S3 and RP2040 sessions configure different matching models
  concurrently.
- A failure, timeout, or disconnect in one session leaves the other ready.
- Control commands target one resolved device; ambiguous learning is rejected.
- Clipboard write, paste command, and acknowledgement transactions are
  serialized across sessions.
- Settings migration preserves the existing active model and assigns it to its
  controller.

### Frontend tests

- Zero, one, and two-device connection summaries render correctly.
- Selecting an RP2040 model does not remove the ESP32-S3 runtime assignment.
- Supported GPIOs and learning state follow the visible model's controller.
- Runtime events do not highlight a keypad belonging to another model.

### Physical coexistence acceptance

With the existing ESP32-S3 (`303a:4002`) and YD-RP2040 connected together:

1. Kivo lists two runtime devices after both complete `HELLO`.
2. Each receives only its controller-compatible topology.
3. Alternating and near-simultaneous presses on both devices execute all
   configured actions without cross-routing or dropped sessions.
4. Unplugging either device leaves the other operational.
5. Putting the RP2040 into `RP2 Boot` removes only the RP2040 runtime session;
   ESP32-S3 remains connected and usable.

## Non-Goals

- QMK or Raw HID transport.
- Reusing Espressif's VID/PID for RP2040.
- Treating the UF2 bootloader as a serial device.
- Persistent assignment among multiple identical devices of one controller.
- Enabling YD-RP2040 GPIO23 through GPIO29 in the first profile.
- Reworking model layout, action semantics, metrics presentation, or the
  general editor navigation.

## References

- Local board pinout and mechanical drawing: `refer/rp2040/YD-2040-PIN.png`
  and `refer/rp2040/YD-RP2040-Metric-SIZE.jpg`.
- Local schematic: `refer/rp2040/YD-2040-2022-V1.1-SCH.pdf`.
- Arduino-Pico PlatformIO and USB configuration:
  <https://arduino-pico.readthedocs.io/en/latest/platformio.html>.
- Raspberry Pi RP2040 USB PID allocations:
  <https://github.com/raspberrypi/usb-pid>.
