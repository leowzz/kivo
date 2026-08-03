# RP2040 Parallel Device Integration And Acceptance Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate the approved firmware, Rust runtime, and Device Management work; prove automated four-Device isolation; verify available ESP32-S3/RP2040 hardware together; and package a regression-checked Kivo application.

**Architecture:** This plan is the final gate after the firmware, backend, and frontend plans. Automated contract tests use fake enumeration/serial transports for deterministic 2x ESP32-S3 plus 2x RP2040 concurrency. Physical checks use explicit serial targets and the real composite USB devices. One acceptance ledger separates automated evidence from hardware actually present during the run.

**Tech Stack:** PlatformIO, Unity, Python/pytest, Rust/Cargo, React/Vitest, Tauri, macOS USB/serial tools, physical ESP32-S3 and YD-RP2040 devices.

## Prerequisites

- Complete `2026-07-31-rp2040-firmware-and-upload.md` Tasks 1-5.
- Complete `2026-07-31-rp2040-multi-device-backend.md` Tasks 1-8.
- Complete `2026-07-31-rp2040-device-management-frontend.md` Tasks 1-7.
- Keep the helper stopped during flashing. All upload commands require explicit `SERIAL`.
- Never treat automated fake-device coverage as evidence of physical HID, descriptor, cable, or re-enumeration behavior.
- Every test, build, and Git command is prefixed with `rtk`.

---

### Task 1: Run The Complete Static And Automated Gate

**Files:**
- Modify: `Makefile`
- Modify: `test/test_release.sh`
- Create: `docs/verification/2026-07-31-rp2040-automated-gate.md`

**Interfaces:**
- Produces: one `make test` gate for native firmware, upload targeting, Rust, frontend, and production build checks.

- [ ] **Step 1: Add a failing release-script expectation for every target**

Extend the release shell test to assert Make contains `build-esp32s3`, `build-rp2040`, `upload-esp32s3`, `upload-rp2040`, and a required serial guard. Assert both upload commands run post-upload v3 verification and no bare `upload` target auto-selects hardware.

- [ ] **Step 2: Run the release test**

Run: `rtk test bash test/test_release.sh`

Expected: FAIL until the consolidated Make/test targets are present.

- [ ] **Step 3: Consolidate non-destructive tests**

Make `test` execute, in this order:

```make
test:
	bash test/test_release.sh
	uv run pytest test/test_upload_targeting.py
	uv run pio test -e native
	cargo test --manifest-path src-tauri/Cargo.toml
	cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
	npm test
	npm run build
```

Do not put physical uploads in `make test`.

- [ ] **Step 4: Run all automated checks**

Run: `rtk test make test`

Expected: PASS.

Run: `rtk proxy env KIVO_FIRMWARE_BUILD_ID=0.1.0+acceptance uv run pio run -e esp32s3`

Expected: PASS and `.pio/build/esp32s3/firmware.bin` exists.

Run: `rtk proxy env KIVO_FIRMWARE_BUILD_ID=0.1.0+acceptance uv run pio run -e rp2040`

Expected: PASS and `.pio/build/rp2040/firmware.uf2` exists.

- [ ] **Step 5: Record and commit the automated gate**

Record command, commit SHA, environment, duration, and pass/fail for each row. Include explicit rows for v2 rejection, GPIO23 rejection, upload serial guards, registry branch search, and frontend build.

```bash
rtk git add Makefile test/test_release.sh docs/verification/2026-07-31-rp2040-automated-gate.md
rtk git commit -m "test: consolidate RP2040 release checks"
```

---

### Task 2: Prove Four Concurrent Devices With Deterministic Transports

**Files:**
- Create: `src-tauri/tests/parallel_devices.rs`
- Modify: `src-tauri/src/coordinator.rs`
- Modify: `src-tauri/src/device.rs`
- Modify: `src-tauri/src/paste.rs`

**Interfaces:**
- Consumes: injected USB enumerator, serial transport factory, clipboard sink, clock, two ESP32-S3 observations, and two RP2040 observations.
- Produces: black-box runtime evidence independent of physical device count.

- [ ] **Step 1: Build the four-Device fixture**

Use these deterministic identities:

```text
luatos-esp32s3-aio / ESP-A / /dev/fake-esp-a
luatos-esp32s3-aio / ESP-B / /dev/fake-esp-b
vccgnd-yd-rp2040 / RP-A / /dev/fake-rp-a
vccgnd-yd-rp2040 / RP-B / /dev/fake-rp-b
```

Each fake transport emits its exact v3 HELLO and records outbound topology/action lines. Assign four distinct Device Profile/Hardware Profile pairs, including two different Hardware Profiles for the same RP2040 board.

- [ ] **Step 2: Add the concurrency and isolation assertions**

Assert all four independently reach Ready after matching CONFIG_OK; alternating and interleaved inputs route to the correct actions; four hotkeys can advance independently; one disconnect leaves three Ready; one invalid CONFIG reply leaves three Ready; port renumbering preserves Device ID/assignment; and one known RP2040 bootloader observation changes only RP-A to Bootloader.

- [ ] **Step 3: Add global paste ordering assertions**

