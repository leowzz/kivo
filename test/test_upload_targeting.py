from dataclasses import dataclass

import pytest

from scripts.enter_download_mode import (
    TargetError,
    require_serial,
    select_download_port,
    select_runtime_port,
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
