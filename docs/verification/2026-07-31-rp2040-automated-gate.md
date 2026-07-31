# RP2040 Automated Release Gate

Date: 2026-08-01

Base commit: `3b3fa6b6180fd7f39d5acb4ad0eab82e96254bd6` (`fix: prove mobile device id wrapping`).

Environment: macOS 15.7.4 (`Darwin 24.6.0 arm64`), uv 0.11.32, Cargo 1.94.1, npm 10.9.8. The firmware acceptance builds used `KIVO_FIRMWARE_BUILD_ID=0.1.0+acceptance`.

`make test` is an automated, non-destructive gate. It runs the release Makefile contract check, upload-targeting Python tests, native firmware tests, Rust tests and Clippy, then frontend tests and build. It has no upload, download-mode, picotool, or serial-device command.

| Check | Command | Commit | Environment | Duration | Result |
| --- | --- | --- | --- | --- | --- |
| Release Makefile contract | `rtk test bash test/test_release.sh` | `3b3fa6b` plus this task's working changes | local shell | < 1 s | PASS |
| Full non-destructive gate | `rtk test make test` | `3b3fa6b` plus this task's working changes | local shell | about 21 s | PASS |
| Protocol v2 rejection | `rtk test uv run pytest test/test_runtime_smoke.py` | `3b3fa6b` plus this task's working changes | uv Python | 0.02 s | PASS, 20 tests including `test_smoke_rejects_wrong_hello` |
| GPIO23 rejection | `rtk test uv run pio test -e native` | `3b3fa6b` plus this task's working changes | PlatformIO native | 0.489 s | PASS, 29 cases including `test_rp2040_learning_accepts_gpio22_and_rejects_gpio23` |
| Upload serial guards | `rtk test uv run pytest test/test_upload_targeting.py` | `3b3fa6b` plus this task's working changes | uv Python | 0.02 s | PASS, 17 tests including missing and ambiguous serial rejection |
| Make serial guard | `rtk proxy make require-serial` | `3b3fa6b` plus this task's working changes | no `SERIAL` supplied | < 1 s | PASS, expected exit 2: `SERIAL is required` |
| Bare upload guard | `rtk proxy make upload` | `3b3fa6b` plus this task's working changes | no hardware target supplied | < 1 s | PASS, expected exit 2; no controller is selected |
| Registry branch search | `rtk proxy rg -n '"esp32s3"|"rp2040"|"esp32c3"|"luatos-esp32s3-aio"|"vccgnd-yd-rp2040"' src-tauri/src/coordinator.rs src-tauri/src/device.rs src-tauri/src/workspace.rs src-tauri/src/lib.rs` | `3b3fa6b` plus this task's working changes | source-only | < 1 s | PASS, expected exit 1 with zero matches |
| Frontend build | `rtk test npm run build` | `3b3fa6b` plus this task's working changes | npm 10.9.8 | 0.145 s | PASS |
| ESP32-S3 acceptance build | `rtk proxy env KIVO_FIRMWARE_BUILD_ID=0.1.0+acceptance uv run pio run -e esp32s3` | `3b3fa6b` plus this task's working changes | PlatformIO ESP32-S3 | 7.602 s | PASS, `.pio/build/esp32s3/firmware.bin` exists |
| RP2040 acceptance build | `rtk proxy env KIVO_FIRMWARE_BUILD_ID=0.1.0+acceptance uv run pio run -e rp2040` | `3b3fa6b` plus this task's working changes | PlatformIO RP2040 | 8.438 s | PASS, `.pio/build/rp2040/firmware.uf2` exists |

The acceptance builds compile firmware only. No physical upload command was run, and no attached device was selected.
