from __future__ import annotations

import argparse
import os
import re
import subprocess
import tempfile
import threading
import time
from pathlib import Path
from typing import Callable

import serial
import yaml
from serial.tools import list_ports


SUPPORTED_GPIOS = frozenset((*range(1, 10), *range(12, 19)))
PRESS_PATTERN = re.compile(r"PRESS ([0-9]+) ([0-9]+)")
USB_VENDOR_ID = 0x303A
USB_PRODUCT_NAME = "ESP Vibe Text Keyboard"


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


def main() -> None:
    project_root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description="ESP Vibe GPIO text helper")
    parser.add_argument(
        "--config", type=Path, default=project_root / "config.yaml"
    )
    arguments = parser.parse_args()
    try:
        serve(arguments.config)
    except KeyboardInterrupt:
        print("helper stopped")


if __name__ == "__main__":
    main()
