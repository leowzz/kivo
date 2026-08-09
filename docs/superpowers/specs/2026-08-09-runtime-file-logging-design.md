# Runtime File Logging Design

**Date:** 2026-08-09

## Goal

Add bounded, persistent diagnostic logging to Kivo's runtime data directory. The
logs must cover application lifecycle, device processing, every physical input,
every action step, configuration operations, and failures without recording
pasted text or other sensitive action payloads.

## Storage And Rotation

Use the official `tauri-plugin-log` crate, version 2.9.0, as the file writer and
rotation implementation. During Tauri setup, resolve the existing application
configuration directory and register a `TargetKind::Folder` target at:

```text
<app_config_dir>/data/log/
```

The active file is `kivo.log`. Configure a maximum file size of 10 MiB and
`RotationStrategy::KeepSome(5)`. The plugin counts the active file within this
limit, so the directory contains at most five Kivo log files. The plugin creates
the directory when needed and appends to the current file across launches.

Register the plugin after the application data path is available in `setup`.
Plugin registration failure is reported to stderr but does not prevent the
workspace, device runtime, or UI from starting.

## Format

Write one JSON object per line. Clear the plugin's default text formatter for the
Kivo file target so each line remains independently parseable. Each entry has a
small common envelope:

- `timestampMs`: Unix timestamp in milliseconds.
- `level`: `info`, `warning`, or `error`.
- `event`: stable snake-case event code.
- `result`: `started`, `succeeded`, `failed`, or omitted when not applicable.
- `detail`: optional diagnostic error code or non-sensitive detail.
- `context`: optional device, profile, hardware, port, button, input, and action
  fields relevant to the event.

The log crate target is restricted to Kivo's explicit runtime logging target so
unstructured dependency messages do not contaminate the JSON Lines file.

## Event Sources

### Application Lifecycle

Record application startup, runtime readiness, exit request, and clean shutdown.
Record failures while initializing logging, the workspace, metrics storage, and
other runtime services when the failure can be observed without changing their
existing behavior.

### Devices And Runtime

Record device scan failures and device state transitions, including connection,
disconnection, identity validation, protocol validation, topology activation or
rejection, learning state, and runtime errors. Avoid logging every successful
500 ms scan when the observed state is unchanged.

The existing `RuntimeEvent` is the canonical event source. Enrich each event
with its device, board, port, Device Profile, and Hardware Profile context, then
write it to the file log and emit it to the frontend. Logging remains outside
the coordinator's state transitions and metrics transactions.

### Inputs And Actions

Record every physical input state emitted by the runtime, including press and
release, physical GPIO/contact input, mapped button, device identity, and the
captured profile context.

Add explicit action lifecycle activities around each action step:

- `action_step_started` when a step is issued or submitted to the host.
- `action_step_completed` when the matching device acknowledgement completes.
- Existing action failure and timeout activities retain their current codes and
  are logged at warning or error level.

Action entries include the button ID, event ID, step number, total step count,
action kind, and safe action metadata. Paste actions record only the Unicode
character count. Open actions record only target type and character count.
Hotkey actions may record normalized keys, media actions their command, and
delay actions their duration. Pasted text, URL query data, and local paths must
never enter `RuntimeActivity`, `RuntimeEvent`, log messages, or error details.

### Configuration Operations

Record successful and failed Device Profile create/save/import/export/delete
operations, settings changes, device rename/setup/forget operations, Runtime
Assignment changes, learning begin/end, and backup import/export. Entries use
stable IDs and result codes; they do not serialize full profiles or backup
contents.

## Failure Isolation

File logging is diagnostic and never participates in device control, action
acknowledgement, workspace mutation, or metrics persistence. A serialization or
file logging failure must not change an operation's returned result. Where the
logging plugin cannot expose a file-write error to the caller, retain normal
runtime behavior and rely on its stderr target during development.

Do not add raw serial byte logging. The structured protocol and runtime events
provide the required diagnostic coverage without producing unbounded noise or
capturing unexpected device payloads.

## Testing

Implementation follows test-driven development:

1. Add failing unit tests for the JSON record envelope and action metadata
   sanitization. Include non-ASCII paste text to prove the recorded value is a
   Unicode character count and the text is absent.
2. Add failing device-session tests for action start and completion activities,
   including paste success and failure.
3. Add failing tests for lifecycle, device transition, scan error, and command
   result records at their integration boundaries.
4. Add a plugin integration test using a temporary folder and a small maximum
   size. Produce enough entries to rotate and assert that the number of matching
   files does not exceed the configured `KeepSome` limit.
5. Run Rust tests, `cargo fmt --check`, and Clippy. Run the repository's broader
   verification when the implementation touches shared frontend contracts.

Existing frontend `RuntimeEvent` fields remain compatible. New action activity
codes flow through the existing event channel and require no UI behavior.
