# GPIO Text Keyboard Design

## Goal

Turn the LuatOS ESP32S3-AIO board into a temporary macOS text-entry device.
Grounding any supported GPIO reports that pin to a local helper. The helper
looks up the pin in a hot-reloaded YAML file, copies the mapped UTF-8 text to
the clipboard, and asks the board's HID keyboard interface to press Command-V.

The mapping stays on the Mac so text and pin assignments can change without
rebuilding or reflashing the board.

## Configuration

The desktop app stores `config.yaml` in its macOS application configuration
directory. On first launch it imports the project-root `config.yaml` when that
file exists. The file uses an integer-to-string mapping:

```yaml
buttons:
  6: |
    GPIO6 对应的中文文本
  7: |
    GPIO7 对应的另一段文本
```

Keys must be integers in the supported GPIO set. Values must be strings and
may contain UTF-8 text, spaces, punctuation, and newlines. Empty strings are
valid and result in no paste action.

The graphical editor validates and atomically replaces the YAML file when Save
or Command-S is used. A successful save replaces the Rust serial worker's
shared in-memory mapping immediately. Invalid startup content opens as empty
mappings with a visible error; correcting and saving it clears the error.

## Supported Inputs

The firmware monitors GPIO0 through GPIO9 and GPIO12 through GPIO18. Each pin
uses the internal pull-up and is active low. The following pins are excluded:

- GPIO10 and GPIO11: onboard LEDs
- GPIO19 and GPIO20: native USB
- All other non-exposed or flash/PSRAM-related pins

GPIO0 is a boot strap and BOOT-button pin. It works as an input after startup,
but holding it low during power-on or reset puts the board into download mode.

Each input is debounced for 30 milliseconds. A stable high-to-low transition
generates one event. Holding a pin low does not repeat. The pin must return to
a stable high level before another press can be generated.

## USB Device

The board runs in USB OTG mode as a composite device with:

- USB CDC for the helper protocol
- USB HID keyboard for the final Command-V keystroke

No text is typed as keyboard characters. This avoids keyboard-layout and
Sogou input-method conversion problems. The helper never takes focus and does
not require macOS Accessibility permission because the board itself sends the
paste shortcut.

## Protocol

The firmware assigns a monotonically increasing 32-bit event ID to each
accepted press and sends one ASCII line over CDC:

```text
PRESS <event-id> <gpio>\n
```

Only one event may wait for a response at a time. Additional presses are
ignored until the event is completed or times out after two seconds.

For a mapped, non-empty value, the helper writes the exact UTF-8 bytes to
macOS `pbcopy` and replies:

```text
PASTE <event-id>\n
```

The firmware accepts only the matching event ID, sends Command-V through the
HID keyboard, and clears the pending event. For an unmapped GPIO or empty
value, the helper replies:

```text
SKIP <event-id>\n
```

The firmware clears the matching event without emitting a keypress. Malformed
or stale responses are ignored. A timeout also clears the pending event so a
stopped helper cannot permanently block later presses.

## Helper Lifecycle

`make helper` starts the Tauri 2 desktop application in development mode. Its
Rust backend repeatedly scans macOS serial ports for the board's TinyUSB CDC
interface and reconnects after USB disconnects. Closing the application stops
and joins the single serial worker. `make helper-build` creates the distributable
macOS application bundle.

## Build And Upload

The default `make` target builds and uploads the firmware. `make test` runs the
native C++ controller, Rust backend, and React interface tests. `make build`,
`make upload`, and `make helper-build` remain available separately.

The initial upload uses the board's built-in USB/JTAG interface. Once the HID
firmware is running, another JTAG upload may require holding BOOT and tapping
RST before running `make`, because USB OTG and built-in USB/JTAG share the
board's USB connection.

## Verification

- Native C++ tests cover the supported GPIO set, debounce, held-low behavior,
  event serialization, pending-event exclusion, timeout, and matching ACKs.
- Rust tests cover valid YAML, Unicode and multiline values, atomic save
  failures, exact USB-device matching, mapped paste, clipboard failure, and
  unmapped skip responses.
- React tests cover initial load, Unicode edits, dirty/save state, save errors,
  runtime events, cleanup, and the GPIO0 warning.
- The firmware builds for ESP32-S3 in USB OTG mode.
- The default `make` target uploads the firmware.
- `make helper-build` produces a macOS `.app` bundle.
- A physical check grounds at least two configured GPIOs and confirms each
  inserts its exact mapped Chinese text once into the focused application.
