# RP2040 Parallel Firmware Handoff

Date: 2026-07-30
Status: Architecture direction approved; target board not selected

## Goal

Add RP2040 support while retaining ESP32-S3 support. Keep both firmware targets
in this repository and preserve the existing Kivo desktop workflow and serial
protocol.

## Decisions

- RP2040 uses Arduino-Pico with TinyUSB, not QMK.
- ESP32-S3 and RP2040 remain parallel, supported targets.
- Keep one repository and one shared protocol/input state machine.
- Put USB/HID and board capabilities behind two small compile-time platform
  implementations. Do not split the firmware into separate repositories or
  spread platform conditionals through the shared control flow.
- Keep arbitrary UTF-8 paste in the Kivo desktop path: the desktop writes the
  clipboard and firmware sends the paste shortcut. Standalone firmware can
  execute shortcuts and key sequences, but a USB keyboard cannot portably set
  the host clipboard or type arbitrary Unicode without host-specific setup.

QMK was rejected because it would introduce a second firmware architecture and
Raw HID transport for RP2040 while ESP32-S3 remains on Arduino. It is useful for
a conventional standalone keyboard, but it does not reduce the work for Kivo's
runtime topology, learning, and desktop-driven Unicode paste behavior.

## Approved Code Boundary

```text
src/
  main.cpp                    shared setup/loop, scanning, protocol handling
  platform/
    Platform.h                declarations and BoardProfile
    esp32s3.cpp               Arduino-ESP32 CDC/HID implementation
    rp2040.cpp                Arduino-Pico + TinyUSB implementation

lib/gpio_trigger/             shared topology, debounce, learning, event state
platformio.ini                native, esp32s3, and rp2040 environments
```

`Platform.h` should expose only operations already required by `main.cpp`, such
as USB initialization, CDC reads/writes, connection state, HID reports, and a
board profile containing the platform ID and safe GPIO allowlist. Each
PlatformIO firmware environment compiles exactly one platform implementation;
no virtual factory or general-purpose HAL is needed.

The shared code should generate `HELLO` from the selected board profile. Pass
the same profile into protocol/topology validation and learning instead of
referencing `kEsp32S3SafePins` from shared classes.

## Current Code Constraints

- `platformio.ini` defines only the `esp32s3` firmware environment.
- `src/main.cpp` directly uses Arduino-ESP32 `USB`, `USBCDC`, and
  `USBHIDKeyboard`, and hard-codes the ESP32-S3 `HELLO` response.
- `lib/gpio_trigger` validates topology, protocol pin lists, and learning
  against `kEsp32S3SafePins` in several files.
- `src-tauri/src/device.rs` discovers one fixed USB VID/PID pair. Parallel
  support needs an explicit set of accepted device identifiers, followed by the
  existing `HELLO` platform/protocol validation.
- Model hardware already has a controller ID, and the device session already
  rejects controller and GPIO mismatches. Preserve that boundary.

The earlier assessment at
`docs/rp2040-compatibility-assessment.md` remains useful for hardware and USB
constraints, but some host-helper references predate the current Tauri runtime.
Use the current Rust device path as authoritative during implementation.

## Still To Decide

1. Exact RP2040 board: Raspberry Pi Pico, Pico W, or a third-party/custom board.
2. Board-specific safe GPIO allowlist and physical pin mapping.
3. Arduino-Pico PlatformIO board/platform settings and UF2 upload workflow.
4. RP2040 USB VID/PID and product strings. Do not reuse Espressif's VID merely
   to avoid changing desktop discovery.

## Next Session

1. Select the exact RP2040 board and verify its Arduino-Pico/TinyUSB support.
2. Finish the design with the GPIO allowlist, USB identity, build/upload flow,
   error handling, and focused test matrix.
3. Write an implementation plan only after that design is approved.
4. Implement the board-profile refactor first, keeping ESP32-S3 behavior green;
   then add the RP2040 platform implementation and physical-device checks.

## Suggested Skills

- `superpowers:brainstorming`: finish the board-specific design and approval.
- `superpowers:writing-plans`: turn the approved design into implementation
  steps.
- `superpowers:test-driven-development`: preserve ESP32 behavior while making
  shared validation board-aware.
- `superpowers:verification-before-completion`: run native, ESP32-S3, RP2040,
  Rust, and frontend checks before claiming support is complete.
