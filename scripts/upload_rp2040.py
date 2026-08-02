import argparse
import json
import platform
import re
import subprocess
import sys
import time
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from pathlib import Path

import serial
from serial.tools.list_ports import comports


RP2040_RUNTIME_USB_ID = (0x2E8A, 0x102E)
RP2040_BOOTSEL_USB_ID = (0x2E8A, 0x0003)
PICOTOOL_PACKAGE = "tool-picotool-rp2040-earlephilhower"


class TargetError(RuntimeError):
    pass


@dataclass(frozen=True)
class UsbDevice:
    usb_id: tuple[int, int]
    serial_number: str | None
    location: str
    bus: int
    address: int


UsbInventory = Callable[[], list[UsbDevice]]
PortInventory = Callable[[], Iterable[object]]
SerialFactory = Callable[[str, int], object]
Picotool = Callable[[list[str]], str]


def walk_usb_devices(value: object) -> Iterable[dict[str, object]]:
    if isinstance(value, dict):
        yield value
        for nested in value.values():
            yield from walk_usb_devices(nested)
    elif isinstance(value, list):
        for nested in value:
            yield from walk_usb_devices(nested)


def parse_hex_id(value: object) -> int | None:
    match = re.search(r"0x([0-9a-fA-F]{1,4})", str(value))
    return int(match.group(1), 16) if match else None


def parse_macos_location(value: object) -> tuple[str, int, int] | None:
    match = re.fullmatch(r"\s*(0x[0-9a-fA-F]+)\s*/\s*(\d+)\s*", str(value))
    if not match:
        return None
    location = match.group(1).lower()
    bus = int(location, 16) >> 24
    return location, bus, int(match.group(2))


def parse_macos_usb_devices(output: str) -> list[UsbDevice]:
    try:
        data = json.loads(output)
    except json.JSONDecodeError as error:
        raise TargetError("system_profiler returned invalid JSON") from error

    devices = []
    for raw in walk_usb_devices(data):
        usb_id = (parse_hex_id(raw.get("vendor_id")), parse_hex_id(raw.get("product_id")))
        if usb_id not in {RP2040_RUNTIME_USB_ID, RP2040_BOOTSEL_USB_ID}:
            continue
        location = parse_macos_location(raw.get("location_id"))
        if location is None:
            continue
        location_id, bus, address = location
        serial_number = raw.get("serial_num") or raw.get("serial_number")
        devices.append(
            UsbDevice(
                usb_id=(int(usb_id[0]), int(usb_id[1])),
                serial_number=str(serial_number) if serial_number else None,
                location=location_id,
                bus=bus,
                address=address,
            )
        )
    return devices


