"""Studio firmware operations. Device mutations require an explicit subcommand."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

import serial
from serial.tools.list_ports import comports

from scripts import enter_download_mode as esp
from scripts import upload_rp2040 as rp
from scripts.verify_runtime_firmware import verify_runtime_firmware, wait_for_runtime_port


BOARDS = {
    "yd-rp2040": (rp.RP2040_RUNTIME_USB_ID, "rp2040", ".uf2"),
    "yd-esp32-s3": (esp.KIVO_USB_ID, "esp32s3", ".bin"),
}
LEGACY_BOARDS = {
    "vccgnd-yd-rp2040": "yd-rp2040",
    "luatos-esp32s3-aio": "yd-esp32-s3",
}


def emit(phase: str, **values: object) -> None:
    print(json.dumps({"phase": phase, **values}), flush=True)


def run_tool(package: str, executable: str, arguments: list[str]) -> str:
    result = subprocess.run(
        ["pio", "pkg", "exec", "-p", package, "--", executable, *arguments],
        capture_output=True, text=True, timeout=240,
    )
    output = result.stdout + result.stderr
    if result.returncode:
        raise RuntimeError(output[-6000:] or f"{executable} exited with {result.returncode}")
    return output


def picotool(arguments: list[str]) -> str:
    return run_tool(rp.PICOTOOL_PACKAGE, "picotool", arguments)


def esptool(port: str, arguments: list[str], after: str = "no_reset") -> str:
    return run_tool("tool-esptoolpy", "esptool.py", [
        "--chip", "esp32s3", "--port", port, "--after", after, *arguments,
    ])


def inspect_firmware(path: Path, board: str) -> dict:
    path = path.resolve(strict=True)
    manifest_path = path.with_name("manifest.json")
    if not manifest_path.is_file():
        raise ValueError("Select a Studio product build with manifest.json beside the firmware file")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if not isinstance(manifest, dict):
        raise ValueError("Invalid firmware manifest")
    if manifest.get("board_profile_id") != board:
        raise ValueError("Firmware board does not match the selected device")
    if path.suffix.lower() != BOARDS[board][2] or manifest.get("firmware_file") != path.name:
        raise ValueError("Select the firmware file described by manifest.json")
    if board == "yd-esp32-s3" and not path.name.endswith(".factory.bin"):
        raise ValueError("ESP32-S3 requires a merged .factory.bin image")
    data = path.read_bytes()
    if not data or len(data) != manifest.get("firmware_size"):
        raise ValueError("Firmware size does not match manifest.json")
    digest = hashlib.sha256(data).hexdigest()
    if digest != manifest.get("firmware_sha256"):
        raise ValueError("Firmware checksum does not match manifest.json")
    for key in ("product_version_id", "build_id"):
        value = manifest.get(key)
        if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9_.-]{1,128}", value):
            raise ValueError(f"Invalid {key} in manifest.json")
    return {
        "path": str(path), "sha256": digest, "bytes": len(data),
        "productVersionId": manifest["product_version_id"],
        "buildId": manifest["build_id"], "boardProfileId": board,
    }


def validate_runtime_product(board: str, serial_number: str, product: str) -> None:
    ports = [p for p in comports() if (p.vid, p.pid) == BOARDS[board][0]
             and p.serial_number == serial_number]
    if not ports:
        # A failed upload can leave the same device in its bootloader.
        return
    port = esp.select_runtime_port(ports, BOARDS[board][0], serial_number)
    with serial.Serial(port.device, 115200, timeout=0.2) as device:
        device.dtr = True
        device.rts = True
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            device.write(b"HELLO\n")
            fields = device.readline(512).decode("utf-8", errors="replace").split()
            if len(fields) < 6 or fields[0] != "HELLO":
                continue
            protocol = int(fields[1])
            if not 3 <= protocol <= 13:
                raise ValueError("Unrecognized installed firmware protocol")
            reported_board = LEGACY_BOARDS.get(fields[3], fields[3])
            if fields[2] != BOARDS[board][1] or reported_board != board:
                raise ValueError("Device firmware reports a different board")
            installed_product = fields[5] if protocol >= 9 else "-"
            if installed_product not in ("-", product):
                raise ValueError(f"Product mismatch: device={installed_product}, firmware={product}")
            return
    raise ValueError("Cannot verify the installed product; device did not answer HELLO")


def prepare_rp(serial_number: str) -> rp.UsbDevice:
    target, flash_id = rp.prepare_bootsel_target(
        serial_number, usb_inventory=rp.scan_usb_devices, ports=comports,
        serial_factory=serial.Serial, picotool=picotool,
        monotonic=time.monotonic, sleep=time.sleep,
    )
    if flash_id.casefold() != serial_number.casefold():
        raise ValueError("RP2040 flash identity changed")
    return target


def prepare_esp(serial_number: str) -> str:
    downloads = [p for p in comports() if (p.vid, p.pid) == esp.DOWNLOAD_USB_ID
                 and p.serial_number and esp.serials_match(serial_number, p.serial_number)]
    if len(downloads) > 1:
        raise ValueError("Multiple ESP32-S3 download ports match the selected device")
    port = downloads[0].device if downloads else esp.enter_download_mode(serial_number)
    output = esptool(port, ["read_mac"])
    matches = re.findall(r"MAC:\s*([0-9a-fA-F:]{17})", output)
    if not matches or not esp.serials_match(serial_number, matches[-1]):
        raise ValueError("ESP32-S3 chip MAC does not match the selected device")
    return port


def backup(board: str, serial_number: str, destination: Path) -> dict:
    destination = destination.expanduser().absolute()
    if destination.suffix.lower() != BOARDS[board][2]:
        raise ValueError(f"Backup filename must end in {BOARDS[board][2]}")
    # Stage beside the destination so a failed read never replaces a valid backup.
    with tempfile.TemporaryDirectory(prefix=".kivo-backup-", dir=destination.parent) as staging:
        image = Path(staging) / destination.name
        emit("connecting")
        if board == "yd-rp2040":
            target = prepare_rp(serial_number)
            selector = rp.target_arguments(target)
            reboot = lambda: picotool(["reboot", *selector])
            read = lambda: picotool(["save", "-a", str(image), "-t", "uf2", *selector])
        else:
            port = prepare_esp(serial_number)
            reboot = lambda: esptool(port, ["run"], after="hard_reset")
            read = lambda: esptool(port, ["read_flash", "0", "ALL", str(image)])
        warning = None
        try:
            emit("reading")
            read()
            data = image.read_bytes()
            if len(data) < 256 * 1024:
                raise ValueError("Flash backup is unexpectedly small")
            with image.open("rb+") as handle:
                os.fsync(handle.fileno())
            os.replace(image, destination)
        finally:
            emit("rebooting")
            try:
                reboot()
                wait_for_runtime_port(serial_number, BOARDS[board][0])
            except Exception as error:
                warning = f"Backup read finished, but automatic restart failed: {error}"
        return {"path": str(destination), "bytes": len(data),
                "sha256": hashlib.sha256(data).hexdigest(), "warning": warning}


def flash(board: str, serial_number: str, path: Path, expected_sha256: str) -> dict:
    firmware = inspect_firmware(path, board)
    if firmware["sha256"] != expected_sha256:
        raise ValueError("Firmware changed after confirmation; select it again")
    emit("checking")
    validate_runtime_product(board, serial_number, firmware["productVersionId"])
    # Keep the exact inspected bytes even if another build replaces the source file.
    with tempfile.TemporaryDirectory(prefix="kivo-flash-") as staging:
        image = Path(staging) / Path(firmware["path"]).name
        data = Path(firmware["path"]).read_bytes()
        if hashlib.sha256(data).hexdigest() != expected_sha256:
            raise ValueError("Firmware changed during preparation")
        image.write_bytes(data)
        emit("connecting")
        if board == "yd-rp2040":
            target = prepare_rp(serial_number)
            emit("writing")
            picotool(["load", "-v", "-x", str(image), *rp.target_arguments(target)])
        else:
            port = prepare_esp(serial_number)
            emit("writing")
            esptool(port, ["write_flash", "0x0", str(image)], after="hard_reset")
    emit("verifying")
    verify_runtime_firmware(serial_number, BOARDS[board][0], BOARDS[board][1], board,
                            firmware["buildId"], firmware["productVersionId"])
    return firmware


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("operation", choices=("inspect", "backup", "flash"))
    parser.add_argument("--board", required=True, choices=BOARDS)
    parser.add_argument("--path", type=Path, required=True)
    parser.add_argument("--serial")
    parser.add_argument("--sha256")
    args = parser.parse_args()
    try:
        if args.operation == "inspect":
            result = inspect_firmware(args.path, args.board)
        else:
            serial_number = esp.require_serial(args.serial)
            result = (backup(args.board, serial_number, args.path)
                      if args.operation == "backup"
                      else flash(args.board, serial_number, args.path, args.sha256))
        emit("complete", **result)
    except Exception as error:
        emit("error", message=str(error))
        raise SystemExit(1) from error


if __name__ == "__main__":
    main()
