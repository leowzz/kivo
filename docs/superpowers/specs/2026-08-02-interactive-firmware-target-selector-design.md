# Interactive Firmware Target Selector Design

**Date:** 2026-08-02

## Context

The named firmware upload targets currently require operators to discover a
hardware serial separately and pass it as `SERIAL=<hardware serial>`. Running
`make upload-rp2040` without that variable exits before showing the devices
that are available. The repository already has a read-only inventory script
that identifies supported runtime and bootloader USB devices.

## Goals

- Preserve the existing explicit `SERIAL=...` upload path.
- When `SERIAL` is absent, open an interactive, target-specific device picker.
- Continuously refresh the inventory while the picker is open.
- Require explicit confirmation even when only one compatible device exists.
- Show when a device was first observed during the picker session.
- Keep device selection separate from building, flashing, and verification.

## Non-Goals

- Do not auto-select a device.
- Do not infer or manufacture a serial for a device that does not expose one.
- Do not change the firmware upload or post-upload verification protocols.
- Do not persist connection history across picker invocations.
- Do not attempt to reconstruct historical USB insertion times from macOS
  logs. That data is not reliably available for devices connected before the
  picker starts.

## User Experience

`make upload-rp2040` and `make upload-esp32s3` keep accepting an explicit
`SERIAL`. When it is omitted, Make launches a full-screen picker built with
`prompt-toolkit`.

The picker refreshes the inventory once per second. Each row shows the device
mode, Board Profile, hardware serial, port, and connection time. Devices found
by the initial scan display `connected before picker started`. A device first
seen by a later scan displays its local first-seen time as `HH:MM:SS`. Removing
a device removes its row. If the selected row survives a refresh, selection
stays on that identity; otherwise it moves to the nearest selectable row.

The controls are:

- Up/Down or `j`/`k`: move selection.
- Enter: confirm the selected device.
- `r`: request an immediate refresh.
- `q` or Escape: cancel without building or flashing.

Rows without a hardware serial remain visible for diagnosis but are disabled
and state why they cannot be selected. Rows that match the Board Profile but
are in a mode unsupported by that upload flow are also visible and disabled.
An empty inventory remains on screen and continues scanning so an operator can
plug in a device without restarting the command.

The RP2040 upload picker accepts serial-bearing runtime and bootloader rows for
`vccgnd-yd-rp2040`. The ESP32-S3 picker selects runtime rows for
`luatos-esp32s3-aio`, because the existing flow must open the runtime serial
port to enter download mode; matching bootloader rows are shown but disabled.

## Architecture

### Inventory

`scripts/list_firmware_targets.py` remains the source of recognized USB
observations. Its existing CDC and macOS UF2 discovery functions are reused;
the selector does not add a second USB enumeration implementation.

### Tracking Model

A small target-tracking model receives successive inventory snapshots and a
clock value. It owns:

- stable row identity;
- whether a row existed in the initial snapshot;
- the first-seen time for rows added later;
- selectable/disabled status and its reason;
- stable selection across refreshes.

Rows use Board Profile, serial, mode, and concrete port as their observation
identity. Anonymous rows retain the same shape with a missing serial. The model
contains no terminal or Make-specific behavior so snapshot transitions can be
tested deterministically. If multiple current observations expose the same
serial for one Board Profile, every conflicting row is disabled because the
downstream upload command cannot target one of them unambiguously.

### Terminal UI

A new selector script composes the tracker with a `prompt-toolkit` full-screen
application. A background refresh task scans once per second, and the `r` key
wakes it immediately. Rendering reads immutable tracker output. The interface
uses stdin for keys and stderr for display because stdout is reserved for the
selected serial.

On confirmation, the application restores the terminal and writes exactly one
line containing the selected hardware serial to stdout. Cancellation exits
nonzero without writing a serial. If stdin or stderr is not a TTY and `SERIAL`
was not supplied, the selector fails immediately with an instruction to pass
`SERIAL=...`; it never waits for input in CI or redirected commands.

### Make Integration

Each public upload target first resolves a serial. A non-empty `SERIAL` is used
unchanged and bypasses the selector. Otherwise Make runs the selector with the
Board Profile and allowed modes for that upload target and captures its stdout.

Only after successful confirmation does the target invoke its existing build,
upload, reset, and runtime verification commands with the selected serial. A
selector failure or cancellation stops the recipe before any build or upload.
The generic `upload` target continues to require choosing a named family
target.

`prompt-toolkit` is added to the Python development dependencies used by the
existing `uv run` tooling.

## Error Handling

- Inventory errors remain visible in the picker while later refreshes retry.
- A device disappearing before Enter cannot be confirmed.
- Duplicate observations are reconciled by the existing inventory merge logic.
- Multiple concrete devices with the same serial remain separate, disabled
  rows and are not silently collapsed or passed to an ambiguous upload command.
- A selected device disappearing after confirmation is handled by the existing
  upload command, which fails rather than targeting a substitute.
- Keyboard interruption, `q`, and Escape restore the terminal and exit before
  the build starts.

## Testing

Implementation follows test-driven development.

Unit tests use fake snapshots and a fake clock to cover initial devices, later
first-seen timestamps, removal and reappearance, stable selection, disabled
anonymous rows, mode restrictions, and duplicate serials on distinct ports.

Selector tests use fake inventory and terminal boundaries to cover explicit
selection, cancellation, empty inventories, immediate refresh, and non-TTY
failure. Rendering tests assert meaningful labels without relying on terminal
escape sequences or wall-clock timing.

Makefile contract tests verify that explicit serials bypass the selector, both
named upload targets pass the correct Board Profile and mode policy, selector
failure prevents the build, and successful selection feeds the original upload
and verification commands. No automated test flashes physical hardware.

## Acceptance Criteria

- `make upload-rp2040` opens the live picker when `SERIAL` is absent.
- A currently connected RP2040 is visible with `connected before picker
  started`, and the operator must press Enter to choose it.
- An RP2040 connected while the picker is open appears without restarting the
  command and shows its first-seen local time.
- The picker never chooses a device automatically.
- `make upload-rp2040 SERIAL=<serial>` behaves as before and launches no UI.
- Cancellation and non-interactive invocation perform no build or flash.
- The selected serial is used for both flashing and runtime verification.
