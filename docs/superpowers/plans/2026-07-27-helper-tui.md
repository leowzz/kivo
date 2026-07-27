# Helper TUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an editable curses GPIO mapping screen with a live button-event log to the existing helper.

**Architecture:** Keep the firmware and serial protocol unchanged. `host/text_helper.py` remains the single helper entry point: a curses main thread owns UI state while one daemon thread runs the existing reconnect loop and forwards log strings through `queue.Queue`.

**Tech Stack:** Python 3.13 standard library (`curses`, `queue`, `threading`, `tempfile`), PyYAML, pyserial, unittest.

## Global Constraints

- Add no dependency.
- Keep `--config` and `make helper` as the entry points.
- Save UTF-8 YAML by atomically replacing the selected configuration file.
- Preserve the existing `PRESS`, `PASTE`, and `SKIP` serial protocol.
- Do not modify firmware.

---

### Task 1: Atomic Mapping Save

**Files:**
- Modify: `host/text_helper.py`
- Test: `test/test_helper.py`

**Interfaces:**
- Produces: `save_mappings(path: Path, buttons: dict[int, str]) -> None`.
- Produces YAML shaped as `{"buttons": {gpio: text}}`, omitting empty values.

- [ ] **Step 1: Write the failing save tests**

Add imports for `yaml` and `save_mappings`, then add:

```python
class SaveMappingsTests(unittest.TestCase):
    def setUp(self):
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary_directory.cleanup)
        self.path = Path(self.temporary_directory.name) / "config.yaml"

    def test_replaces_config_with_unicode_multiline_mapping(self):
        self.path.write_text("buttons:\n  1: old\n", encoding="utf-8")

        save_mappings(self.path, {6: "你好\n第二行", 7: ""})

        self.assertEqual(
            {"buttons": {6: "你好\n第二行"}},
            yaml.safe_load(self.path.read_text(encoding="utf-8")),
        )

    def test_replace_failure_keeps_original_config(self):
        self.path.write_text("buttons:\n  1: old\n", encoding="utf-8")

        with patch("host.text_helper.os.replace", side_effect=OSError("full")):
            with self.assertRaisesRegex(OSError, "full"):
                save_mappings(self.path, {6: "new"})

        self.assertEqual("buttons:\n  1: old\n", self.path.read_text(encoding="utf-8"))
        self.assertEqual([self.path], list(self.path.parent.iterdir()))
```

- [ ] **Step 2: Run the tests and verify RED**

Run: `rtk uv run python -m unittest test.test_helper.SaveMappingsTests -v`

Expected: import error because `save_mappings` does not exist.

- [ ] **Step 3: Implement atomic replacement**

Add `os` and `tempfile` imports and this function after `MappingConfig`:

```python
def save_mappings(path: Path, buttons: dict[int, str]) -> None:
    document = {"buttons": {gpio: text for gpio, text in buttons.items() if text}}
    descriptor, temporary_name = tempfile.mkstemp(
        dir=path.parent, prefix=f".{path.name}.", text=True
    )
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as temporary_file:
            yaml.safe_dump(
                document, temporary_file, allow_unicode=True, sort_keys=True
            )
        os.replace(temporary_path, path)
    finally:
        temporary_path.unlink(missing_ok=True)
```

- [ ] **Step 4: Run focused and existing helper tests**

Run: `rtk uv run python -m unittest test.test_helper.SaveMappingsTests -v`

Expected: both save tests pass.

Run: `rtk uv run python -m unittest discover -s test -p 'test_helper.py' -v`

Expected: all helper tests pass.

- [ ] **Step 5: Commit**

```bash
rtk git add host/text_helper.py test/test_helper.py
rtk git commit -m "feat: save GPIO mappings atomically"
```

### Task 2: Observable And Stoppable Serial Loop

**Files:**
- Modify: `host/text_helper.py`
- Test: `test/test_helper.py`

**Interfaces:**
- Changes: `MappingConfig(path: Path, report: Callable[[str], None] = print)`.
- Changes: `serve(config_path: Path, report: Callable[[str], None] = print, stop: threading.Event | None = None) -> None`.
- Produces log messages for config state, connection state, and every valid press.

- [ ] **Step 1: Write a failing event-log test**

