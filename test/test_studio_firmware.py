import hashlib
import json
from pathlib import Path
import subprocess
import struct
from types import SimpleNamespace
from unittest.mock import Mock

import pytest

from scripts import studio_firmware as fw


def uf2(blocks=1):
    return b"".join(struct.pack("<8I", 0x0A324655, 0x9E5D5157, 0x2000,
        0x10000000 + block * 256, 256, block, blocks, 0xE48BFF56)
        + b"x" * 476 + struct.pack("<I", 0x0AB16F30) for block in range(blocks))


def esp_image():
    data = bytearray(64)
    data[0] = 0xE9
    struct.pack_into("<H", data, 12, 9)
    return bytes(data)


def artifact(tmp_path, board="yd-rp2040"):
    path = tmp_path / ("firmware.uf2" if board == "yd-rp2040" else "firmware.factory.bin")
    path.write_bytes(uf2() if board == "yd-rp2040" else esp_image())
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
    data = bytearray(path.read_bytes())
    data[32] ^= 1
    path.write_bytes(data)
    with pytest.raises(ValueError, match="checksum"):
        fw.inspect_firmware(path, "yd-rp2040")


def test_esp_rejects_application_only_image_at_zero(tmp_path):
    path, manifest = artifact(tmp_path, "yd-esp32-s3")
    assert fw.inspect_firmware(path, "yd-esp32-s3")["path"] == str(path)
    renamed = path.with_name("firmware.bin")
    path.rename(renamed)
    manifest["firmware_file"] = renamed.name
    renamed.with_name("manifest.json").write_text(json.dumps(manifest))
    assert fw.inspect_firmware(renamed, "yd-esp32-s3")["path"] == str(renamed)
    data = bytearray(renamed.read_bytes())
    struct.pack_into("<I", data, 32, 0xABCD5432)
    renamed.write_bytes(data)
    with pytest.raises(ValueError, match="Application-only"):
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
    runtime_check = Mock()
    monkeypatch.setattr(fw, "verify_runtime_firmware", runtime_check)
    def tool(args):
        assert args[:3] == ["load", "-v", "-x"]
        assert Path(args[3]).read_bytes() == path.read_bytes()
        assert Path(args[3]) != path
    monkeypatch.setattr(fw, "picotool", tool)
    fw.flash("yd-rp2040", "SERIAL", path, manifest["firmware_sha256"])
    runtime_check.assert_called_once_with("SERIAL", (0x2E8A, 0x102E), "rp2040", "yd-rp2040", "new-build", "key-rp-k1-r01")


def test_current_product_identity_can_be_read_before_temporary_replacement(monkeypatch):
    port = SimpleNamespace(vid=0x2E8A, pid=0x102E, serial_number="SERIAL", device="port")
    monkeypatch.setattr(fw, "comports", lambda: [port])
    connection = Mock()
    connection.readline.return_value = b"HELLO 13 rp2040 yd-rp2040 old other-product more\n"
    context = Mock(__enter__=Mock(return_value=connection), __exit__=Mock(return_value=False))
    monkeypatch.setattr(fw.serial, "Serial", Mock(return_value=context))
    assert fw.read_runtime_hello("yd-rp2040", "SERIAL")["productVersionId"] == "other-product"


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
    assert fw.read_runtime_hello("yd-rp2040", "SERIAL")["buildId"] == "old"


def test_raw_uf2_and_full_bin_do_not_require_product_manifests(tmp_path):
    for board, data in [("yd-rp2040", uf2()), ("yd-esp32-s3", esp_image())]:
        path = tmp_path / ("raw" + fw.BOARDS[board][2])
        path.write_bytes(data)
        assert "productVersionId" not in fw.inspect_firmware(path, board)


@pytest.mark.parametrize("offset,value", [(28, 0xE48BFF59), (12, 0x20000000), (8, 1), (24, 2)])
def test_uf2_rejects_wrong_chip_ram_targets_and_incomplete_blocks(tmp_path, offset, value):
    data = bytearray(uf2())
    struct.pack_into("<I", data, offset, value)
    path = tmp_path / "invalid.uf2"
    path.write_bytes(data)
    with pytest.raises(ValueError, match="RP2040"):
        fw.inspect_firmware(path, "yd-rp2040")


