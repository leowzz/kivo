import json
import platform
import re
import subprocess
from collections.abc import Callable, Iterable
from dataclasses import dataclass


class WindowsUsbError(RuntimeError):
    pass


@dataclass(frozen=True)
class WindowsUsbDevice:
    usb_id: tuple[int, int]
    serial_number: str
    location: str | None


_PNP_DEVICE_SCRIPT = r"""
$ErrorActionPreference = 'Stop'
$devices = @(
  Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue -InstanceId `
    'USB\VID_303A&PID_4002\*', `
    'USB\VID_303A&PID_1001\*', `
    'USB\VID_2E8A&PID_102E\*', `
    'USB\VID_2E8A&PID_0003\*' | Where-Object {
      $_.InstanceId -match '^USB\\VID_[0-9A-F]{4}&PID_[0-9A-F]{4}\\'
    } | ForEach-Object {
    $location = @(
      (Get-PnpDeviceProperty -InstanceId $_.InstanceId `
        -KeyName 'DEVPKEY_Device_LocationPaths').Data
    )[0]
    [PSCustomObject]@{
      instance_id = $_.InstanceId
      location = $location
    }
  }
)
ConvertTo-Json -Compress -InputObject $devices
"""


def parse_windows_usb_devices(
    output: str,
    supported_ids: Iterable[tuple[int, int]],
) -> list[WindowsUsbDevice]:
    try:
        raw_devices = json.loads(output)
    except json.JSONDecodeError as error:
        raise WindowsUsbError("PowerShell returned invalid USB inventory JSON") from error
    if not isinstance(raw_devices, list):
        raise WindowsUsbError("PowerShell returned an invalid USB inventory")

    supported = set(supported_ids)
    devices = []
    pattern = re.compile(
        r"^USB\\VID_([0-9A-F]{4})&PID_([0-9A-F]{4})\\(.+)$",
        re.IGNORECASE,
    )
    for raw in raw_devices:
        if not isinstance(raw, dict):
            continue
        match = pattern.fullmatch(str(raw.get("instance_id", "")))
        if not match:
            continue
        usb_id = (int(match.group(1), 16), int(match.group(2), 16))
        if usb_id not in supported:
            continue
        location = raw.get("location")
        devices.append(
            WindowsUsbDevice(
                usb_id=usb_id,
                serial_number=match.group(3),
                location=str(location).casefold() if location else None,
            )
        )
    return devices


def scan_windows_usb_devices(
    supported_ids: Iterable[tuple[int, int]],
    *,
    run: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
    system_name: str | None = None,
) -> list[WindowsUsbDevice]:
    if (platform.system() if system_name is None else system_name) != "Windows":
        raise WindowsUsbError("Windows USB inventory requires Windows")
    try:
        result = run(
            [
                "powershell.exe",
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                _PNP_DEVICE_SCRIPT,
            ],
            check=False,
            capture_output=True,
            text=True,
            timeout=15,
        )
    except subprocess.TimeoutExpired as error:
        raise WindowsUsbError("PowerShell USB inventory timed out after 15 seconds") from error
    except OSError as error:
        raise WindowsUsbError(f"cannot run PowerShell USB inventory: {error}") from error
    if result.returncode:
        detail = result.stderr.strip() or f"exit {result.returncode}"
        raise WindowsUsbError(f"PowerShell USB inventory failed: {detail}")
    return parse_windows_usb_devices(result.stdout, supported_ids)