Add a fake serial device that yields one press and then stops the loop:

```python
class SerialLoopTests(unittest.TestCase):
    def test_reports_pressed_gpio_and_result(self):
        stop = threading.Event()

        class Device:
            def __enter__(self):
                return self

            def __exit__(self, *arguments):
                return False

            def readline(self):
                stop.set()
                return b"PRESS 12 6\n"

            def write(self, value):
                self.written = value

            def flush(self):
                pass

        reports = []
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "config.yaml"
            path.write_text("buttons:\n  6: hello\n", encoding="utf-8")
            with patch("host.text_helper.find_device_port", return_value="fake"), patch(
                "host.text_helper.serial.Serial", return_value=Device()
            ), patch("host.text_helper.copy_to_clipboard"):
                serve(path, reports.append, stop)

        self.assertIn("GPIO6: PASTE 12", reports)
```

Import `threading` and `serve` in the test module.

- [ ] **Step 2: Run the test and verify RED**

Run: `rtk uv run python -m unittest test.test_helper.SerialLoopTests -v`

Expected: `serve()` rejects the `report` and `stop` arguments.

- [ ] **Step 3: Route output through the callback**

Change the constructor to:

```python
def __init__(self, path: Path, report: Callable[[str], None] = print):
    self.path = path
    self.report = report
    self.buttons: dict[int, str] = {}
    self._observed_mtime_ns: int | None = None
```

In the first `reload_if_changed` exception block, replace the output call with:

```python
except OSError as error:
    self.report(f"config unavailable: {error}")
    return False
```

In the validation exception block, replace the output call with:

```python
except (OSError, UnicodeError, yaml.YAMLError, ValueError) as error:
    self.report(f"config reload failed: {error}")
    return False
```

Replace the successful-load output call with:

```python
self.buttons = buttons
self.report(f"loaded {len(buttons)} button mapping(s) from {self.path}")
return True
```

Update `serve` to create a default event when needed:

```python
def serve(
    config_path: Path,
    report: Callable[[str], None] = print,
    stop: threading.Event | None = None,
) -> None:
    stop = stop or threading.Event()
    config = MappingConfig(config_path, report)
    config.reload_if_changed()

    while not stop.is_set():
        port = find_device_port()
        if port is None:
            config.reload_if_changed()
            stop.wait(0.5)
            continue

        report(f"connected to {port}")
        try:
            with serial.Serial(port, 115200, timeout=0.5, write_timeout=1) as device:
                while not stop.is_set():
                    config.reload_if_changed()
                    raw_line = device.readline()
                    if not raw_line:
                        continue
                    try:
                        press = parse_press_line(raw_line.decode("ascii"))
                    except UnicodeDecodeError:
                        press = None
                    if press is None:
                        continue
                    event_id, gpio = press
                    response = handle_press(event_id, gpio, config.buttons)
                    device.write(response.encode("ascii"))
                    device.flush()
                    report(f"GPIO{gpio}: {response.strip()}")
        except (OSError, serial.SerialException, subprocess.SubprocessError) as error:
            report(f"device disconnected: {error}")
            stop.wait(0.5)
```

- [ ] **Step 4: Run helper tests**

Run: `rtk uv run python -m unittest discover -s test -p 'test_helper.py' -v`

Expected: all helper tests pass with no stdout or stderr warnings.

- [ ] **Step 5: Commit**

```bash
rtk git add host/text_helper.py test/test_helper.py
rtk git commit -m "refactor: expose helper events to TUI"
```

### Task 3: Curses Mapping Editor And Split Log

**Files:**
- Modify: `host/text_helper.py`
- Test: `test/test_helper.py`

**Interfaces:**
- Produces: `TextBuffer(text: str)` with `handle(key: int | str) -> str | None` and `text() -> str`.
- Produces: `run_tui(screen: curses.window, config_path: Path) -> None`.
- Changes: `main()` calls `curses.wrapper(run_tui, arguments.config)`.

- [ ] **Step 1: Write failing editor behavior tests**

Add:

