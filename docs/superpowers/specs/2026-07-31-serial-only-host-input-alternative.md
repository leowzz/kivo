# Serial-Only Host Input Alternative

Date: 2026-07-31
Status: Deferred alternative

## Purpose

Record a possible future architecture in which controller firmware exposes USB
CDC serial only and Kivo executes paste and hotkey actions through native host
input APIs. This is not part of the RP2040 parallel-device delivery. The current
design remains USB CDC for Kivo protocol traffic plus USB HID for keyboard
reports.

## Motivation

Moving action execution to the desktop would make controller firmware
responsible only for GPIO scanning, learning, topology validation, and serial
events. ESP32-S3, RP2040, future ESP32-C3, and later Controller Families could
share a smaller platform boundary without implementing keyboard HID reports.

This alternative does not eliminate USB. Runtime devices still use native USB
CDC and retain their Board Profile VID/PID, hardware serial, protocol identity,
Device ID, and independent serial session.

## Proposed Data Flow

```text
Device input
  -> USB CDC event with Device ID session context
  -> Kivo resolves Runtime Assignment and ordered actions
  -> host clipboard/input backend executes Paste or Hotkey
  -> Kivo records completion or failure against that Device
```

Firmware would no longer receive `PASTE` or `HOTKEY` commands and would not
emit keyboard HID reports. The exact event acknowledgement replacement must be
designed before implementation so firmware backpressure, event ordering, and
timeouts remain explicit rather than becoming fire-and-forget behavior.

## Desktop Boundary

Kivo would introduce a host input backend behind one platform-neutral
interface:

- write clipboard text;
- press and release a normalized hotkey;
- report permission availability;
- distinguish unsupported input from a transient execution failure;
- guarantee modifier release after partial failure.

The existing global clipboard coordinator would remain. Paste transactions from
all Devices would execute in monotonic host-receive FIFO order, while unrelated
hotkeys could remain independent subject to each Device's action ordering.

On macOS, native input injection requires Accessibility permission. Kivo could
still discover Devices, edit Device Profiles, configure Hardware Profiles, and
learn inputs without that permission, but it could not execute user actions.
Permission state would therefore be a host-level runtime prerequisite, not a
Device-specific error.

Windows and Linux would require separate input backends. Linux Wayland support,
secure-input fields, sandboxed applications, remote desktops, and applications
that reject synthetic events require explicit product and compatibility tests;
they cannot be assumed equivalent to physical USB HID input.

## What Remains Unchanged

- Controller Family, Board Profile, Device, and Device ID concepts.
- Multi-Device discovery, Enrollment, Runtime Assignment, and Device
  Management.
- One serial worker and ordered event queue per Device.
- Hardware Profile compatibility and topology configuration.
- Bootloader correlation and explicit upload targeting.
- Device Profile import/export and full-backup boundaries.

## Benefits

- Smaller firmware platform adapters and USB descriptors.
- One host-owned action engine for all Controller Families.
- No firmware-specific HID keycode translation or report timing.
- Easier action types that depend on host services rather than keyboard HID.

## Costs And Risks

- Mandatory OS input-control permission, beginning with macOS Accessibility.
- Kivo must remain running and authorized for any action to execute.
- Platform-specific implementation and QA for every supported desktop OS.
- Synthetic input may behave differently from a physical keyboard in secure or
  restricted applications.
- A host input backend becomes security-sensitive code with broader privileges.
- Protocol acknowledgement and failure semantics must be redesigned.

## Evaluation Gate

Reconsider this alternative only after a focused prototype demonstrates:

1. Reliable modifier press/release and paste behavior across representative
   macOS applications.
2. Clear Accessibility permission detection, recovery, and revocation behavior.
3. Equivalent per-Device ordering, timeout isolation, and multi-Device latency.
4. Defined behavior for secure input and applications that reject synthetic
   events.
5. A credible Windows and Linux strategy if those platforms enter scope.

Until those checks pass and the permission tradeoff is accepted, USB HID
remains the production action-execution path.
