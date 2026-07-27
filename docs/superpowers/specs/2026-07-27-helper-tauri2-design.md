# Helper Tauri 2 Desktop App Design

## Goal

Replace the Python curses helper with a distributable macOS desktop application
built with Tauri 2, Rust, React, and TypeScript. The application keeps the
existing firmware and `PRESS`/`PASTE`/`SKIP` serial protocol unchanged while
making GPIO text mappings and live device activity available in a graphical UI.

## Scope

The first release targets macOS only. Rust replaces all Python helper behavior:
USB serial discovery, reconnects, protocol parsing, clipboard writes, mapping
persistence, and event reporting. React owns presentation and user input only.
After parity is verified, the Python helper and its tests are removed rather
than retained as a second implementation.

Firmware, GPIO behavior, and the PlatformIO build remain unchanged. The
existing supported GPIO set is `0-9` and `12-18`; GPIO0 is shown with a warning
because holding it low during startup enters download mode.

## Application Structure

The repository root becomes the frontend package and contains the Vite React
application. `src-tauri` contains the Tauri application and all helper runtime
logic.

- `src-tauri/src/config.rs` validates, loads, and atomically saves YAML mappings.
- `src-tauri/src/protocol.rs` parses button events and produces serial replies.
- `src-tauri/src/device.rs` owns one background serial worker, reconnects, writes
  clipboard text with macOS `pbcopy`, and emits runtime events through Tauri.
- `src-tauri/src/lib.rs` initializes application state and exposes the smallest
  command surface needed by React: load mappings, save mappings, and read status.
- `src/App.tsx` renders the complete application and subscribes to runtime events.
- `src/types.ts` defines the command and event payloads shared inside the
  frontend.

The background worker owns the serial port. Mappings live in an
`Arc<RwLock<...>>`, so a successful save becomes effective immediately without
restarting the worker. There is no local HTTP server and no Python sidecar.

## Configuration

The application stores `config.yaml` in the Tauri app configuration directory.
On first launch, it imports a legacy `config.yaml` from the current working
directory when present; otherwise it creates an empty mapping document. This is
the only migration path needed for the existing helper.

The schema remains compatible:

```yaml
buttons:
  6: first value
  7: |-
    first line
    second line
```

All keys must be supported GPIO numbers and all values must be strings. Empty
values are omitted when saving. Rust writes UTF-8 YAML to a temporary file in
the same directory and renames it over the destination so a failed write cannot
truncate the active configuration.

## Runtime Flow

At startup Tauri loads the mapping file, starts the serial worker, and opens the
main window. The worker scans serial devices for USB vendor ID `0x303A` and the
product name `ESP Vibe Text Keyboard`. It reconnects automatically until the
application exits.

For each valid `PRESS <event_id> <gpio>` line, the worker reads the current
mapping. A non-empty mapping is sent to `pbcopy`, followed by `PASTE <event_id>`
to the device. Missing or empty mappings receive `SKIP <event_id>`. Malformed
and non-UTF-8 input is ignored. Connection changes, accepted presses, skips,
configuration errors, and clipboard failures are emitted as typed events to
the frontend.

## Interface

The window is a quiet desktop tool rather than a marketing page. A compact
toolbar shows the product name, connection status, configuration path, and a
Save button. The main area uses the existing split workflow:

- The left side lists every supported GPIO and a one-line preview. Selecting a
  row opens its multiline text in a stable editor below the list. GPIO0 carries
  a concise boot-mode warning.
- The right side is a bounded, timestamped event log. New entries appear without
  moving or resizing the mapping editor.
- Save is enabled only when mappings differ from the last successful save.
  `Cmd+S` invokes the same action. A failed save preserves the editor contents
  and displays the error near the Save button and in the event log.

The layout stacks mappings above events on narrow windows. Native buttons,
textareas, status text, and focus outlines provide keyboard access without a UI
component dependency. The visual system uses neutral surfaces, a green
connected state, amber warnings, and red errors; it does not use gradients or
decorative cards.

## Error Handling And Shutdown

An invalid startup file does not start the serial worker with ambiguous state.
The UI opens with empty mappings and a visible configuration error that can be
corrected and saved. Serial absence and disconnects are normal states and keep
retrying. Clipboard or serial write failures are logged and cause that event to
be skipped rather than acknowledged as pasted.

Tauri's exit lifecycle signals the worker and joins it before process shutdown.
Only one worker and one serial connection exist for the application lifetime.

## Verification

Rust unit tests cover YAML validation and atomic replacement, protocol parsing,
mapped and unmapped responses, malformed input, and device filtering. React
tests cover initial loading, mapping edits, dirty/save state, save errors,
runtime events, and the GPIO0 warning. The production frontend build and
`cargo test` must pass.

A Tauri development smoke test must prove that the real desktop window renders,
loads the migrated configuration, edits and saves a multiline Unicode mapping,
and exits cleanly. Physical-device verification must prove reconnect behavior,
one mapped button producing `PASTE`, one unmapped button producing `SKIP`, and
the mapped Unicode text reaching the macOS clipboard.
