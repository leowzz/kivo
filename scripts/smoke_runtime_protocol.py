import argparse
import sys
import time
from typing import Protocol

import serial

try:
    from .enter_download_mode import TargetError, require_serial
    from .verify_runtime_firmware import parse_usb_id, wait_for_runtime_port
except ImportError:
    from enter_download_mode import TargetError, require_serial
    from verify_runtime_firmware import parse_usb_id, wait_for_runtime_port


class LineTransport(Protocol):
    def write(self, data: bytes) -> int: ...

    def readline(self) -> bytes: ...


def write_line(device: LineTransport, line: str) -> None:
    device.write(line.encode())


def read_line(device: LineTransport) -> str:
    line = device.readline().decode("utf-8", errors="replace").strip()
    if not line:
        raise RuntimeError("timed out waiting for firmware response")
    return line


def expect_tokens(device: LineTransport, expected: list[str]) -> None:
    line = read_line(device)
    if line.split() != expected:
        raise RuntimeError(f"expected {' '.join(expected)!r}, got {line!r}")


def send_topology(device: LineTransport, revision: int, pins: list[int], expect_ok: bool) -> None:
    pins_text = " ".join(str(pin) for pin in pins)
    write_line(device, f"CONFIG_BEGIN {revision} 30\n")
    write_line(device, f"CONFIG_DIRECT {revision} 0 {len(pins)} {pins_text}\n")
    if expect_ok:
        write_line(device, f"CONFIG_COMMIT {revision}\n")
        expect_tokens(device, ["CONFIG_OK", str(revision)])
    else:
        expect_tokens(device, ["CONFIG_ERROR", str(revision), "invalid_direct"])


def state_down_event(device: LineTransport) -> int:
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        line = device.readline().decode("utf-8", errors="replace").strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) == 5 and parts[0] == "STATE" and parts[2] == "DIRECT" and parts[-1] == "DOWN":
            try:
                event_id = int(parts[1])
            except ValueError:
                continue
            if event_id > 0:
                return event_id
        if len(parts) == 7 and parts[0] == "STATE" and parts[-1] == "DOWN" and parts[2] == "CONTACT":
            try:
                event_id = int(parts[1])
            except ValueError:
                continue
            if event_id > 0:
                return event_id
    raise RuntimeError("timed out waiting for STATE ... DOWN")


def run_smoke(
    device: LineTransport,
    *,
    family: str,
    board: str,
    valid_pins: list[int],
    rejected_pins: list[int],
    build: str | None = None,
    exercise_actions: bool = False,
) -> None:
    if not valid_pins:
        raise RuntimeError("valid pins are required")
    write_line(device, "HELLO\n")
    hello = read_line(device).split()
    expected_hello = ["HELLO", "3", family, board]
    if hello[:4] != expected_hello or (build is not None and hello[4:5] != [build]):
        raise RuntimeError(f"expected HELLO 3 {family} {board}, got {' '.join(hello)!r}")

    revision = 1
    send_topology(device, revision, valid_pins, expect_ok=True)
    for pin in rejected_pins:
        revision += 1
        send_topology(device, revision, [pin], expect_ok=False)

    revision += 1
    pins_text = " ".join(str(pin) for pin in valid_pins)
    write_line(device, f"LEARN_BEGIN {revision} {len(valid_pins)} {pins_text}\n")
    expect_tokens(device, ["LEARN_OK", str(revision)])
    write_line(device, f"LEARN_END {revision}\n")
    expect_tokens(device, ["LEARN_OK", str(revision)])

    if exercise_actions:
        event_id = state_down_event(device)
        write_line(device, f"PASTE {event_id} 1 2\n")
        expect_tokens(device, ["DONE", str(event_id), "1"])
        write_line(device, f"HOTKEY {event_id} 2 2 1 25\n")
        expect_tokens(device, ["DONE", str(event_id), "2"])


def parse_pins(value: str) -> list[int]:
    try:
        pins = [int(pin, 10) for pin in value.split(",")]
    except ValueError as error:
        raise argparse.ArgumentTypeError("pins must be comma-separated integers") from error
    if not pins or any(pin < 0 or pin > 255 for pin in pins):
        raise argparse.ArgumentTypeError("pins must be integers from 0 to 255")
    return pins


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--serial", required=True)
    parser.add_argument("--vid", required=True, type=parse_usb_id)
    parser.add_argument("--pid", required=True, type=parse_usb_id)
    parser.add_argument("--family", required=True)
    parser.add_argument("--board", required=True)
    parser.add_argument("--valid-pins", required=True, type=parse_pins)
    parser.add_argument("--rejected-pins", required=True, type=parse_pins)
    parser.add_argument("--exercise-actions", action="store_true")
    args = parser.parse_args()
    try:
        serial_number = require_serial(args.serial)
        port = wait_for_runtime_port(serial_number, (args.vid, args.pid))
        with serial.Serial(port.device, 115200, timeout=0.5) as device:
            run_smoke(
                device,
                family=args.family,
                board=args.board,
                valid_pins=args.valid_pins,
                rejected_pins=args.rejected_pins,
                exercise_actions=args.exercise_actions,
            )
    except (TargetError, RuntimeError, serial.SerialException) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1) from error


if __name__ == "__main__":
    main()
