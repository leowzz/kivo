# RP2040 Firmware Physical Evidence

Status: BLOCKED

The approved design is [RP2040 Parallel Device Support Design](../superpowers/specs/2026-07-31-rp2040-parallel-device-support-design.md). Its normative text was not modified.

| Device | Command timestamp | Observed VID:PID / serial | HELLO | GPIO boundary | CDC / learning | HID | Outcome |
| --- | --- | --- | --- | --- | --- | --- | --- |
| YD-RP2040 (authorized target) | 2026-07-31 15:56:56 CST | No matching inventory row; `picotool info --ser E0C9125B0D9B -a` reported no accessible BOOTSEL device | Not Run | Not Run | Not Run | Not Run | BLOCKED: serial `E0C9125B0D9B` absent |
| Luatos ESP32-S3 AIO (inventory only) | 2026-07-31 15:56:56 CST | `303a:4002`, `68B6B33D9F58`, `/dev/cu.usbmodem68B6B33D9F582` | Not Run | Not Run | Not Run | Not Run | Not tested after RP2040 identity gate failed |

No firmware was built or flashed. No helper process was started or stopped. The exact RP2040 serial was never substituted with another candidate or selected by enumeration order.
