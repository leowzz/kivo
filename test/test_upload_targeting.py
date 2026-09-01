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
    windows_uf2_rows,
)
from scripts.verify_runtime_firmware import verify_runtime_firmware, wait_for_expected_hello
from scripts.windows_usb import WindowsUsbDevice, parse_windows_usb_devices


@dataclass
class FakePort:
    device: str
    vid: int
    pid: int
    serial_number: str | None
    location: str | None = None


class FakeRuntimeSerial:
    def __init__(self, responses: list[bytes]) -> None:
        self.responses = iter(responses)
        self.write_count = 0

    def __enter__(self) -> "FakeRuntimeSerial":
        return self

    def __exit__(self, *_args: object) -> None:
        return None

    def write(self, payload: bytes) -> None:
        assert payload == b"HELLO\n"
        self.write_count += 1

    def readline(self) -> bytes:
        return next(self.responses)


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


def test_select_download_port_accepts_equivalent_hex_serial_format() -> None:
    ports = [
        FakePort("/dev/target", 0x303A, 0x1001, "68:B6:B3:3D:9F:58", "1-1"),
    ]

    assert select_download_port(ports, "68B6B33D9F58", "1-1").device == "/dev/target"


@pytest.mark.parametrize(
    "observed",
    ["6:8:B6:B3:3D:9F:58", "68-B6:B3-3D:9F-58", "68:B6:B3:3D:9F:59"],
)
def test_select_download_port_rejects_malformed_or_different_hex_serial(observed: str) -> None:
    ports = [FakePort("/dev/target", 0x303A, 0x1001, observed, "1-1")]

    with pytest.raises(TargetError, match="not found"):
        select_download_port(ports, "68B6B33D9F58", "1-1")


def test_runtime_verifier_retries_until_protocol_is_ready() -> None:
    runtime = FakeRuntimeSerial(
        [b"", b"HELLO 7 esp32s3 yd-esp32-s3 acceptance\n"]
    )
    opened: list[str] = []
    times = iter([0.0, 0.0, 0.1])

    wait_for_expected_hello(
        "/dev/target",
        ["HELLO", "7", "esp32s3", "yd-esp32-s3", "acceptance"],
        timeout=1,
        serial_factory=lambda port, *_args, **_kwargs: opened.append(port) or runtime,
        monotonic=lambda: next(times),
        sleep=lambda _duration: None,
    )
    assert opened == ["/dev/target"]
    assert runtime.write_count == 2


def test_runtime_verifier_requires_generic_protocol_v12(monkeypatch: pytest.MonkeyPatch) -> None:
    observed: list[tuple[str, list[str]]] = []
    monkeypatch.setattr(
        "scripts.verify_runtime_firmware.wait_for_runtime_port",
        lambda *_args: SimpleNamespace(device="/dev/target"),
    )
    monkeypatch.setattr(
        "scripts.verify_runtime_firmware.wait_for_expected_hello",
        lambda port, expected: observed.append((port, expected)),
    )

    verify_runtime_firmware(
        "TARGET",
        (0x2E8A, 0x102E),
        "rp2040",
        "yd-rp2040",
        "v0.6.1",
    )

    assert observed == [
        (
            "/dev/target",
            ["HELLO", "12", "rp2040", "yd-rp2040", "v0.6.1", "-"],
        )
    ]


def test_runtime_verifier_bounds_timeout_and_reports_expected_and_observed() -> None:
    runtime = FakeRuntimeSerial([b"", b"WRONG 3 response\n"])
    times = iter([0.0, 0.0, 0.5, 1.0])

    with pytest.raises(TargetError) as captured:
        wait_for_expected_hello(
            "/dev/target",
            ["HELLO", "7", "esp32s3", "yd-esp32-s3", "acceptance"],
            timeout=1,
            serial_factory=lambda *_args, **_kwargs: runtime,
            monotonic=lambda: next(times),
            sleep=lambda _duration: None,
        )

    assert runtime.write_count == 2
    assert "timed out" in str(captured.value)
    assert "HELLO 7 esp32s3 yd-esp32-s3 acceptance" in str(captured.value)
    assert "WRONG 3 response" in str(captured.value)


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
            ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", None),
            ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", "/dev/tty.usbmodem"),
            ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP2", None),
        ]
    )

    assert merged == [
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", "/dev/tty.usbmodem"),
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP2", None),
    ]


def test_inventory_preserves_same_serial_on_distinct_concrete_ports() -> None:
    rows = [
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", "/dev/tty.a"),
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", "/dev/tty.b"),
    ]

    assert merge_rows(rows) == rows


def test_inventory_reconciles_one_portless_row_against_two_concrete_rows() -> None:
    rows = [
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", None),
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", "/dev/tty.a"),
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", "/dev/tty.b"),
    ]

    assert merge_rows(rows) == rows[1:]


def test_inventory_retains_unmatched_portless_observation() -> None:
    rows = [
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", None),
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", None),
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", "/dev/tty.a"),
    ]

    assert merge_rows(rows) == [rows[2], rows[0]]


def test_inventory_deduplicates_exact_concrete_observations() -> None:
    row = ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", "/dev/tty.a")

    assert merge_rows([row, row]) == [row]


def test_inventory_preserves_distinct_serialless_cdc_ports() -> None:
    rows = [
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", None, "/dev/tty.a"),
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", None, "/dev/tty.b"),
    ]

    assert merge_rows(rows) == rows


def test_inventory_preserves_serialless_profiler_rows_in_observation_order() -> None:
    rows = [
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", None, None),
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", None, None),
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
        ("bootloader", (0x2E8A, 0x0003), "yd-rp2040", "RP1", None)
    ]


def test_windows_inventory_parses_base_pnp_devices_and_locations() -> None:
    output = """[
      {
        "instance_id": "USB\\\\VID_2E8A&PID_102E\\\\RUNTIME-1",
        "location": "PCIROOT(0)#USBROOT(0)#USB(3)"
      },
      {
        "instance_id": "USB\\\\VID_2E8A&PID_0003\\\\BOOTSEL-1",
        "location": "PCIROOT(0)#USBROOT(0)#USB(3)"
      },
      {
        "instance_id": "USB\\\\VID_2E8A&PID_0003&MI_00\\\\INTERFACE",
        "location": "ignored"
      }
    ]"""

    assert parse_windows_usb_devices(
        output, {(0x2E8A, 0x102E), (0x2E8A, 0x0003)}
    ) == [
        WindowsUsbDevice(
            usb_id=(0x2E8A, 0x102E),
            serial_number="RUNTIME-1",
            location="pciroot(0)#usbroot(0)#usb(3)",
        ),
        WindowsUsbDevice(
            usb_id=(0x2E8A, 0x0003),
            serial_number="BOOTSEL-1",
            location="pciroot(0)#usbroot(0)#usb(3)",
        ),
    ]


def test_windows_inventory_adds_bootsel_target_to_picker() -> None:
    runner = lambda *_args, **_kwargs: SimpleNamespace(
        returncode=0,
        stderr="",
        stdout=(
            '[{"instance_id":"USB\\\\VID_2E8A&PID_0003\\\\BOOTSEL-1",'
            '"location":"PCIROOT(0)#USBROOT(0)#USB(3)"}]'
        ),
    )

    assert list(windows_uf2_rows(run=runner, system_name="Windows")) == [
        (
            "bootloader",
            (0x2E8A, 0x0003),
            "yd-rp2040",
            "BOOTSEL-1",
            None,
        )
    ]
