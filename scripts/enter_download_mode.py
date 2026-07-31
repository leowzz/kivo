import argparse
import sys
import time
from collections.abc import Iterable

import serial
from serial.tools.list_ports import comports


KIVO_USB_ID = (0x303A, 0x4002)
DOWNLOAD_USB_ID = (0x303A, 0x1001)


class TargetError(RuntimeError):
    pass


def require_serial(serial_number: str | None) -> str:
    serial_number = (serial_number or "").strip()
    if not serial_number:
        raise TargetError("SERIAL is required")
    return serial_number


def select_runtime_port(ports: Iterable[object], usb_id: tuple[int, int], serial_number: str) -> object:
    serial_number = require_serial(serial_number)
    matches = [
        port
        for port in ports
        if (getattr(port, "vid", None), getattr(port, "pid", None)) == usb_id
        and getattr(port, "serial_number", None) == serial_number
    ]
    if not matches:
        raise TargetError(f"serial {serial_number} not found for {usb_id[0]:04x}:{usb_id[1]:04x}")
    if len(matches) != 1:
        raise TargetError(f"multiple ports found for serial {serial_number}")
    return matches[0]


def select_download_port(ports: Iterable[object], serial_number: str, location: str | None) -> object:
    serial_number = require_serial(serial_number)
    if not location:
        raise TargetError("USB location is required for ESP32-S3 download mode")
    matches = [
        port
        for port in ports
        if (getattr(port, "vid", None), getattr(port, "pid", None)) == DOWNLOAD_USB_ID
        and getattr(port, "location", None) == location
        and (
            not getattr(port, "serial_number", None)
            or getattr(port, "serial_number", None) == serial_number
        )
    ]
    if not matches:
        raise TargetError(
            f"download port for serial {serial_number} not found at USB location {location}"
        )
    if len(matches) != 1:
        raise TargetError(f"multiple ports found at USB location {location}")
    return matches[0]


def wait_for_download_port(serial_number: str, location: str, timeout: float = 10.0) -> object:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            return select_download_port(comports(), serial_number, location)
        except TargetError:
            time.sleep(0.1)
    return select_download_port(comports(), serial_number, location)


def enter_download_mode(serial_number: str) -> str:
    serial_number = require_serial(serial_number)
    runtime_port = select_runtime_port(comports(), KIVO_USB_ID, serial_number)
    location = getattr(runtime_port, "location", None)
    if not location:
        raise TargetError(f"runtime serial {serial_number} has no USB location")

    print(
        f"Entering download mode through {runtime_port.device} at USB location {location}",
        file=sys.stderr,
    )
    try:
        with serial.Serial(runtime_port.device, 1200, timeout=1) as device:
            device.dtr = True
            device.rts = True
            time.sleep(0.5)
    except serial.SerialException as error:
        raise TargetError(
            f"cannot open {runtime_port.device}; stop make helper first: {error}"
        ) from error

    download_port = wait_for_download_port(serial_number, location)
    print(f"Resolved download port {download_port.device}", file=sys.stderr)
    return download_port.device


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--serial", required=True)
    args = parser.parse_args()
    try:
        print(enter_download_mode(args.serial))
    except TargetError as error:
        raise SystemExit(str(error)) from error


if __name__ == "__main__":
    main()
