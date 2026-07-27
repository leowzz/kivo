from __future__ import annotations

import argparse
import curses
import os
import queue
import re
import subprocess
import tempfile
import threading
import time
import unicodedata
from collections import deque
from pathlib import Path
from typing import Callable

import serial
import yaml
from serial.tools import list_ports


SUPPORTED_GPIOS = frozenset((*range(1, 10), *range(12, 19)))
PRESS_PATTERN = re.compile(r"PRESS ([0-9]+) ([0-9]+)")
USB_VENDOR_ID = 0x303A
USB_PRODUCT_NAME = "ESP Vibe Text Keyboard"


def _display_width(text: str) -> int:
    # ponytail: terminal-width approximation; use wcwidth if emoji editing matters.
    return sum(
        0
        if unicodedata.combining(character)
        else 2
        if unicodedata.east_asian_width(character) in "WF"
        else 1
        for character in text
    )


class TextBuffer:
    def __init__(self, text: str):
        self.lines = text.split("\n")
        self.row = 0
        self.column = 0

    def text(self) -> str:
        return "\n".join(self.lines)

    def handle(self, key: int | str) -> str | None:
        if key in (27, "\x1b"):
            return "cancel"
        if key in (19, "\x13"):
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
        elif key in (10, 13, "\n", "\r"):
            line = self.lines[self.row]
            self.lines[self.row : self.row + 1] = [
                line[: self.column],
                line[self.column :],
            ]
            self.row += 1
            self.column = 0
        elif key in (curses.KEY_BACKSPACE, 127, 8, "\x7f", "\b"):
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


class MappingConfig:
    def __init__(self, path: Path, report: Callable[[str], None] = print):
        self.path = path
        self.report = report
        self.buttons: dict[int, str] = {}
        self._observed_mtime_ns: int | None = None

    def reload_if_changed(self) -> bool:
        try:
            mtime_ns = self.path.stat().st_mtime_ns
        except OSError as error:
            self.report(f"config unavailable: {error}")
            return False

        if mtime_ns == self._observed_mtime_ns:
            return False
        self._observed_mtime_ns = mtime_ns

        try:
            document = yaml.safe_load(self.path.read_text(encoding="utf-8"))
            buttons = self._validate(document)
        except (OSError, UnicodeError, yaml.YAMLError, ValueError) as error:
            self.report(f"config reload failed: {error}")
            return False

        self.buttons = buttons
        self.report(f"loaded {len(buttons)} button mapping(s) from {self.path}")
        return True

    @staticmethod
    def _validate(document: object) -> dict[int, str]:
        if not isinstance(document, dict):
            raise ValueError("config root must be a mapping")

        raw_buttons = document.get("buttons")
        if not isinstance(raw_buttons, dict):
            raise ValueError("buttons must be a mapping")

        buttons: dict[int, str] = {}
        for gpio, text in raw_buttons.items():
            if type(gpio) is not int or gpio not in SUPPORTED_GPIOS:
                raise ValueError(f"unsupported GPIO key: {gpio!r}")
            if not isinstance(text, str):
                raise ValueError(f"GPIO{gpio} value must be a string")
            buttons[gpio] = text
        return buttons


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


def parse_press_line(line: str) -> tuple[int, int] | None:
    match = PRESS_PATTERN.fullmatch(line.rstrip("\r\n"))
    if match is None:
        return None
    return int(match.group(1)), int(match.group(2))


def copy_to_clipboard(text: str) -> None:
    subprocess.run(["pbcopy"], input=text.encode("utf-8"), check=True)


def handle_press(
    event_id: int,
    gpio: int,
    buttons: dict[int, str],
    clipboard_writer: Callable[[str], None] = copy_to_clipboard,
) -> str:
    text = buttons.get(gpio, "")
    if not text:
        return f"SKIP {event_id}\n"

    clipboard_writer(text)
    return f"PASTE {event_id}\n"


def find_device_port() -> str | None:
    for port in list_ports.comports():
        if port.vid == USB_VENDOR_ID and (
            port.product == USB_PRODUCT_NAME or USB_PRODUCT_NAME in port.description
        ):
            return port.device
    return None


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
    curses.raw()

    try:
        while True:
            while True:
                try:
                    logs.append(
                        f"{time.strftime('%H:%M:%S')} {reports.get_nowait()}"
                    )
                except queue.Empty:
                    break

            height, width = screen.getmaxyx()
            screen.erase()
            if height < 20 or width < 70:
                screen.addnstr(
                    0, 0, "terminal too small (minimum 70x20)", max(0, width - 1)
                )
            else:
                divider = width * 2 // 3
                cursor_position: tuple[int, int] | None = None
                screen.addnstr(0, 1, "GPIO mappings", divider - 2, curses.A_BOLD)
                screen.vline(
                    0, divider, getattr(curses, "ACS_VLINE", ord("|")), height - 1
                )
                screen.addnstr(
                    0,
                    divider + 2,
                    "Button log",
                    width - divider - 3,
                    curses.A_BOLD,
                )

                if editor is None:
                    for index, gpio in enumerate(gpios):
                        preview = buttons[gpio].replace("\n", "\\n") or "<empty>"
                        screen.addnstr(
                            index + 2,
                            1,
                            f"GPIO{gpio:<2}  {preview}",
                            divider - 2,
                            curses.A_REVERSE
                            if index == selected
                            else curses.A_NORMAL,
                        )
                    keys = "Enter edit  Ctrl-S save  q quit"
                    curses.curs_set(0)
                else:
                    screen.addnstr(
                        2,
                        1,
                        f"Editing GPIO{gpios[selected]}",
                        divider - 2,
                        curses.A_BOLD,
                    )
                    visible_height = height - 6
                    top = max(0, editor.row - visible_height + 1)
                    for offset, line in enumerate(
                        editor.lines[top : top + visible_height]
                    ):
                        screen.addnstr(offset + 3, 1, line, divider - 2)
                    cursor_position = (
                        editor.row - top + 3,
                        min(
                            _display_width(
                                editor.lines[editor.row][: editor.column]
                            )
                            + 1,
                            divider - 1,
                        ),
                    )
                    keys = "Ctrl-S save  Esc cancel"
                    curses.curs_set(1)

                for row, message in enumerate(
                    list(logs)[-(height - 3) :], start=2
                ):
                    screen.addnstr(
                        row,
                        divider + 2,
                        message,
                        width - divider - 3,
                    )
                status = logs[-1] if logs else "starting"
                screen.addnstr(
                    height - 1,
                    1,
                    f"{keys} | {status}",
                    width - 2,
                    curses.A_REVERSE,
                )
                if cursor_position is not None:
                    screen.move(*cursor_position)
            screen.refresh()

            try:
                key = screen.get_wch()
            except curses.error:
                continue

            if key in (3, "\x03"):
                return
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
            elif key in (10, 13, "\n", "\r"):
                editor = TextBuffer(buttons[gpios[selected]])
            elif key in (19, "\x13"):
                try:
                    save_mappings(config_path, buttons)
                except OSError as error:
                    reports.put(f"save failed: {error}")
                else:
                    reports.put(f"saved {config_path}")
    finally:
        stop.set()
        worker.join(timeout=1)


def main() -> None:
    project_root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description="ESP Vibe GPIO text helper")
    parser.add_argument(
        "--config", type=Path, default=project_root / "config.yaml"
    )
    arguments = parser.parse_args()
    try:
        curses.wrapper(run_tui, arguments.config)
    except KeyboardInterrupt:
        print("helper stopped")


if __name__ == "__main__":
    main()
