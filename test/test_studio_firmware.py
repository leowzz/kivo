import hashlib
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
from unittest.mock import Mock

import pytest

from scripts import studio_firmware as fw


def artifact(tmp_path, board="yd-rp2040"):
    path = tmp_path / ("firmware.uf2" if board == "yd-rp2040" else "firmware.factory.bin")
    path.write_bytes(b"firmware")
    manifest = {
        "board_profile_id": board, "firmware_file": path.name,
        "firmware_size": path.stat().st_size,
        "firmware_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "product_version_id": "key-rp-k1-r01", "build_id": "new-build",
    }
    path.with_name("manifest.json").write_text(json.dumps(manifest))
    return path, manifest


def test_inspection_validates_board_and_content_before_touching_device(tmp_path):
    path, manifest = artifact(tmp_path)
    assert fw.inspect_firmware(path, "yd-rp2040")["sha256"] == manifest["firmware_sha256"]
    with pytest.raises(ValueError, match="board"):
        fw.inspect_firmware(path, "yd-esp32-s3")
    path.write_bytes(b"tampered")
    with pytest.raises(ValueError, match="checksum"):
        fw.inspect_firmware(path, "yd-rp2040")


def test_esp_requires_merged_image(tmp_path):
    path, manifest = artifact(tmp_path, "yd-esp32-s3")
    assert fw.inspect_firmware(path, "yd-esp32-s3")["path"] == str(path)
    renamed = path.with_name("firmware.bin")
    path.rename(renamed)
    manifest["firmware_file"] = renamed.name
    renamed.with_name("manifest.json").write_text(json.dumps(manifest))
    with pytest.raises(ValueError, match="merged"):
        fw.inspect_firmware(renamed, "yd-esp32-s3")


@pytest.fixture
def rp_device(monkeypatch):
    target = fw.rp.UsbDevice(fw.rp.RP2040_BOOTSEL_USB_ID, "ROM", "slot", 1, 9)
    monkeypatch.setattr(fw, "prepare_rp", Mock(return_value=target))
    monkeypatch.setattr(fw, "wait_for_runtime_port", Mock())
    return target


def test_rp_backup_reads_all_flash_atomically_and_reboots(tmp_path, monkeypatch, rp_device):
    destination = tmp_path / "original.uf2"
    destination.write_bytes(b"previous backup")
    data = b"x" * (512 * 1024)

    def tool(args):
        assert args[-4:] == ["--bus", "1", "--address", "9"]
        if args[0] == "save":
            assert args[1] == "-a"
            assert destination.read_bytes() == b"previous backup"
            Path(args[2]).write_bytes(data)
        return ""

    command = Mock(side_effect=tool)
    monkeypatch.setattr(fw, "picotool", command)
    result = fw.backup("yd-rp2040", "SERIAL", destination)
    assert destination.read_bytes() == data
    assert result["sha256"] == hashlib.sha256(data).hexdigest()
    assert result["warning"] is None
    assert command.call_args.args[0][0] == "reboot"


def test_failed_read_keeps_existing_backup_and_still_attempts_reboot(tmp_path, monkeypatch, rp_device):
    destination = tmp_path / "original.uf2"
    destination.write_bytes(b"previous backup")

    def tool(args):
        if args[0] == "save":
            Path(args[2]).write_bytes(b"partial")
            raise RuntimeError("USB disconnected")
        return ""

    command = Mock(side_effect=tool)
    monkeypatch.setattr(fw, "picotool", command)
    with pytest.raises(RuntimeError, match="disconnected"):
        fw.backup("yd-rp2040", "SERIAL", destination)
    assert destination.read_bytes() == b"previous backup"
    assert list(tmp_path.iterdir()) == [destination]
    assert command.call_args.args[0][0] == "reboot"


def test_backup_survives_reboot_failure(tmp_path, monkeypatch, rp_device):
    def tool(args):
        if args[0] == "save":
            Path(args[2]).write_bytes(b"x" * (512 * 1024))
        else:
            raise RuntimeError("reboot failed")
    monkeypatch.setattr(fw, "picotool", tool)
    result = fw.backup("yd-rp2040", "SERIAL", tmp_path / "original.uf2")
    assert Path(result["path"]).is_file()
    assert "reboot failed" in result["warning"]