@pytest.fixture
def test_install(tmp_path, monkeypatch):
    path = tmp_path / "test.uf2"
    path.write_bytes(uf2())
    image = {**fw.inspect_firmware(path, "yd-rp2040"), "buildId": "io-test-unit"}
    monkeypatch.setattr(fw, "build_io_test", Mock(return_value=image))
    monkeypatch.setattr(fw, "read_runtime_hello", Mock(return_value={"buildId": "original", "productVersionId": "original-product"}))
    monkeypatch.setattr(fw, "write_image", Mock())
    monkeypatch.setattr(fw, "verify_runtime_firmware", Mock())
    def backup(board, serial_number, destination):
        destination.write_bytes(uf2(1024))
        digest = hashlib.sha256(destination.read_bytes()).hexdigest()
        fw.atomic_json(destination.with_suffix(".uf2.backup.json"), {"boardProfileId": board, "serial": serial_number, "sha256": digest})
        return {"path": str(destination), "sha256": digest, "bytes": destination.stat().st_size}
    monkeypatch.setattr(fw, "backup", Mock(side_effect=backup))
    return image


def test_test_firmware_install_backs_up_once_and_retains_recovery_across_retries(tmp_path, monkeypatch, test_install):
    fw.write_image.side_effect = RuntimeError("USB lost")
    with pytest.raises(RuntimeError, match="USB lost"):
        fw.install_io_test(tmp_path, "yd-rp2040", "SERIAL")
    status = fw.recovery_status(tmp_path, "yd-rp2040", "SERIAL")
    assert Path(status["backupPath"]).is_file()
    fw.write_image.side_effect = None
    fw.read_runtime_hello.return_value = {"buildId": "io-test-unit"}
    result = fw.install_io_test(tmp_path, "yd-rp2040", "SERIAL")
    assert result["backupPath"] == status["backupPath"]
    fw.backup.assert_called_once()
    fw.verify_runtime_firmware.assert_called_once_with("SERIAL", (0x2E8A, 0x102E), "rp2040", "yd-rp2040", "io-test-unit", "-")


def test_failed_backup_prevents_test_firmware_write(tmp_path, monkeypatch, test_install):
    fw.backup.side_effect = RuntimeError("read failed")
    with pytest.raises(RuntimeError, match="read failed"):
        fw.install_io_test(tmp_path, "yd-rp2040", "SERIAL")
    fw.write_image.assert_not_called()


def test_existing_test_firmware_without_original_backup_is_not_saved_as_original(tmp_path, test_install):
    fw.read_runtime_hello.return_value = {"buildId": "io-test-existing"}
    with pytest.raises(ValueError, match="original backup"):
        fw.install_io_test(tmp_path, "yd-rp2040", "SERIAL")
    fw.backup.assert_not_called()
    fw.write_image.assert_not_called()


def test_backup_files_restore_without_a_product_manifest_and_end_test_session(tmp_path, monkeypatch, test_install):
    installed = fw.install_io_test(tmp_path, "yd-rp2040", "SERIAL")
    monkeypatch.setattr(fw, "wait_for_runtime_port", Mock())
    backup = Path(installed["backupPath"])
    result = fw.flash("yd-rp2040", "SERIAL", backup, installed["backupSha256"], root=tmp_path)
    assert not result.get("warning")
    assert fw.recovery_status(tmp_path, "yd-rp2040", "SERIAL") == {}
    assert backup.is_file()
    with pytest.raises(ValueError, match="another device"):
        fw.flash("yd-rp2040", "OTHER", backup, installed["backupSha256"])


def test_damaged_original_backup_blocks_reinstall(tmp_path, test_install):
    installed = fw.install_io_test(tmp_path, "yd-rp2040", "SERIAL")
    Path(installed["backupPath"]).write_bytes(b"damaged")
    fw.write_image.reset_mock()
    with pytest.raises(ValueError, match="damaged"):
        fw.install_io_test(tmp_path, "yd-rp2040", "SERIAL")
    fw.write_image.assert_not_called()


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
