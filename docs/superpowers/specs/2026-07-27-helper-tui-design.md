# Helper TUI Design

## Goal

Replace the helper's scrolling console output with a terminal UI that keeps the
GPIO text mappings editable while the helper listens for real button presses.
Saving replaces the selected YAML configuration file and takes effect without
restarting the helper.

## Interface

The terminal is split into two panes:

- The left pane lists every supported GPIO and its current mapped text. The
  selected row is highlighted. Arrow keys or `j`/`k` move the selection.
- The right pane is a bounded event log. It shows connection changes, config
  load/save errors, and each accepted press as GPIO plus `PASTE` or `SKIP`.

Pressing Enter on a GPIO opens a multiline editor for that mapping. Enter adds
a newline, Backspace deletes, and arrow keys move the cursor. `Ctrl+S` accepts
the edited value in memory and writes the complete mapping to disk. Escape
discards the current edit. Outside the editor, `Ctrl+S` saves all current
mappings and `q` exits.

The footer always shows the available keys and the current connection or save
status. A terminal that is too small shows a resize message instead of drawing
overlapping panes.

## Configuration

The TUI opens the same `--config` path used by the existing helper. It validates
the document with `MappingConfig` and displays all supported GPIOs; absent keys
appear as empty values.

Saving emits the existing schema:

```yaml
buttons:
  6: first value
  7: |-
    first line
    second line
```

Empty mappings are omitted. The helper writes UTF-8 YAML to a temporary file in
the configuration directory and replaces the target with `os.replace`, so an
interrupted write cannot leave a partial configuration. A failed save keeps the
editor contents and reports the error in the log.

## Runtime Flow

The curses main thread owns all screen state and input. The existing serial
reconnect loop runs in one daemon thread and sends log records to the UI through
a standard-library queue. It continues to parse `PRESS` lines, copy mapped text
with `pbcopy`, and reply with the existing `PASTE` or `SKIP` protocol.

The serial thread reads mappings through `MappingConfig`. After a successful
save, its existing modification-time reload picks up the replacement file. No
firmware or protocol change is required.

## Errors And Shutdown

Missing or invalid startup configuration leaves every GPIO empty and reports
the existing validation error in the log. Serial disconnects are logged and the
helper keeps reconnecting. `q`, `Ctrl+C`, or a terminal error restores the
terminal before the process exits.

## Verification

One focused Python test covers YAML serialization and atomic replacement,
including Unicode, multiline text, empty mappings, and replacement failure.
Existing helper protocol and hot-reload tests remain green. A pseudo-terminal
smoke check verifies that the split layout starts and exits cleanly; physical
hardware verification confirms that pressing two mapped GPIO buttons appends
the correct right-pane entries and pastes the configured text.