def test_esp_backup_uses_detected_full_flash(tmp_path, monkeypatch):
    monkeypatch.setattr(fw, "prepare_esp", Mock(return_value="COM7"))
    monkeypatch.setattr(fw, "wait_for_runtime_port", Mock())
    def tool(port, args, after="no_reset"):
        assert port == "COM7"
        if args[0] == "read_flash":
            assert args[1:3] == ["0", "ALL"]
            Path(args[3]).write_bytes(b"x" * (1024 * 1024))
        else:
            assert args == ["run"] and after == "hard_reset"
    monkeypatch.setattr(fw, "esptool", tool)
    assert fw.backup("yd-esp32-s3", "SERIAL", tmp_path / "original.bin")["bytes"] == 1024 * 1024


def test_flash_rejects_changed_confirmation_before_bootloader(tmp_path, monkeypatch, rp_device):
    path, _ = artifact(tmp_path)
    with pytest.raises(ValueError, match="changed after confirmation"):
        fw.flash("yd-rp2040", "SERIAL", path, "wrong-hash")
    fw.prepare_rp.assert_not_called()


def test_flash_verifies_written_bytes_and_runtime_identity(tmp_path, monkeypatch, rp_device):
    path, manifest = artifact(tmp_path)
    product_check = Mock()
    runtime_check = Mock()
    monkeypatch.setattr(fw, "validate_runtime_product", product_check)
    monkeypatch.setattr(fw, "verify_runtime_firmware", runtime_check)
    def tool(args):
        assert args[:3] == ["load", "-v", "-x"]
        assert Path(args[3]).read_bytes() == b"firmware"
        assert Path(args[3]) != path
    monkeypatch.setattr(fw, "picotool", tool)
    fw.flash("yd-rp2040", "SERIAL", path, manifest["firmware_sha256"])
    product_check.assert_called_once_with("yd-rp2040", "SERIAL", "key-rp-k1-r01")
    runtime_check.assert_called_once_with("SERIAL", (0x2E8A, 0x102E), "rp2040", "yd-rp2040", "new-build", "key-rp-k1-r01")


def test_runtime_product_mismatch_is_rejected(monkeypatch):
    port = SimpleNamespace(vid=0x2E8A, pid=0x102E, serial_number="SERIAL", device="port")
    monkeypatch.setattr(fw, "comports", lambda: [port])
    connection = Mock()
    connection.readline.return_value = b"HELLO 13 rp2040 yd-rp2040 old other-product more\n"
    context = Mock(__enter__=Mock(return_value=connection), __exit__=Mock(return_value=False))
    monkeypatch.setattr(fw.serial, "Serial", Mock(return_value=context))
    with pytest.raises(ValueError, match="Product mismatch"):
        fw.validate_runtime_product("yd-rp2040", "SERIAL", "key-rp-k1-r01")


@pytest.mark.parametrize("reply", [
    b"HELLO 8 rp2040 vccgnd-yd-rp2040 old 1 1\n",
    b"HELLO 9 rp2040 yd-rp2040 old - 1 1\n",
    b"HELLO 13 rp2040 yd-rp2040 old key-rp-k1-r01 1 1\n",
])
def test_legacy_generic_and_matching_product_firmware_can_be_upgraded(monkeypatch, reply):
    port = SimpleNamespace(vid=0x2E8A, pid=0x102E, serial_number="SERIAL", device="port")
    monkeypatch.setattr(fw, "comports", lambda: [port])
    connection = Mock()
    connection.readline.return_value = reply
    context = Mock(__enter__=Mock(return_value=connection), __exit__=Mock(return_value=False))
    monkeypatch.setattr(fw.serial, "Serial", Mock(return_value=context))
    fw.validate_runtime_product("yd-rp2040", "SERIAL", "key-rp-k1-r01")


def test_esp_bootloader_retry_requires_matching_chip_mac(monkeypatch):
    port = SimpleNamespace(vid=0x303A, pid=0x1001, serial_number="AA:BB:CC:DD:EE:FF", device="COM9")
    monkeypatch.setattr(fw, "comports", lambda: [port])
    monkeypatch.setattr(fw, "esptool", Mock(return_value="MAC: aa:bb:cc:dd:ee:ff"))
    assert fw.prepare_esp("AABBCCDDEEFF") == "COM9"
    fw.esptool.return_value = "MAC: 11:22:33:44:55:66"
    with pytest.raises(ValueError, match="MAC"):
        fw.prepare_esp("AABBCCDDEEFF")


def test_tools_have_bounded_execution_and_report_failures(monkeypatch):
    runner = Mock(return_value=subprocess.CompletedProcess([], 1, "", "USB lost"))
    monkeypatch.setattr(fw.subprocess, "run", runner)
    with pytest.raises(RuntimeError, match="USB lost"):
        fw.picotool(["save", "-a", "output.uf2"])
    assert runner.call_args.kwargs["timeout"] == 240