```python
class TextBufferTests(unittest.TestCase):
    def test_edits_unicode_multiline_text(self):
        editor = TextBuffer("ab\ncd")
        editor.handle(curses.KEY_DOWN)
        editor.handle(curses.KEY_RIGHT)
        editor.handle("你")
        editor.handle(10)
        editor.handle("好")

        self.assertEqual("ab\nc你\n好d", editor.text())

    def test_backspace_joins_lines_and_escape_cancels(self):
        editor = TextBuffer("a\nb")
        editor.handle(curses.KEY_DOWN)

        self.assertIsNone(editor.handle(curses.KEY_BACKSPACE))
        self.assertEqual("ab", editor.text())
        self.assertEqual("cancel", editor.handle(27))
```

Import `curses` and `TextBuffer` in the test module.

- [ ] **Step 2: Run editor tests and verify RED**

Run: `rtk uv run python -m unittest test.test_helper.TextBufferTests -v`

Expected: import error because `TextBuffer` does not exist.

- [ ] **Step 3: Implement the editor state**

Add:

```python
class TextBuffer:
    def __init__(self, text: str):
        self.lines = text.split("\n")
        self.row = 0
        self.column = 0

    def text(self) -> str:
        return "\n".join(self.lines)

    def handle(self, key: int | str) -> str | None:
        if key == 27:
            return "cancel"
        if key == 19:
            return "save"
        if key == curses.KEY_UP:
            self.row = max(0, self.row - 1)
            self.column = min(self.column, len(self.lines[self.row]))
        elif key == curses.KEY_DOWN:
            self.row = min(len(self.lines) - 1, self.row + 1)
            self.column = min(self.column, len(self.lines[self.row]))
        elif key == curses.KEY_LEFT:
            if self.column:
                self.column -= 1
            elif self.row:
                self.row -= 1
                self.column = len(self.lines[self.row])
        elif key == curses.KEY_RIGHT:
            if self.column < len(self.lines[self.row]):
                self.column += 1
            elif self.row < len(self.lines) - 1:
                self.row += 1
                self.column = 0
        elif key in (10, 13):
            line = self.lines[self.row]
            self.lines[self.row : self.row + 1] = [
                line[: self.column],
                line[self.column :],
            ]
            self.row += 1
            self.column = 0
        elif key in (curses.KEY_BACKSPACE, 127, 8):
            if self.column:
                line = self.lines[self.row]
                self.lines[self.row] = (
                    line[: self.column - 1] + line[self.column :]
                )
                self.column -= 1
            elif self.row:
                previous_length = len(self.lines[self.row - 1])
                self.lines[self.row - 1] += self.lines.pop(self.row)
                self.row -= 1
                self.column = previous_length
        elif isinstance(key, str) and key.isprintable():
            line = self.lines[self.row]
            self.lines[self.row] = line[: self.column] + key + line[self.column :]
            self.column += len(key)
        return None
```

- [ ] **Step 4: Run editor tests and verify GREEN**

Run: `rtk uv run python -m unittest test.test_helper.TextBufferTests -v`

Expected: both editor tests pass.

- [ ] **Step 5: Implement the curses screen**

Add `curses`, `queue`, `threading`, `time`, and `deque` imports. Implement:

