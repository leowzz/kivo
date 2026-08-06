from importlib import import_module
from pathlib import Path
from types import SimpleNamespace

import pytest
import serial


RUNTIME_SERIAL = "500315221060821C"
BOOTSEL_SERIAL = "E0C9125B0D9B"


class FakeTouchSerial:
    def __init__(self) -> None:
        self.dtr: bool | None = None
        self.closed = False

    def __enter__(self) -> "FakeTouchSerial":
        return self

    def __exit__(self, *_args: object) -> None:
        self.closed = True


def test_runtime_target_is_reset_verified_and_uploaded_by_usb_address() -> None:
    upload = import_module("scripts.upload_rp2040")
    runtime = upload.UsbDevice(
        usb_id=(0x2E8A, 0x102E),
        serial_number=RUNTIME_SERIAL,
        location="0x02100000",
        bus=2,
        address=1,
    )
    other_bootsel = upload.UsbDevice(
        usb_id=(0x2E8A, 0x0003),
        serial_number="OTHER-BOOTSEL",
        location="0x00100000",
        bus=0,
        address=4,
    )
    other_runtime = upload.UsbDevice(
        usb_id=(0x2E8A, 0x102E),
        serial_number="OTHER-RUNTIME",
        location="0x00100000",
        bus=0,
        address=1,
    )
    selected_bootsel = upload.UsbDevice(
        usb_id=(0x2E8A, 0x0003),
        serial_number=BOOTSEL_SERIAL,
        location="0x02100000",
        bus=2,
        address=7,
    )
    snapshots = iter(
        [
            [other_runtime, runtime, other_bootsel],
            [selected_bootsel, other_bootsel, other_runtime],
        ]
    )
    touch = FakeTouchSerial()
    picotool_calls: list[list[str]] = []

    def picotool(arguments: list[str]) -> str:
        picotool_calls.append(arguments)
        if arguments[:2] == ["info", "-a"]:
            return f"Device Information\n flash id: 0x{RUNTIME_SERIAL}\n"
        return ""

    result = upload.upload_rp2040(
        RUNTIME_SERIAL,
        Path("firmware.uf2"),
        usb_inventory=lambda: next(snapshots),
        ports=lambda: [
            SimpleNamespace(
                device="/dev/cu.other",
                vid=0x2E8A,
                pid=0x102E,
                serial_number="OTHER-RUNTIME",
            ),
            SimpleNamespace(
                device="/dev/cu.target",
                vid=0x2E8A,
                pid=0x102E,
                serial_number=RUNTIME_SERIAL,
            )
        ],
        serial_factory=lambda port, baudrate: (
            touch
            if (port, baudrate) == ("/dev/cu.target", 1200)
            else pytest.fail("opened the wrong runtime port")
        ),
        picotool=picotool,
        monotonic=iter([0.0, 0.1]).__next__,
        sleep=lambda _duration: None,
    )

    assert result == RUNTIME_SERIAL
    assert touch.dtr is False
    assert touch.closed
    assert picotool_calls == [
        ["info", "-a", "--bus", "2", "--address", "7"],
        [
            "load",
            "-x",
            "firmware.uf2",
            "--bus",
            "2",
            "--address",
            "7",
        ],
    ]


def test_bootsel_target_uses_flash_id_as_runtime_serial_without_touching_cdc() -> None:
    upload = import_module("scripts.upload_rp2040")
    target = upload.UsbDevice(
        usb_id=(0x2E8A, 0x0003),
        serial_number=BOOTSEL_SERIAL,
        location="0x02100000",
        bus=2,
        address=7,
    )
    other = upload.UsbDevice(
        usb_id=(0x2E8A, 0x0003),
        serial_number="OTHER-BOOTSEL",
        location="0x00100000",
        bus=0,
        address=4,
    )
    picotool_calls: list[list[str]] = []

    def picotool(arguments: list[str]) -> str:
        picotool_calls.append(arguments)
        return f"Device Information\n flash id: 0x{RUNTIME_SERIAL}\n"

    result = upload.upload_rp2040(
        BOOTSEL_SERIAL,
        Path("firmware.uf2"),
        usb_inventory=lambda: [other, target],
        ports=lambda: [],
        serial_factory=lambda *_args: pytest.fail("BOOTSEL target must not be touched"),
        picotool=picotool,
    )

    assert result == RUNTIME_SERIAL
    assert picotool_calls[-1] == [
        "load",
        "-x",
        "firmware.uf2",
        "--bus",
        "2",
        "--address",
        "7",
    ]


