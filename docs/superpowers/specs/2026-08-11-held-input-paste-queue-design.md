# Held Input Paste Queue Design

Date: 2026-08-11
Status: Approved

## Goal

Allow one Kivo device to keep a physical input pressed indefinitely without
blocking Paste or later actions on another device.

Kivo must continue to serialize clipboard mutations and firmware Paste reports,
but only input sequences that have submitted an actual Paste request may
participate in Paste scheduling.

## Observed Failure

The coordinator registers every physical input edge with the global
`PasteCoordinator`. Protocol v6 keeps a Down sequence unfinished until the
matching Up edge settles its release placeholder. This is correct for trigger
tracking, because Release may be configured and a held input may remain active
for an arbitrary duration.

The Paste coordinator currently examines only the lowest registered receive
sequence. When that sequence is unfinished and contains no Paste request, it
returns without considering later sequences. A permanently held input can
therefore block every later Paste globally.

Once another device begins a Paste action, its device-local action sequence
waits for the global Paste grant. Later Hotkey actions on that same device then
wait behind the blocked Paste, making both Paste and ordinary shortcuts appear
unavailable.

This is a valid operating state, not a stuck-key error. One connected device is
expected to keep an input pressed indefinitely.

## Confirmed Semantics

- A physical input that is still held does not reserve the global clipboard.
- Only sequences containing a submitted Paste request participate in Paste
  selection.
- Among Paste requests that are ready for selection, the lowest host receive
  sequence runs first.
- An already active Paste is never preempted.
- A Paste submitted later by an older sequence may run next according to its
  receive sequence, but it cannot retroactively reorder a Paste that already
  started.
- Per-device action order remains unchanged. `Paste -> Enter` still waits for
  Paste completion before sending Enter.
- Hotkey-only actions continue to bypass the global Paste coordinator.

## Design

Keep the current `SequenceQueue` registry and request data model. Change only
the Paste coordinator's next-request selection.

When no Paste is active, the coordinator will:

1. Remove finished sequences that contain no requests.
2. Scan registered sequences in receive-sequence order.
3. Select the first sequence containing a queued Paste request.
4. Ignore unfinished sequences that currently contain no request.
5. Return without work only when no registered sequence contains a request.

An ignored unfinished sequence remains registered. It may later receive a
Paste request from a delayed Long Press or Release trigger, or be marked
finished when the input lifecycle settles. Device cancellation and workspace
replacement continue to remove their owned sequences through the existing
cleanup paths.

No new timeout is added for an empty sequence. Empty sequences no longer hold a
resource, so timing them out would incorrectly classify a legitimate held input
as a failure.

## Paste Transaction Safety

The selected Paste transaction keeps the existing critical section:

1. Write the selected text to the host clipboard.
2. Grant that device permission to send its firmware `PASTE` command.
3. Hold the global Paste slot until matching `DONE`, cancellation, clipboard
   failure, or the existing 1800 ms timeout.
4. Select the next queued Paste request.

Skipping an empty sequence cannot allow clipboard writes to overlap. At most
one `ActivePaste` continues to exist, and the next selection still occurs only
after the active transaction leaves that state.

## Error Handling And Cleanup

- Clipboard write failure keeps the existing device-local failure behavior and
  cancels the affected sequence.
- An active Paste timeout keeps releasing the global slot and reporting only
  against its source device.
- Disconnecting a device keeps cancelling its active and queued Paste requests
  and finishing its owned receive sequences.
- A held input is not logged or surfaced as an error merely because it has no Up
  edge.
- The unrelated `trigger_occurred` log-level classification is outside this
  change.

## Verification

### Paste coordinator tests

- Register unfinished sequence 1 with no request and sequence 2 with a Paste
  request. Sequence 2 must receive `Granted` immediately.
- Complete sequence 2, then submit a Paste request for the still-unfinished
  sequence 1. That request must execute normally.
- Preserve existing FIFO ordering when multiple sequences already contain
  Paste requests.
- Preserve the active Paste timeout and cancellation behavior.

### Multi-device runtime test

Use two protocol v6 device workers:

1. Device A sends Down for a Hotkey-only input and never sends Up.
2. Device A's Hotkey completes.
3. Device B sends Down for a `Paste -> Hotkey` action.
4. Device B must receive its `PASTE` command without Device A releasing.
5. After matching `DONE`, Device B must receive its following Hotkey command.
6. Both device workers remain ready and no action timeout is recorded.

### Regression verification

- Run focused Paste coordinator and parallel-device tests.
- Run the complete Rust test suite.
- Run `git diff --check`.
- Physical acceptance holds a key on one connected RP2040 while another RP2040
  executes Paste followed by a shortcut. Automated tests do not replace this
  physical acceptance.

## Non-Goals

- Changing firmware or the serial protocol.
- Changing Device Profile or action configuration formats.
- Adding a maximum held-input duration.
- Allowing concurrent clipboard writes.
- Reordering or preempting an active Paste transaction.
- Fixing unrelated runtime-log severity classifications.
