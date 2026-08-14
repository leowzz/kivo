import os
import subprocess
from pathlib import Path

import pytest

from scripts.resolve_firmware_port import PortResolutionError, resolve_runtime_port


ROOT = Path(__file__).resolve().parents[1]


def runtime_row(
    board: str, serial: str, port: str
) -> tuple[str, tuple[int, int], str, str, str]:
    usb_id = (0x2E8A, 0x102E) if board == "yd-rp2040" else (0x303A, 0x4002)
    return "runtime", usb_id, board, serial, port


def test_resolves_exact_runtime_board_and_serial() -> None:
    rows = [
        runtime_row("yd-rp2040", "RP-A", "/dev/rp-a"),
        runtime_row("yd-rp2040", "RP-B", "/dev/rp-b"),
        runtime_row("yd-esp32-s3", "ESP-A", "/dev/esp-a"),
    ]

    assert (
        resolve_runtime_port(rows, board="yd-rp2040", serial_number="RP-B")
        == "/dev/rp-b"
    )


def test_rejects_missing_and_ambiguous_runtime_ports() -> None:
    with pytest.raises(PortResolutionError, match="was not found"):
        resolve_runtime_port([], board="yd-rp2040", serial_number="MISSING")

    duplicate = [
        runtime_row("yd-rp2040", "RP-A", "/dev/rp-a"),
        runtime_row("yd-rp2040", "RP-A", "/dev/rp-a-duplicate"),
    ]
    with pytest.raises(PortResolutionError, match="multiple ports"):
        resolve_runtime_port(duplicate, board="yd-rp2040", serial_number="RP-A")


def run_make_monitor(
    tmp_path: Path,
    target: str,
    *,
    serial: str | None = None,
    baud: int | None = None,
) -> tuple[subprocess.CompletedProcess[str], list[str]]:
    log_path = tmp_path / "uv.log"
    fake_uv = tmp_path / "fake-uv"
    fake_uv.write_text(
        """#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$KIVO_TEST_LOG"
case " $* " in
  *" scripts/select_firmware_target.py "*) printf '%s\n' "SELECTED-SERIAL" ;;
  *" scripts/resolve_firmware_port.py "*) printf '%s\n' "/dev/fake-runtime" ;;
esac
"""
    )
    fake_uv.chmod(0o755)
    command = ["make", target, f"UV={fake_uv}"]
    if serial is not None:
        command.append(f"SERIAL={serial}")
    if baud is not None:
        command.append(f"BAUD={baud}")
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=os.environ | {"KIVO_TEST_LOG": str(log_path)},
        text=True,
        capture_output=True,
        check=False,
    )
    invocations = log_path.read_text().splitlines() if log_path.exists() else []
    return result, invocations


def test_default_monitor_selects_rp2040_and_opens_resolved_port(tmp_path: Path) -> None:
    result, invocations = run_make_monitor(tmp_path, "monitor")

    assert result.returncode == 0, result.stderr
    assert any("scripts/kill_helper.py" in line for line in invocations)
    assert any(
        "select_firmware_target.py --board yd-rp2040 --mode runtime" in line
        for line in invocations
    )
    assert any(
        "resolve_firmware_port.py --board yd-rp2040 --serial SELECTED-SERIAL"
        in line
        for line in invocations
    )
    assert any(
        "pio device monitor --port /dev/fake-runtime --baud 115200" in line
        for line in invocations
    )


def test_named_monitor_uses_explicit_serial_and_baud(tmp_path: Path) -> None:
    result, invocations = run_make_monitor(
        tmp_path,
        "monitor-esp32s3",
        serial="ESP-EXPLICIT",
        baud=921600,
    )

    assert result.returncode == 0, result.stderr
    assert not any("select_firmware_target.py" in line for line in invocations)
    assert any(
        "resolve_firmware_port.py --board yd-esp32-s3 --serial ESP-EXPLICIT"
        in line
        for line in invocations
    )
    assert any(
        "pio device monitor --port /dev/fake-runtime --baud 921600" in line
        for line in invocations
    )