```python
def run_tui(screen: curses.window, config_path: Path) -> None:
    reports: queue.Queue[str] = queue.Queue()
    stop = threading.Event()
    config = MappingConfig(config_path, reports.put)
    config.reload_if_changed()
    gpios = sorted(SUPPORTED_GPIOS)
    buttons = {gpio: config.buttons.get(gpio, "") for gpio in gpios}
    logs: deque[str] = deque(maxlen=200)
    selected = 0
    editor: TextBuffer | None = None
    worker = threading.Thread(
        target=serve, args=(config_path, reports.put, stop), daemon=True
    )
    worker.start()
    screen.timeout(100)

    try:
        while True:
            while True:
                try:
                    logs.append(f"{time.strftime('%H:%M:%S')} {reports.get_nowait()}")
                except queue.Empty:
                    break

            height, width = screen.getmaxyx()
            screen.erase()
            if height < 20 or width < 70:
                screen.addnstr(0, 0, "terminal too small (minimum 70x20)", width - 1)
            else:
                divider = width * 2 // 3
                screen.addnstr(0, 1, "GPIO mappings", divider - 2, curses.A_BOLD)
                screen.vline(0, divider, curses.ACS_VLINE, height - 1)
                screen.addnstr(
                    0, divider + 2, "Button log", width - divider - 3, curses.A_BOLD
                )

                if editor is None:
                    for index, gpio in enumerate(gpios):
                        preview = buttons[gpio].replace("\n", "\\n") or "<empty>"
                        screen.addnstr(
                            index + 2,
                            1,
                            f"GPIO{gpio:<2}  {preview}",
                            divider - 2,
                            curses.A_REVERSE if index == selected else curses.A_NORMAL,
                        )
                    footer = "Enter edit  Ctrl-S save  q quit"
                    curses.curs_set(0)
                else:
                    screen.addnstr(
                        2, 1, f"Editing GPIO{gpios[selected]}", divider - 2, curses.A_BOLD
                    )
                    visible_height = height - 6
                    top = max(0, editor.row - visible_height + 1)
                    for offset, line in enumerate(editor.lines[top : top + visible_height]):
                        screen.addnstr(offset + 3, 1, line, divider - 2)
                    screen.move(editor.row - top + 3, min(editor.column + 1, divider - 1))
                    footer = "Ctrl-S save  Esc cancel"
                    curses.curs_set(1)

                for row, message in enumerate(list(logs)[-(height - 3) :], start=2):
                    screen.addnstr(row, divider + 2, message, width - divider - 3)
                screen.addnstr(height - 1, 1, footer, width - 2, curses.A_REVERSE)
            screen.refresh()

            try:
                key = screen.get_wch()
            except curses.error:
                continue

            if editor is not None:
                result = editor.handle(key)
                if result == "cancel":
                    editor = None
                elif result == "save":
                    buttons[gpios[selected]] = editor.text()
                    try:
                        save_mappings(config_path, buttons)
                    except OSError as error:
                        reports.put(f"save failed: {error}")
                    else:
                        reports.put(f"saved {config_path}")
                        editor = None
                continue

            if key in ("q", "Q"):
                return
            if key in (curses.KEY_UP, "k"):
                selected = max(0, selected - 1)
            elif key in (curses.KEY_DOWN, "j"):
                selected = min(len(gpios) - 1, selected + 1)
            elif key in (10, 13):
                editor = TextBuffer(buttons[gpios[selected]])
            elif key == 19:
                try:
                    save_mappings(config_path, buttons)
                except OSError as error:
                    reports.put(f"save failed: {error}")
                else:
                    reports.put(f"saved {config_path}")
    finally:
        stop.set()
        worker.join(timeout=1)
```

Update `main()` to run `curses.wrapper(run_tui, arguments.config)` and retain
clean `KeyboardInterrupt` handling:

```python
try:
    curses.wrapper(run_tui, arguments.config)
except KeyboardInterrupt:
    print("helper stopped")
```

Update `CliTests.test_keyboard_interrupt_exits_cleanly` to patch the new entry
point and import `run_tui`:

```python
with patch("host.text_helper.curses.wrapper", side_effect=KeyboardInterrupt) as wrapper, patch(
    "sys.argv", ["text-helper"]
), redirect_stdout(output):
    main()

wrapper.assert_called_once()
self.assertIs(run_tui, wrapper.call_args.args[0])
self.assertEqual("helper stopped\n", output.getvalue())
```

- [ ] **Step 6: Run all automated tests**

Run: `rtk make test`

Expected: native C++ and Python suites pass.

- [ ] **Step 7: Run terminal smoke check**

Run in a real terminal: `rtk make helper`

Verify: both panes render without overlap; Enter edits Unicode and multiline
text; Escape cancels; Ctrl+S replaces `config.yaml`; q restores the terminal.
Without hardware, the right pane must still show config and reconnect status.

- [ ] **Step 8: Run hardware acceptance check**

With the ESP32-S3 connected, map two GPIOs, save, and ground each pin once.

Verify: each press appends the correct `GPIO<n>: PASTE <event-id>` entry in the
right pane and inserts the exact mapped text once into the focused macOS app.

- [ ] **Step 9: Commit**

```bash
rtk git add host/text_helper.py test/test_helper.py
rtk git commit -m "feat: add GPIO helper TUI"
```
