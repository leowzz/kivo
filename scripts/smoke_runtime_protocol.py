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
    for _ in range(32):
        line = read_line(device)
        tokens = line.split()
        if tokens == expected:
            return
        if tokens and tokens[0] in {"HELLO", "STATE", "LEARN_DIRECT", "LEARN_CONTACT"}:
            continue
        raise RuntimeError(f"expected {' '.join(expected)!r}, got {line!r}")
    raise RuntimeError(f"expected {' '.join(expected)!r}, got too many asynchronous events")


def validate_hello(
    line: str,
    family: str,
    board: str,
    build: str,
    protocol_version: int = 6,
) -> None:
    parts = line.split()
    expected = ["HELLO", str(protocol_version), family, board, build]
    if protocol_version >= 9:
        expected.append("-")
    if parts[: len(expected)] != expected:
        raise RuntimeError(f"invalid HELLO: expected {' '.join(expected)!r}, got {line!r}")
    pin_count_index = len(expected)
    if len(parts) < pin_count_index + 2:
        raise RuntimeError(f"invalid HELLO: missing non-empty pin list in {line!r}")
    if not all(token.isascii() and token.isdigit() for token in parts[pin_count_index:]):
        raise RuntimeError(f"invalid HELLO: non-integer pin data in {line!r}")
    try:
        pin_count = int(parts[pin_count_index])
        pins = [int(pin) for pin in parts[pin_count_index + 1 :]]
    except ValueError as error:
        raise RuntimeError(f"invalid HELLO: non-integer pin data in {line!r}") from error
    if (
        pin_count <= 0
        or len(pins) != pin_count
        or len(set(pins)) != len(pins)
        or any(pin > 255 for pin in pins)
    ):
        raise RuntimeError(f"invalid HELLO: inconsistent pin list in {line!r}")


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
    build: str,
    exercise_actions: bool = False,
    protocol_version: int = 6,
) -> None:
    if not valid_pins:
        raise RuntimeError("valid pins are required")
    if protocol_version < 3 or protocol_version > 12:
        raise RuntimeError("unsupported protocol version")
    write_line(device, "HELLO\n")
    validate_hello(read_line(device), family, board, build, protocol_version)

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
        run_id = 1 if protocol_version >= 6 else event_id
        total = 3 if protocol_version >= 6 else 2
        write_line(device, f"PASTE {run_id} 1 {total}\n")
        expect_tokens(device, ["DONE", str(run_id), "1"])
        if protocol_version >= 6:
            write_line(device, f"DELAY {run_id} 2 3 500\n")
            expect_tokens(device, ["DONE", str(run_id), "2"])
            write_line(device, f"MEDIA {run_id} 3 3 205\n")
        else:
            write_line(device, f"HOTKEY {run_id} 2 2 1 25\n")
        expect_tokens(device, ["DONE", str(run_id), str(total)])


def parse_pins(value: str) -> list[int]:
    try:
        pins = [int(pin, 10) for pin in value.split(",")]
    except ValueError as error:
        raise argparse.ArgumentTypeError("pins must be comma-separated integers") from error
    if not pins or any(pin < 0 or pin > 255 for pin in pins):
        raise argparse.ArgumentTypeError("pins must be integers from 0 to 255")
    return pins


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument("--serial", required=True)
    parser.add_argument("--vid", required=True, type=parse_usb_id)
    parser.add_argument("--pid", required=True, type=parse_usb_id)
    parser.add_argument("--family", required=True)
    parser.add_argument("--board", required=True)
    parser.add_argument("--build", required=True)
    parser.add_argument("--valid-pins", required=True, type=parse_pins)
    parser.add_argument("--rejected-pins", required=True, type=parse_pins)
    parser.add_argument("--exercise-actions", action="store_true")
    parser.add_argument("--protocol-version", type=int, choices=range(3, 13), default=12)
    return parser


def run_from_args(
    args: argparse.Namespace,
    *,
    port_waiter: object = wait_for_runtime_port,
    serial_factory: object = serial.Serial,
) -> None:
    port = port_waiter(require_serial(args.serial), (args.vid, args.pid))
    with serial_factory(port.device, 115200, timeout=0.5) as device:
        run_smoke(
            device,
            family=args.family,
            board=args.board,
            build=args.build,
            valid_pins=args.valid_pins,
            rejected_pins=args.rejected_pins,
            exercise_actions=args.exercise_actions,
            protocol_version=args.protocol_version,
        )


def main() -> None:
    args = build_parser().parse_args()
    try:
        run_from_args(args)
    except (TargetError, RuntimeError, serial.SerialException) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1) from error


if __name__ == "__main__":
    main()
