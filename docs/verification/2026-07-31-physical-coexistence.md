# Physical Parallel Device Acceptance

Run window: 2026-08-01 03:29-03:43 CST

| Acceptance item | Result | Evidence |
| --- | --- | --- |
| Automated 2x ESP32-S3 + 2x RP2040 isolation | Pass | `src-tauri/tests/parallel_devices.rs` passed; frozen review `67507be..96b1aa7` returned Spec PASS / Quality PASS. |
| Physical RP2040 descriptor / CDC / HID | Not Run | Authorized serial `E0C9125B0D9B` was absent from inventory. |
| Physical ESP32-S3 regression | Fail | Exact serial `68B6B33D9F58` programmed and re-enumerated as `303a:4002`, but the v3 HELLO handshake returned no data. |
| Physical mixed-device coexistence | Not Run | No RP2040 was attached; a single ESP32-S3 cannot prove coexistence or isolation. |
| Live Device Management two-device workflow | Not Run | Required physical pair was unavailable and the ESP32-S3 runtime protocol was not Ready. |

## Device Inventory

Initial read-only inventory after `rtk proxy make helper-kill`:

```text
runtime  303a:4002  luatos-esp32s3-aio  68B6B33D9F58  /dev/cu.usbmodem68B6B33D9F582
```

No `2e8a:0003` or `2e8a:102e` row was present. During the explicit ESP32-S3 upload, the same USB location `1-1.2.4` re-enumerated as:

```text
bootloader  303a:1001  luatos-esp32s3-aio  68:B6:B3:3D:9F:58  /dev/cu.usbmodem112401
```

The bootloader serial is the colon-delimited form of the runtime serial. Selection remained bound to the original serial plus transient USB location; no first/only-device fallback was used.

## Physical Isolation Boundary

Unplug/reconnect isolation, RP2040 ROM-mode transitions, alternating presses, Device-attributed activity, Home aggregation, and selected-Device metrics were all Not Run. Preview screenshots and deterministic fake-device tests demonstrate UI/runtime behavior only and are not physical evidence.
