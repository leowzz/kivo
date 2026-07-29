import time

import serial
from serial.tools.list_ports import comports


KIVO_USB_ID = (0x303A, 0x4002)
DOWNLOAD_USB_ID = (0x303A, 0x1001)


def main() -> None:
    ports = list(comports())
    kivo_ports = [
        port.device for port in ports if (port.vid, port.pid) == KIVO_USB_ID
    ]
    if len(kivo_ports) > 1:
        raise SystemExit(f"multiple Kivo devices found: {', '.join(kivo_ports)}")
    if not kivo_ports:
        if any((port.vid, port.pid) == DOWNLOAD_USB_ID for port in ports):
            print("ESP32-S3 is already in download mode")
            return
        raise SystemExit("Kivo device not found")

    port = kivo_ports[0]
    print(f"Entering download mode through {port} at 1200 baud")
    try:
        with serial.Serial(port, 1200, timeout=1) as device:
            device.dtr = True
            device.rts = True
            time.sleep(0.5)
    except serial.SerialException as error:
        message = f"cannot open {port}; stop make helper first: {error}"
        raise SystemExit(message) from error
    time.sleep(2)


if __name__ == "__main__":
    main()
