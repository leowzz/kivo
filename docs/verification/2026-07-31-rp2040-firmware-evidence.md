# RP2040 Firmware Physical Evidence

Status: RP2040 NOT RUN; ESP32-S3 RUNTIME PROTOCOL PASS; ESP32-S3 HID NOT RUN

The approved design is [RP2040 Parallel Device Support Design](../superpowers/specs/2026-07-31-rp2040-parallel-device-support-design.md). Its normative text was not modified.

| Device | Command timestamp | Observed VID:PID / serial | HELLO | GPIO boundary | CDC / learning | HID | Outcome |
| --- | --- | --- | --- | --- | --- | --- | --- |
| YD-RP2040 (authorized target) | 2026-08-01 03:29 CST | No matching row in `scripts/list_firmware_targets.py` | Not Run | Not Run | Not Run | Not Run | NOT RUN: serial `E0C9125B0D9B` absent; no substitute selected |
| Luatos ESP32-S3 AIO | 2026-08-01 follow-up | Runtime `303a:4002`, `68B6B33D9F58`, `/dev/cu.usbmodem68B6B33D9F582`; download `303a:1001`, `68:B6:B3:3D:9F:58`, `/dev/cu.usbmodem112401`; USB location `1-1.2.4` | `HELLO 3 esp32s3 luatos-esp32s3-aio 0.1.0+acceptance 17 0 1 2 3 4 5 6 7 8 9 12 13 14 15 16 17 18` | PASS: GPIO0/18 accepted; GPIO10/23 returned `CONFIG_ERROR ... invalid_direct` | PASS: runtime CDC re-enumeration and `LEARN_BEGIN`/`LEARN_END` revision 4 returned `LEARN_OK 4` | Not Run | PASS for upload, descriptor, CDC protocol, configuration boundary, and learning; physical HID action was not exercised |

## 2026-08-01 Acceptance Attempt

- `rtk proxy make helper-kill` completed before inventory/upload.
- Inventory contained exactly one recognized target: `runtime 303a:4002 luatos-esp32s3-aio 68B6B33D9F58 /dev/cu.usbmodem68B6B33D9F582`. It contained no RP2040 row, so RP2040 upload, GPIO0/22 boundary checks, GPIO23/29 rejection, CDC learning, HID actions, and runtime/BOOTSEL identity reconciliation were Not Run.
- The first explicit ESP32-S3 upload exposed that its bootloader formats the same MAC-derived serial as `68:B6:B3:3D:9F:58`. Target selection still used the captured USB location and never fell back to enumeration order. A focused regression now treats only equivalent 12-hex-digit colon/hyphen forms as the same serial.
- OpenOCD programmed and verified all ESP32-S3 image segments. The board remained in `303a:1001` until the explicit esptool `run --after hard_reset` step returned it to the Kivo runtime descriptor `303a:4002` with serial `68B6B33D9F58`.
- The initial complete command was `rtk proxy make upload-esp32s3 SERIAL=68B6B33D9F58 BUILD_ID=0.1.0+acceptance`. It selected `/dev/cu.usbmodem112401` at the same USB location, programmed and verified the image, reset into runtime, then exposed the empty-HELLO regression. The same command passed after the startup-order fix.
- Follow-up diagnostics reproduced the empty response on both `/dev/cu.usbmodem68B6B33D9F582` and `/dev/tty.usbmodem68B6B33D9F582`, with all four DTR/RTS combinations. `lsof` found no holder. `system_profiler` confirmed product `Kivo Keyboard`, VID/PID `303a:4002`, serial `68B6B33D9F58`; `strings` confirmed the flashed binary contains `0.1.0+acceptance`, `luatos-esp32s3-aio`, and `HELLO 3`.
- An A/B flash comparison proved the regression began when protocol-v3 HELLO formatting moved into a global C++ initializer. Commit `e43bee7` defers that allocation until Arduino `setup()`, after runtime startup. It also keeps protocol parsing board-neutral so GPIO23 reaches the ESP32-S3 board validator and produces an explicit error instead of a timeout.
- The repaired image was flashed with the same exact command. OpenOCD verification, explicit hard reset, and `verify_runtime_firmware.py` all passed. The standard smoke command then passed with valid GPIO0/18, rejected GPIO10/23, and learning revision 4. Duplicate startup/request HELLO lines and asynchronous state lines are tolerated without accepting an incorrect command acknowledgement.

No RP2040 was flashed. The exact RP2040 serial was never substituted with another candidate. ESP32-S3 HID actions were not exercised because that requires a physical input action. Automated transport tests are not counted as physical GPIO, CDC, HID, or mixed-device evidence.
