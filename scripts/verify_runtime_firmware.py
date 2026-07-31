import argparse
import sys
import time

import serial
from serial.tools.list_ports import comports

try:
    from .enter_download_mode import TargetError, require_serial, select_runtime_port
except ImportError:
    from enter_download_mode import TargetError, require_serial, select_runtime_port


def parse_usb_id(value: str) -> int:
    try:
        return int(value, 0)
    except ValueError as error:
        raise argparse.ArgumentTypeError(f"invalid USB ID: {value}") from error


def wait_for_runtime_port(serial_number: str, usb_id: tuple[int, int], timeout: float = 10.0) -> object:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            return select_runtime_port(comports(), usb_id, serial_number)
        except TargetError:
            time.sleep(0.1)
    return select_runtime_port(comports(), usb_id, serial_number)


def verify_runtime_firmware(
    serial_number: str, usb_id: tuple[int, int], family: str, board: str, build: str
) -> None:
    serial_number = require_serial(serial_number)
    port = wait_for_runtime_port(serial_number, usb_id)
    try:
        with serial.Serial(port.device, 115200, timeout=1) as device:
            device.write(b"HELLO\n")
            line = device.readline().decode("utf-8", errors="replace").strip()
    except serial.SerialException as error:
        raise TargetError(f"cannot open {port.device}: {error}") from error

    expected = ["HELLO", "3", family, board, build]
    if line.split()[:5] != expected:
        raise TargetError(f"unexpected runtime handshake on {port.device}: {line!r}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--serial", required=True)
    parser.add_argument("--vid", required=True, type=parse_usb_id)
    parser.add_argument("--pid", required=True, type=parse_usb_id)
    parser.add_argument("--family", required=True)
    parser.add_argument("--board", required=True)
    parser.add_argument("--build", required=True)
    args = parser.parse_args()
    try:
        verify_runtime_firmware(
            args.serial, (args.vid, args.pid), args.family, args.board, args.build
        )
    except TargetError as error:
        print(error, file=sys.stderr)
        raise SystemExit(1) from error


if __name__ == "__main__":
    main()