Emit Paste requests in host receive order ESP-B, RP-A, ESP-A, RP-B. Assert clipboard writes and outbound PASTE grants use exactly that order. Delay RP-A's DONE and prove later requests wait while an unrelated RP-B hotkey completes. Force one timeout and prove the remaining requests execute without coalescing or loss.

- [ ] **Step 4: Run the integration test repeatedly**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml --test parallel_devices -- --test-threads=1`

Expected: PASS.

Run: `rtk proxy sh -c 'for run in 1 2 3 4 5; do rtk cargo test --manifest-path src-tauri/Cargo.toml --test parallel_devices -- --test-threads=1 || exit 1; done'`

Expected: five consecutive PASS results with stable FIFO assertions.

- [ ] **Step 5: Commit the black-box integration fixture**

```bash
rtk git add src-tauri/tests/parallel_devices.rs src-tauri/src/coordinator.rs src-tauri/src/device.rs src-tauri/src/paste.rs
rtk git commit -m "test: prove four-device runtime isolation"
```

---

### Task 3: Flash And Verify The Observed YD-RP2040

**Files:**
- Modify: `docs/verification/2026-07-31-rp2040-firmware-evidence.md`

**Interfaces:**
- Consumes: observed ROM serial `E0C9125B0D9B`, explicit RP2040 uploader, runtime verifier, and physical GPIO/HID checks.
- Produces: proof that ROM and runtime identities reconcile and that GPIO0-22/CDC/HID behave as designed.

- [ ] **Step 1: Stop the helper and inventory USB targets**

Run: `rtk proxy make helper-kill`

Expected: any Kivo helper releases serial ports.

Run: `rtk proxy uv run python scripts/list_firmware_targets.py`

Expected: one bootloader row `bootloader 2e8a:0003 vccgnd-yd-rp2040 E0C9125B0D9B -`.

- [ ] **Step 2: Upload only the observed RP2040**

Run: `rtk proxy make upload-rp2040 SERIAL=E0C9125B0D9B BUILD_ID=0.1.0+acceptance`

Expected: picotool names serial `E0C9125B0D9B`; the verifier observes runtime `2e8a:102e`, the same serial, and `HELLO 3 rp2040 vccgnd-yd-rp2040 0.1.0+acceptance 23 0 ... 22`.

- [ ] **Step 3: Exercise board boundaries and actions**

Run `rtk proxy uv run python scripts/smoke_runtime_protocol.py --serial E0C9125B0D9B --vid 0x2e8a --pid 0x102e --family rp2040 --board vccgnd-yd-rp2040 --valid-pins 0,22 --rejected-pins 23,29 --exercise-actions`. Physically actuate the configured input when prompted. Require CONFIG_OK for GPIO0/22, CONFIG_ERROR for GPIO23/29, a learning event, visible Paste/Hotkey behavior, and matching DONE steps.

- [ ] **Step 4: Verify re-enumeration identity**

Return the board to ROM boot mode and inventory again. Confirm Device Management retains the same known Device row, name, and Runtime Assignment while mode changes from Runtime to Bootloader and CDC port disappears. Return to runtime and confirm automatic Ready after CONFIG_OK.

- [ ] **Step 5: Record evidence**

Append exact command timestamps, VID/PID, serial, full HELLO line, CONFIG responses, learning result, HID observations, and Device Management mode transitions to the firmware evidence document. Mark any unavailable physical action explicitly Not Run; do not infer it from automated tests.

---

### Task 4: Verify An Explicit ESP32-S3 Alongside RP2040

**Files:**
- Modify: `docs/verification/2026-07-31-rp2040-firmware-evidence.md`
- Create: `docs/verification/2026-07-31-physical-coexistence.md`

**Interfaces:**
- Consumes: one operator-selected `303a:4002` serial and RP2040 serial `E0C9125B0D9B`.
- Produces: physical coexistence evidence without enumeration-order targeting.

- [ ] **Step 1: Inventory and select the ESP32-S3 serial explicitly**

Run: `rtk proxy uv run python scripts/list_firmware_targets.py`

Copy the serial from the desired `runtime 303a:4002 luatos-esp32s3-aio` row into the `SERIAL` argument in the next command. If no such row exists, record ESP32-S3 physical coexistence as Not Run and continue the automated/package gates; never substitute the RP2040 or auto-select another row.

- [ ] **Step 2: Flash and verify that exact ESP32-S3**

Run: `rtk proxy make upload-esp32s3 SERIAL=<selected-303a-4002-serial> BUILD_ID=0.1.0+acceptance`

Expected: only the selected serial enters `303a:1001`, upload succeeds on its resolved port, and runtime returns `303a:4002` with `HELLO 3 esp32s3 luatos-esp32s3-aio 0.1.0+acceptance ...`.

- [ ] **Step 3: Start Kivo with both physical Devices**

Run: `rtk npm run tauri dev`

Expected: two known rows with distinct Device IDs. Assign each an exact compatible Hardware Profile and require both Ready. Alternate physical presses and confirm correct Device-attributed activity, no cross-routing, Home profile aggregation, and selected-Device metrics.

- [ ] **Step 4: Exercise physical isolation**

Unplug RP2040 and confirm ESP32-S3 stays Ready. Reconnect RP2040 and confirm automatic assignment activation. Put RP2040 into ROM boot and confirm only its row changes mode. Return it to runtime. Then unplug ESP32-S3 and confirm RP2040 stays Ready.

- [ ] **Step 5: Record the physical matrix**

For each acceptance item, record Pass, Fail, or Not Run with exact Device IDs/serials and observed behavior. Do not mark the approved design's multi-device physical acceptance complete if only one physical Device was available.

---

### Task 5: Verify Device Management Workflows Against Live State

**Files:**
- Modify: `docs/verification/2026-07-31-device-management-ui.md`
- Modify: `docs/verification/2026-07-31-physical-coexistence.md`

**Interfaces:**
- Consumes: live backend state plus preview fixtures for states not physically available.
- Produces: workflow evidence for enrollment, assignment, invalid references, learning, offline forget/re-enroll, metrics, and backup.

- [ ] **Step 1: Verify enrollment and assignment isolation**

For a valid previously unknown runtime Device, confirm immediate default naming and Unassigned status. Assign one exact pair and confirm no other row changes. Test zero/one/multiple compatible Hardware Profile choices using checked-in fixtures or live profiles.

- [ ] **Step 2: Verify invalid assignment and live update behavior**

Delete or retarget an assigned Hardware Profile. Confirm the Device retains its two stored IDs, stops, and shows repairable invalid assignment without fallback. Repair explicitly. Then perform one action-only edit and one topology edit; observe no topology transition for action-only, and independent Configuring/Ready/error states for topology edits.

- [ ] **Step 3: Verify targeted learning**

Start learning for one exact Device/Hardware Profile. Confirm other physical/fake Devices continue on saved topology. Capture a draft, end learning without saving, and confirm runtime restoration. Save the profile and confirm only Devices assigned to that Hardware Profile reconfigure.

- [ ] **Step 4: Verify forget and historical attribution**

Disconnect one Device, forget it, and confirm its record/name/assignment disappear while Device-filtered historical metrics remain queryable by the same Device ID. Reconnect and confirm deterministic default name plus no assignment. Verify old activity retains the event-time name and profile.

- [ ] **Step 5: Verify full backup and restore**

Export a full backup containing multiple profiles, Devices, assignments, metrics, and bounded activity. Mutate configuration and metrics, restore, and confirm both switch together. Export one Device Profile and inspect that it contains Hardware Profiles but no Device IDs, assignments, metrics, or activity.

- [ ] **Step 6: Record workflow evidence**

Append exact fixture/live source, commands, Device IDs, result, and screenshot path for every workflow. Clearly label preview-only states.

---

### Task 6: Package, Inspect Scope, And Close Acceptance

**Files:**
- Modify: `docs/verification/2026-07-31-rp2040-automated-gate.md`
- Modify: `docs/verification/2026-07-31-physical-coexistence.md`

**Interfaces:**
- Produces: release build evidence, focused diff review, and an honest acceptance ledger.

- [ ] **Step 1: Re-run the complete gate from a clean process state**

Stop dev servers/helper, then run: `rtk test make test`

Expected: PASS.

Run: `rtk proxy env KIVO_FIRMWARE_BUILD_ID=0.1.0+acceptance uv run pio run -e esp32s3`

Expected: PASS.

Run: `rtk proxy env KIVO_FIRMWARE_BUILD_ID=0.1.0+acceptance uv run pio run -e rp2040`

Expected: PASS.

- [ ] **Step 2: Build the macOS application**

Run: `rtk npm run tauri build -- --bundles app`

Expected: PASS and produce the Kivo `.app` bundle under `src-tauri/target/release/bundle/macos/`.

- [ ] **Step 3: Inspect the complete scope**

Run: `rtk git status --short`

Expected: only intentional implementation/evidence files are modified.

Run: `rtk git diff --check`

Expected: no whitespace errors.

Run: `rtk git diff --stat HEAD~12..HEAD`

Expected: changes are confined to firmware adapters/shared core, upload scripts, Rust domain/runtime/persistence, React Device Management/editor, tests, and verification docs.

- [ ] **Step 4: Close the evidence ledger**

Summarize separately:

```text
Automated 2x ESP32-S3 + 2x RP2040: Pass/Fail
Physical RP2040 descriptor/CDC/HID: Pass/Fail/Not Run
Physical ESP32-S3 regression: Pass/Fail/Not Run
Physical mixed-device coexistence: Pass/Fail/Not Run
Packaged application: Pass/Fail
```

Do not collapse Not Run into Pass. Link exact logs/screenshots and list any residual hardware-only risk.

- [ ] **Step 5: Commit final verification artifacts**

```bash
rtk git add docs/verification/2026-07-31-rp2040-automated-gate.md docs/verification/2026-07-31-rp2040-firmware-evidence.md docs/verification/2026-07-31-device-management-ui.md docs/verification/2026-07-31-physical-coexistence.md
rtk git commit -m "test: record parallel device acceptance"
```
