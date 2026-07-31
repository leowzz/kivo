import json
import platform
import re
import subprocess
import sys
from collections.abc import Iterator

from serial.tools.list_ports import comports


KNOWN_TARGETS = {
    (0x303A, 0x4002): ("runtime", "luatos-esp32s3-aio"),
    (0x303A, 0x1001): ("bootloader", "luatos-esp32s3-aio"),
    (0x2E8A, 0x102E): ("runtime", "vccgnd-yd-rp2040"),
    (0x2E8A, 0x0003): ("bootloader", "vccgnd-yd-rp2040"),
}


class InventoryError(RuntimeError):
    pass


def format_row(mode: str, usb_id: tuple[int, int], board: str, serial_number: str | None, port: str | None) -> str:
    return "\t".join(
        [mode, f"{usb_id[0]:04x}:{usb_id[1]:04x}", board, serial_number or "-", port or "-"]
    )


def cdc_rows() -> Iterator[tuple[str, tuple[int, int], str, str | None, str | None]]:
    for port in comports():
        usb_id = (port.vid, port.pid)
        target = KNOWN_TARGETS.get(usb_id)
        if target:
            mode, board = target
            yield mode, usb_id, board, port.serial_number, port.device


def parse_usb_id(value: object) -> int | None:
    match = re.search(r"0x([0-9a-fA-F]{1,4})", str(value))
    return int(match.group(1), 16) if match else None


def walk_usb_devices(value: object) -> Iterator[dict[str, object]]:
    if isinstance(value, dict):
        yield value
        for nested in value.values():
            yield from walk_usb_devices(nested)
    elif isinstance(value, list):
        for nested in value:
            yield from walk_usb_devices(nested)


def macos_uf2_rows(
    *,
    run: object = subprocess.run,
    system_name: str | None = None,
) -> Iterator[tuple[str, tuple[int, int], str, str | None, None]]:
    if (platform.system() if system_name is None else system_name) != "Darwin":
        return
    try:
        result = run(
            ["system_profiler", "SPUSBDataType", "-json"],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
    except subprocess.TimeoutExpired as error:
        raise InventoryError("system_profiler timed out after 10 seconds") from error
    except OSError as error:
        raise InventoryError(f"cannot run system_profiler: {error}") from error
    if result.returncode:
        detail = result.stderr.strip() or f"exit {result.returncode}"
        raise InventoryError(f"system_profiler failed: {detail}")
    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise InventoryError("system_profiler returned invalid JSON") from error
    for device in walk_usb_devices(data):
        usb_id = (parse_usb_id(device.get("vendor_id")), parse_usb_id(device.get("product_id")))
        if None in usb_id or usb_id not in KNOWN_TARGETS:
            continue
        mode, board = KNOWN_TARGETS[usb_id]
        if mode == "bootloader":
            serial_number = device.get("serial_num") or device.get("serial_number")
            yield mode, usb_id, board, str(serial_number) if serial_number else None, None


def merge_rows(
    rows: Iterator[tuple[str, tuple[int, int], str, str | None, str | None]],
) -> list[tuple[str, tuple[int, int], str, str | None, str | None]]:
    merged: dict[
        tuple[str, tuple[int, int], str, str | None],
        tuple[str, tuple[int, int], str, str | None, str | None],
    ] = {}
    for row in rows:
        key = row[:4]
        existing = merged.get(key)
        if existing is None or (existing[4] is None and row[4] is not None):
            merged[key] = row
    return list(merged.values())


def main() -> None:
    try:
        rows = merge_rows(iter(list(cdc_rows()) + list(macos_uf2_rows())))
    except InventoryError as error:
        print(f"list_firmware_targets: {error}", file=sys.stderr)
        raise SystemExit(1) from error
    for row in rows:
        print(format_row(*row))


if __name__ == "__main__":
    main()