def scan_macos_usb_devices(
    *,
    run: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
) -> list[UsbDevice]:
    if platform.system() != "Darwin":
        raise TargetError("targeted RP2040 upload currently requires macOS")
    try:
        result = run(
            ["system_profiler", "SPUSBDataType", "-json"],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
    except subprocess.TimeoutExpired as error:
        raise TargetError("system_profiler timed out after 10 seconds") from error
    except OSError as error:
        raise TargetError(f"cannot run system_profiler: {error}") from error
    if result.returncode:
        detail = result.stderr.strip() or f"exit {result.returncode}"
        raise TargetError(f"system_profiler failed: {detail}")
    return parse_macos_usb_devices(result.stdout)


def run_picotool(
    arguments: list[str],
    *,
    run: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
) -> str:
    command = [
        "pio",
        "pkg",
        "exec",
        "-p",
        PICOTOOL_PACKAGE,
        "--",
        "picotool",
        *arguments,
    ]
    try:
        result = run(command, check=False, capture_output=True, text=True)
    except OSError as error:
        raise TargetError(f"cannot run picotool: {error}") from error
    if result.returncode:
        detail = result.stderr.strip() or result.stdout.strip()
        raise TargetError(detail or f"picotool exited with {result.returncode}")
    if arguments and arguments[0] == "load":
        if result.stdout:
            print(result.stdout, end="", file=sys.stderr)
        if result.stderr:
            print(result.stderr, end="", file=sys.stderr)
    return result.stdout


def target_arguments(target: UsbDevice) -> list[str]:
    return ["--bus", str(target.bus), "--address", str(target.address)]


def read_flash_id(target: UsbDevice, picotool: Picotool) -> str:
    output = picotool(["info", "-a", *target_arguments(target)])
    match = re.search(r"^\s*flash id:\s*0x([0-9a-fA-F]+)\s*$", output, re.MULTILINE)
    if not match:
        raise TargetError("picotool did not report the RP2040 flash ID")
    return match.group(1).upper()


def require_one(matches: list[object], description: str) -> object:
    if not matches:
        raise TargetError(f"{description} was not found")
    if len(matches) != 1:
        raise TargetError(f"multiple {description}s were found")
    return matches[0]


def select_runtime_port(ports: Iterable[object], serial_number: str) -> object:
    matches = [
        port
        for port in ports
        if (getattr(port, "vid", None), getattr(port, "pid", None))
        == RP2040_RUNTIME_USB_ID
        and getattr(port, "serial_number", None) == serial_number
    ]
    return require_one(matches, f"runtime port for serial {serial_number}")


def wait_for_bootsel(
    location: str,
    *,
    usb_inventory: UsbInventory,
    monotonic: Callable[[], float],
    sleep: Callable[[float], None],
    timeout: float = 10.0,
) -> UsbDevice:
    deadline = monotonic() + timeout
    while True:
        matches = [
            device
            for device in usb_inventory()
            if device.usb_id == RP2040_BOOTSEL_USB_ID
            and device.location == location
        ]
        if len(matches) == 1:
            return matches[0]
        if len(matches) > 1:
            raise TargetError(f"multiple BOOTSEL devices appeared at USB location {location}")
        if monotonic() >= deadline:
            raise TargetError(
                f"timed out waiting for the selected device at USB location {location} "
                "to enter BOOTSEL mode"
            )
        sleep(0.1)


def touch_runtime_port(
    runtime_port: object,
    *,
    serial_factory: SerialFactory,
) -> None:
    device_path = str(getattr(runtime_port, "device"))
    print(f"Entering BOOTSEL through {device_path}", file=sys.stderr)
    try:
        with serial_factory(device_path, 1200) as device:
            device.dtr = False
    except (serial.SerialException, OSError) as error:
        raise TargetError(
            f"cannot open {device_path}; run make helper-kill first: {error}"
        ) from error


def find_bootsel_by_flash_id(
    devices: list[UsbDevice],
    serial_number: str,
    *,
    picotool: Picotool,
) -> tuple[UsbDevice, str] | None:
    matches = []
    for device in devices:
        if device.usb_id != RP2040_BOOTSEL_USB_ID:
            continue
        flash_id = read_flash_id(device, picotool)
        if flash_id.casefold() == serial_number.casefold():
            matches.append((device, flash_id))
    if len(matches) > 1:
        raise TargetError(f"multiple BOOTSEL devices match flash ID {serial_number}")
    return matches[0] if matches else None


def prepare_bootsel_target(
    serial_number: str,
    *,
    usb_inventory: UsbInventory,
    ports: PortInventory,
    serial_factory: SerialFactory,
    picotool: Picotool,
    monotonic: Callable[[], float],
    sleep: Callable[[float], None],
) -> tuple[UsbDevice, str]:
    devices = usb_inventory()
    boot_matches = [
        device
        for device in devices
        if device.usb_id == RP2040_BOOTSEL_USB_ID
        and device.serial_number == serial_number
    ]
    if boot_matches:
        target = require_one(boot_matches, f"BOOTSEL device with serial {serial_number}")
        return target, read_flash_id(target, picotool)

    runtime_matches = [
        device
        for device in devices
        if device.usb_id == RP2040_RUNTIME_USB_ID
        and device.serial_number == serial_number
    ]
    if not runtime_matches:
        existing_bootsel = find_bootsel_by_flash_id(
            devices, serial_number, picotool=picotool
        )
        if existing_bootsel:
            return existing_bootsel
        raise TargetError(f"RP2040 serial {serial_number} was not found")

    runtime_device = require_one(
        runtime_matches, f"runtime device with serial {serial_number}"
    )
    runtime_port = select_runtime_port(ports(), serial_number)
    touch_runtime_port(runtime_port, serial_factory=serial_factory)
    target = wait_for_bootsel(
        runtime_device.location,
        usb_inventory=usb_inventory,
        monotonic=monotonic,
        sleep=sleep,
    )
    flash_id = read_flash_id(target, picotool)
    if flash_id.casefold() != serial_number.casefold():
        raise TargetError(
            f"RP2040 identity mismatch at USB location {runtime_device.location}: "
            f"selected {serial_number}, found flash ID {flash_id}"
        )
    return target, flash_id


def upload_rp2040(
    serial_number: str,
    firmware: Path,
    *,
    usb_inventory: UsbInventory = scan_macos_usb_devices,
    ports: PortInventory = comports,
    serial_factory: SerialFactory = serial.Serial,
    picotool: Picotool = run_picotool,
    monotonic: Callable[[], float] = time.monotonic,
    sleep: Callable[[float], None] = time.sleep,
) -> str:
    serial_number = serial_number.strip()
    if not serial_number:
        raise TargetError("SERIAL is required")
    target, runtime_serial = prepare_bootsel_target(
        serial_number,
        usb_inventory=usb_inventory,
        ports=ports,
        serial_factory=serial_factory,
        picotool=picotool,
        monotonic=monotonic,
        sleep=sleep,
    )
    print(
        f"Uploading to RP2040 at USB bus {target.bus}, address {target.address} "
        f"(flash ID {runtime_serial})",
        file=sys.stderr,
    )
    picotool(
        [
            "load",
            "-x",
            str(firmware),
            *target_arguments(target),
        ]
    )
    return runtime_serial


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--serial", required=True)
    parser.add_argument("--firmware", type=Path, required=True)
    args = parser.parse_args()
    try:
        print(upload_rp2040(args.serial, args.firmware))
    except TargetError as error:
        raise SystemExit(f"upload_rp2040: {error}") from error


if __name__ == "__main__":
    main()
