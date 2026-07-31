from dataclasses import dataclass
from types import SimpleNamespace
import subprocess

import pytest

from scripts.enter_download_mode import (
    TargetError,
    require_serial,
    select_download_port,
    select_runtime_port,
)
from scripts.list_firmware_targets import (
    InventoryError,
    macos_uf2_rows,
    merge_rows,
)


@dataclass
class FakePort:
    device: str
    vid: int
    pid: int
    serial_number: str | None
    location: str | None = None


def test_select_port_requires_exact_serial() -> None:
    ports = [
        FakePort("/dev/a", 0x303A, 0x4002, "AAA"),
        FakePort("/dev/b", 0x303A, 0x4002, "BBB"),
    ]

    assert select_runtime_port(ports, (0x303A, 0x4002), "BBB").device == "/dev/b"
    with pytest.raises(TargetError, match="serial CCC not found"):
        select_runtime_port(ports, (0x303A, 0x4002), "CCC")


def test_missing_serial_argument_is_rejected() -> None:
    with pytest.raises(TargetError, match="SERIAL is required"):
        require_serial("")


def test_select_port_rejects_duplicate_matching_serial() -> None:
    ports = [
        FakePort("/dev/a", 0x303A, 0x4002, "AAA"),
        FakePort("/dev/b", 0x303A, 0x4002, "AAA"),
    ]

    with pytest.raises(TargetError, match="multiple ports"):
        select_runtime_port(ports, (0x303A, 0x4002), "AAA")


def test_select_download_port_uses_runtime_location_and_serial_when_present() -> None:
    ports = [
        FakePort("/dev/other", 0x303A, 0x1001, "OTHER", "1-2"),
        FakePort("/dev/target", 0x303A, 0x1001, None, "1-1"),
    ]

    assert select_download_port(ports, "TARGET", "1-1").device == "/dev/target"


def test_select_download_port_rejects_missing_or_ambiguous_location() -> None:
    ports = [FakePort("/dev/a", 0x303A, 0x1001, None, "1-1")]

    with pytest.raises(TargetError, match="USB location is required"):
        select_download_port(ports, "TARGET", None)
    with pytest.raises(TargetError, match="not found"):
        select_download_port(ports, "TARGET", "1-2")

    ambiguous = ports + [FakePort("/dev/b", 0x303A, 0x1001, None, "1-1")]
    with pytest.raises(TargetError, match="multiple ports"):
        select_download_port(ambiguous, "TARGET", "1-1")


def test_select_download_port_rejects_same_location_with_other_serial() -> None:
    ports = [FakePort("/dev/other", 0x303A, 0x1001, "OTHER", "1-1")]

    with pytest.raises(TargetError, match="serial TARGET not found"):
        select_download_port(ports, "TARGET", "1-1")


def test_inventory_merges_duplicate_identity_and_prefers_cdc_port() -> None:
    merged = merge_rows(
        [
            ("bootloader", (0x2E8A, 0x0003), "vccgnd-yd-rp2040", "RP1", None),
            ("bootloader", (0x2E8A, 0x0003), "vccgnd-yd-rp2040", "RP1", "/dev/tty.usbmodem"),
            ("bootloader", (0x2E8A, 0x0003), "vccgnd-yd-rp2040", "RP2", None),
        ]
    )

    assert merged == [
        ("bootloader", (0x2E8A, 0x0003), "vccgnd-yd-rp2040", "RP1", "/dev/tty.usbmodem"),
        ("bootloader", (0x2E8A, 0x0003), "vccgnd-yd-rp2040", "RP2", None),
    ]


def test_inventory_preserves_distinct_serialless_cdc_ports() -> None:
    rows = [
        ("bootloader", (0x2E8A, 0x0003), "vccgnd-yd-rp2040", None, "/dev/tty.a"),
        ("bootloader", (0x2E8A, 0x0003), "vccgnd-yd-rp2040", None, "/dev/tty.b"),
    ]

    assert merge_rows(rows) == rows


def test_inventory_preserves_serialless_profiler_rows_in_observation_order() -> None:
    rows = [
        ("bootloader", (0x2E8A, 0x0003), "vccgnd-yd-rp2040", None, None),
        ("bootloader", (0x2E8A, 0x0003), "vccgnd-yd-rp2040", None, None),
    ]

    assert merge_rows(rows) == rows


@pytest.mark.parametrize(
    ("runner", "error"),
    [
        (
            lambda *_args, **_kwargs: SimpleNamespace(returncode=1, stderr="profiler failed", stdout=""),
            "profiler failed",
        ),
        (
            lambda *_args, **_kwargs: SimpleNamespace(returncode=0, stderr="", stdout="not json"),
            "invalid JSON",
        ),
        (
            lambda *_args, **_kwargs: (_ for _ in ()).throw(subprocess.TimeoutExpired("system_profiler", 10)),
            "timed out",
        ),
    ],
)
def test_inventory_surfaces_system_profiler_failures(runner: object, error: str) -> None:
    with pytest.raises(InventoryError, match=error):
        list(macos_uf2_rows(run=runner, system_name="Darwin"))


def test_inventory_parses_structured_uf2_json() -> None:
    runner = lambda *_args, **_kwargs: SimpleNamespace(
        returncode=0,
        stderr="",
        stdout='{"SPUSBDataType": [{"vendor_id": "0x2e8a", "product_id": "0x0003", "serial_num": "RP1"}]}',
    )

    assert list(macos_uf2_rows(run=runner, system_name="Darwin")) == [
        ("bootloader", (0x2E8A, 0x0003), "vccgnd-yd-rp2040", "RP1", None)
    ]