def test_runtime_target_aborts_when_bootsel_flash_identity_does_not_match() -> None:
    upload = import_module("scripts.upload_rp2040")
    runtime = upload.UsbDevice(
        (0x2E8A, 0x102E), RUNTIME_SERIAL, "0x02100000", 2, 1
    )
    wrong_bootsel = upload.UsbDevice(
        (0x2E8A, 0x0003), BOOTSEL_SERIAL, "0x02100000", 2, 7
    )
    snapshots = iter([[runtime], [wrong_bootsel]])
    picotool_calls: list[list[str]] = []

    def picotool(arguments: list[str]) -> str:
        picotool_calls.append(arguments)
        return "Device Information\n flash id: 0x1111222233334444\n"

    with pytest.raises(upload.TargetError, match="identity mismatch"):
        upload.upload_rp2040(
            RUNTIME_SERIAL,
            Path("firmware.uf2"),
            usb_inventory=lambda: next(snapshots),
            ports=lambda: [
                SimpleNamespace(
                    device="/dev/cu.target",
                    vid=0x2E8A,
                    pid=0x102E,
                    serial_number=RUNTIME_SERIAL,
                )
            ],
            serial_factory=lambda *_args: FakeTouchSerial(),
            picotool=picotool,
            monotonic=iter([0.0, 0.1]).__next__,
            sleep=lambda _duration: None,
        )

    assert len(picotool_calls) == 1
    assert picotool_calls[0][:2] == ["info", "-a"]


def test_busy_runtime_port_reports_how_to_release_it() -> None:
    upload = import_module("scripts.upload_rp2040")
    runtime = upload.UsbDevice(
        (0x2E8A, 0x102E), RUNTIME_SERIAL, "0x02100000", 2, 1
    )

    with pytest.raises(upload.TargetError, match="make helper-kill"):
        upload.upload_rp2040(
            RUNTIME_SERIAL,
            Path("firmware.uf2"),
            usb_inventory=lambda: [runtime],
            ports=lambda: [
                SimpleNamespace(
                    device="/dev/cu.target",
                    vid=0x2E8A,
                    pid=0x102E,
                    serial_number=RUNTIME_SERIAL,
                )
            ],
            serial_factory=lambda *_args: (_ for _ in ()).throw(
                serial.SerialException("resource busy")
            ),
            picotool=lambda _arguments: pytest.fail("must fail before picotool"),
        )


def test_parses_macos_usb_location_into_picotool_bus_and_address() -> None:
    upload = import_module("scripts.upload_rp2040")
    profiler_json = """
    {
      "SPUSBDataType": [{
        "_items": [{
          "_name": "RP2 Boot",
          "vendor_id": "0x2e8a  (Raspberry Pi)",
          "product_id": "0x0003 (RP2 Boot)",
          "serial_num": "E0C9125B0D9B",
          "location_id": "0x02100000 / 7"
        }]
      }]
    }
    """

    assert upload.parse_macos_usb_devices(profiler_json) == [
        upload.UsbDevice(
            (0x2E8A, 0x0003), BOOTSEL_SERIAL, "0x02100000", 2, 7
        )
    ]


def test_windows_runtime_target_is_matched_by_location_and_uploaded_by_serial() -> None:
    upload = import_module("scripts.upload_rp2040")
    location = "pciroot(0)#usbroot(0)#usb(3)"
    runtime = upload.UsbDevice(
        upload.RP2040_RUNTIME_USB_ID, RUNTIME_SERIAL, location, None, None
    )
    bootsel = upload.UsbDevice(
        upload.RP2040_BOOTSEL_USB_ID, BOOTSEL_SERIAL, location, None, None
    )
    snapshots = iter([[runtime], [bootsel]])
    calls: list[list[str]] = []

    def picotool(arguments: list[str]) -> str:
        calls.append(arguments)
        if arguments[:2] == ["info", "-a"]:
            return f"Device Information\n flash id: 0x{RUNTIME_SERIAL}\n"
        return ""

    assert upload.upload_rp2040(
        RUNTIME_SERIAL,
        Path("firmware.uf2"),
        usb_inventory=lambda: next(snapshots),
        ports=lambda: [
            SimpleNamespace(
                device="COM7",
                vid=0x2E8A,
                pid=0x102E,
                serial_number=RUNTIME_SERIAL,
            )
        ],
        serial_factory=lambda *_args: FakeTouchSerial(),
        picotool=picotool,
        monotonic=iter([0.0, 0.1]).__next__,
        sleep=lambda _duration: None,
    ) == RUNTIME_SERIAL
    assert calls == [
        ["info", "-a", "--ser", BOOTSEL_SERIAL],
        ["load", "-x", "firmware.uf2", "--ser", BOOTSEL_SERIAL],
    ]
