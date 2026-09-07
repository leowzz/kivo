"""Studio firmware operations. Device mutations require an explicit subcommand."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile
import time
import uuid

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
    if path.suffix.lower() != BOARDS[board][2]:
        raise ValueError("Firmware extension does not match the selected board")
    data = path.read_bytes()
    validate_image(data, board)
    digest = hashlib.sha256(data).hexdigest()
    result = {"path": str(path), "sha256": digest, "bytes": len(data), "boardProfileId": board}
    backup_metadata = path.with_suffix(path.suffix + ".backup.json")
    if backup_metadata.is_file():
        metadata = json.loads(backup_metadata.read_text(encoding="utf-8"))
        if metadata.get("boardProfileId") != board or metadata.get("sha256") != digest:
            raise ValueError("Backup board or checksum does not match its record")
        result["backupSerial"] = metadata["serial"]
        return result
    manifest_path = path.with_name("manifest.json")
    if not manifest_path.is_file():
        return result
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if not isinstance(manifest, dict):
        raise ValueError("Invalid firmware manifest")
    if manifest.get("board_profile_id") != board:
        raise ValueError("Firmware board does not match the selected device")
    if path.suffix.lower() != BOARDS[board][2] or manifest.get("firmware_file") != path.name:
        raise ValueError("Select the firmware file described by manifest.json")
    if not data or len(data) != manifest.get("firmware_size"):
        raise ValueError("Firmware size does not match manifest.json")
    if digest != manifest.get("firmware_sha256"):
        raise ValueError("Firmware checksum does not match manifest.json")
    for key in ("product_version_id", "build_id"):
        value = manifest.get(key)
        if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9_.-]{1,128}", value):
            raise ValueError(f"Invalid {key} in manifest.json")
    return {**result,
        "productVersionId": manifest["product_version_id"],
        "buildId": manifest["build_id"], "boardProfileId": board,
    }


def validate_image(data: bytes, board: str) -> None:
    if board == "yd-rp2040":
        if not data or len(data) % 512:
            raise ValueError("Invalid RP2040 UF2 image")
        count = len(data) // 512
        seen = set()
        for offset in range(0, len(data), 512):
            magic1, magic2, flags, address, size, block, total, family = struct.unpack_from("<8I", data, offset)
            end = struct.unpack_from("<I", data, offset + 508)[0]
            if (magic1, magic2, end) != (0x0A324655, 0x9E5D5157, 0x0AB16F30) or (
                flags & 1 or not flags & 0x2000 or family != 0xE48BFF56
                or size != 256 or not 0x10000000 <= address < 0x11000000
                or address % 256 or total != count or block >= count or block in seen
            ):
                raise ValueError("UF2 is not a complete RP2040 flash image")
            seen.add(block)
    elif (len(data) < 36 or data[0] != 0xE9 or struct.unpack_from("<H", data, 12)[0] != 9):
        raise ValueError("Invalid ESP32-S3 image; select a full flash or factory BIN")
    elif data[32:36] == struct.pack("<I", 0xABCD5432):
        raise ValueError("Application-only BIN cannot be written at 0x0; select a full flash or factory BIN")


def read_runtime_hello(board: str, serial_number: str) -> dict | None:
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
            return {"buildId": fields[4], "productVersionId": fields[5] if protocol >= 9 else "-"}
    raise ValueError("Cannot verify the installed product; device did not answer HELLO")


def atomic_json(path: Path, value: dict) -> None:
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent, delete=False) as handle:
        temporary = Path(handle.name)
        try:
            json.dump(value, handle)
            handle.flush()
            os.fsync(handle.fileno())
        except BaseException:
            temporary.unlink(missing_ok=True)
            raise
    try:
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def recovery_path(root: Path, board: str, serial_number: str) -> Path:
    serial_key = hashlib.sha256(serial_number.encode()).hexdigest()[:20]
    return root / "output" / "firmware-backups" / board / serial_key / "active.json"


def recovery_status(root: Path, board: str, serial_number: str) -> dict:
    record = recovery_path(root, board, serial_number)
    if not record.is_file():
        return {}
    metadata = json.loads(record.read_text(encoding="utf-8"))
    if metadata.get("boardProfileId") != board or metadata.get("serial") != serial_number:
        raise ValueError("Original firmware backup belongs to another device")
    path = Path(metadata["path"])
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != metadata["sha256"]:
        raise ValueError(f"Original firmware backup is missing or damaged: {path}")
    return {"backupPath": str(path), "backupSha256": metadata["sha256"]}


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
            atomic_json(destination.with_suffix(destination.suffix + ".backup.json"), {
                "boardProfileId": board, "serial": serial_number,
                "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data),
            })
        finally:
            emit("rebooting")
            try:
                reboot()
                wait_for_runtime_port(serial_number, BOARDS[board][0])
            except Exception as error:
                warning = f"Backup read finished, but automatic restart failed: {error}"
        return {"path": str(destination), "bytes": len(data),
                "sha256": hashlib.sha256(data).hexdigest(), "warning": warning}


def write_image(board: str, serial_number: str, path: Path, expected_sha256: str) -> None:
    with tempfile.TemporaryDirectory(prefix="kivo-flash-") as staging:
        image = Path(staging) / path.name
        data = path.read_bytes()
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
            esptool(port, ["write_flash", "--flash_mode", "keep", "--flash_freq", "keep",
                           "--flash_size", "keep", "0x0", str(image)])
            esptool(port, ["verify_flash", "0x0", str(image)])
            esptool(port, ["run"], after="hard_reset")


def flash(board: str, serial_number: str, path: Path, expected_sha256: str, *, root: Path | None = None) -> dict:
    firmware = inspect_firmware(path, board)
    if firmware["sha256"] != expected_sha256:
        raise ValueError("Firmware changed after confirmation; select it again")
    emit("checking")
    if firmware.get("backupSerial") and firmware["backupSerial"] != serial_number:
        raise ValueError("Firmware backup belongs to another device")
    # Keep the exact inspected bytes even if another build replaces the source file.
    write_image(board, serial_number, Path(firmware["path"]), expected_sha256)
    emit("verifying")
    if firmware.get("buildId"):
        verify_runtime_firmware(serial_number, BOARDS[board][0], BOARDS[board][1], board,
                            firmware["buildId"], firmware["productVersionId"])
    else:
        try:
            wait_for_runtime_port(serial_number, BOARDS[board][0])
        except Exception:
            firmware["warning"] = "Flash verified, but Kivo runtime USB was not detected. Reconnect the device if needed."
    if root is not None and not firmware.get("warning"):
        recovery_path(root, board, serial_number).unlink(missing_ok=True)
    return firmware


def build_io_test(root: Path, board: str) -> dict:
    sources = [root / "firmware/src/io_test.cpp", root / "platformio.ini"]
    sources.extend(sorted((root / "lib/gpio_trigger/src").glob("*")))
    digest = hashlib.sha256(b"".join(path.read_bytes() for path in sources if path.is_file())).hexdigest()[:12]
    build_id = f"io-test-{digest}"
    environment = f"io-test-{BOARDS[board][1]}"
    env = os.environ.copy()
    env.pop("KIVO_PRODUCT_GENERATED_DIR", None)
    env["KIVO_FIRMWARE_BUILD_ID"] = build_id
    env["PLATFORMIO_BUILD_DIR"] = str(root / ".pio/io-test")
    emit("building")
    result = subprocess.run(["pio", "run", "-e", environment], cwd=root, env=env,
                            capture_output=True, text=True, timeout=600)
    if result.returncode:
        raise RuntimeError((result.stdout + result.stderr)[-6000:])
    filename = "firmware.uf2" if board == "yd-rp2040" else "firmware.factory.bin"
    output = root / "output/io-test" / board / build_id / filename
    output.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(root / ".pio/io-test" / environment / filename, output)
    return {**inspect_firmware(output, board), "buildId": build_id}


def install_io_test(root: Path, board: str, serial_number: str) -> dict:
    firmware = build_io_test(root, board)
    recovery = recovery_status(root, board, serial_number)
    if not recovery:
        hello = read_runtime_hello(board, serial_number)
        if hello and hello["buildId"].startswith("io-test-"):
            raise ValueError("Device already runs I/O test firmware, but its original backup was not found")
        record = recovery_path(root, board, serial_number)
        destination = record.parent / uuid.uuid4().hex / ("original" + BOARDS[board][2])
        destination.parent.mkdir(parents=True, exist_ok=True)
        original = backup(board, serial_number, destination)
        atomic_json(record, {**original, "boardProfileId": board, "serial": serial_number})
        recovery = {"backupPath": original["path"], "backupSha256": original["sha256"]}
    emit("backup_saved", **recovery)
    write_image(board, serial_number, Path(firmware["path"]), firmware["sha256"])
    emit("verifying")
    verify_runtime_firmware(serial_number, BOARDS[board][0], BOARDS[board][1], board,
                            firmware["buildId"], "-")
    return {**firmware, **recovery}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("operation", choices=("inspect", "backup", "flash", "build_test", "install_test", "status"))
    parser.add_argument("--board", required=True, choices=BOARDS)
    parser.add_argument("--path", type=Path)
    parser.add_argument("--serial")
    parser.add_argument("--sha256")
    args = parser.parse_args()
    try:
        root = Path.cwd()
        serial_number = esp.require_serial(args.serial) if args.operation not in ("inspect", "build_test") else None
        if args.operation == "build_test":
            result = build_io_test(root, args.board)
        elif args.operation == "status":
            result = recovery_status(root, args.board, serial_number)
        elif args.operation == "install_test":
            result = install_io_test(root, args.board, serial_number)
        elif args.path is None:
            raise ValueError("A firmware path is required")
        elif args.operation == "inspect":
            result = inspect_firmware(args.path, args.board)
        else:
            result = (backup(args.board, serial_number, args.path)
                      if args.operation == "backup"
                      else flash(args.board, serial_number, args.path, args.sha256, root=root))
        emit("complete", **result)
    except Exception as error:
        emit("error", message=str(error))
        raise SystemExit(1) from error


if __name__ == "__main__":
    main()
