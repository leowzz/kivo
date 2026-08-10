# Action And Device Workspace Follow-up Implementation Plan

> **Execution:** Follow this plan task by task with systematic debugging, test-driven development, and verification before completion.

> **Status (2026-08-10):** Implementation, review fixes, startup-crash repair, full verification, and the macOS physical Action matrix are complete. Physical logs cover complex Paste, one-run Paste -> Delay 500ms -> Play/Pause, Next, Volume Up, and Volume Down; the user confirmed the observed key behavior is correct. Previous uses the same verified Consumer Control path but was not separately sampled.

**Goal:** Make edited Actions reliably reach the active device runtime, simplify shortcut selection, restore the shared button design system, and reshape Device Management into a narrow device list with a wide editing workspace.

**Architecture:** Keep Device Profiles as reusable/shareable configuration documents and Devices as runtime assignments. Shared-profile confirmation remains scoped to Device Management; ordinary profile editing on Button Behavior returns to debounced autosave. The existing protocol-v6 host-created run remains the single execution path for Paste, Delay, and Media, with regression coverage proving ordered command generation. UI changes stay within existing React components and CSS primitives.

**Tech Stack:** React 19, TypeScript, Vitest/Testing Library, Tauri 2, Rust, C++17/PlatformIO Unity, Playwright.

**Product reference:** `docs/superpowers/specs/2026-08-10-action-device-workspace-followups.md`

---

### Task 1: Repair Cross-page Autosave And Runtime Snapshot Updates

**Files:**

- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/i18n.ts` only if saved-state text is missing

- [x] Add a regression test proving that merely visiting a shared profile in Device Management does not suppress later Button Behavior autosave.
- [x] Add a regression test that edits shared I/O, leaves Device Management without saving/copying, waits past the debounce interval, and proves `save_device_profile` is not invoked.
- [x] Assert the visible save state reaches the localized `save.saved` state after the save completes.
- [x] Represent only a genuinely edited shared draft as pending; opening settings or switching workspace tabs must not create pending state.
- [x] Keep a pending shared draft gated across page navigation until explicit shared save or per-device copy clears it.
- [x] Keep the existing explicit “save shared profile” and “duplicate for this device” behavior unchanged inside Device Management.
- [x] Run `npm test -- src/App.test.tsx` and confirm the regression passes.

### Task 2: Lock Paste, Delay, And Media To One Ordered Protocol-v6 Run

**Files:**

- Modify: `src-tauri/src/protocol.rs`
- Modify: `test/test_gpio_trigger/test_main.cpp` only where parser/HID coverage is missing
- Modify: `test/test_runtime_smoke.py` only where end-to-end command ordering coverage is missing

- [x] Add a Rust protocol test for `Paste -> Delay 500ms -> Media Play/Pause`, asserting one host-created run ID, steps `1/3`, `2/3`, `3/3`, and exact ordered commands.
- [x] Add a production `DeviceSession` test that starts from `DeviceMessage::State`, creates host run `1` independently of input event ID `700`, traverses deferred paste grant, rejects a mismatched grant, and then emits exact Delay/Media steps on correct `DONE` messages.
- [x] Confirm native parser tests cover valid and invalid `DELAY` and `MEDIA` commands, and media usage transport produces a press then release.
- [x] Confirm runtime smoke coverage observes ordered completion without inventing a second run ID.
- [x] Change production protocol/firmware code only if the new tests expose a real defect; otherwise preserve the already-correct execution path.
- [x] Run focused Rust, native, and smoke suites.

### Task 3: Rebuild The Shortcut Picker Around Stable Names And Category Tabs

**Files:**

- Modify: `src/HotkeyPicker.tsx`
- Modify: `src/HotkeyPicker.test.tsx`
- Modify: `src/styles/app.css`
- Modify: `src/App.test.tsx` for changed accessible names

- [x] Add failing tests proving Chinese UI still exposes technical modifier names such as `Command`, `Control`, `Option / Alt`, `Left Command`, and `Right Alt`.
- [x] Add failing tests for an accessible category `tablist`, default `常用` selection, one visible `tabpanel`, and switching to `功能键` without losing selected keys.
- [x] Add a DOM-order assertion that shortcut recording appears before search/category browsing.
- [x] Replace localized modifier labels with stable English technical labels while retaining localized category and helper copy.
- [x] Centralize the technical token label formatter in `src/hotkey.ts` and reuse it in picker chips, checkboxes, recording results, and Action summaries.
- [x] Move the recording command immediately below selected-key chips.
- [x] Render one active category list behind horizontally scrollable tabs; search filters only the active category.
- [x] Preserve six ordinary-key validation, independent left/right modifiers, recording, and selected values across tab changes.
- [x] Run `npm test -- src/HotkeyPicker.test.tsx src/App.test.tsx`.

### Task 4: Restore Shared Button Primitives

**Files:**

- Modify: `src/styles/app.css`
- Modify: `src/ActionDialog.tsx`
- Modify: `src/ActionDialog.test.tsx`
- Modify: `src/DeviceManagement.tsx`
- Modify: `src/DeviceManagement.test.tsx`
- Modify: `src/ConfigurationSettingsDialog.tsx`
- Modify: `src/ConfigurationSettingsDialog.test.tsx`

- [x] Add `secondary-button` to the existing button primitive with hover, disabled, and focus-visible states.
- [x] Assign `primary-button` to save/apply commands, `danger-button` to destructive commands, and `secondary-button` to cancel/settings/shared-profile commands.
- [x] Remove private color-only classes that leave Action dialog buttons with browser-native geometry.
- [x] Add component assertions for semantic button class assignments.
- [x] Run the three focused component test suites.

### Task 5: Make Device Management A Narrow-list/Wide-workspace Page

**Files:**

- Modify: `src/styles/app.css`
- Modify: `src/DeviceManagement.tsx` only if table markup needs accessible truncation titles
- Modify: `src/DeviceManagement.test.tsx`

- [x] Change the desktop grid to a `320-360px` device column and a flexible editing workspace.
- [x] Remove the left table/candidate `560px` minimum width and use bounded fractional columns with ellipsis for long values.
- [x] Keep Device Details, I/O Mapping, and Key Layout in the right page workspace, not in dialogs.
- [x] Preserve the existing stacked layout below `900px` and prevent page-level horizontal overflow at `390px`.
- [x] Add structural regression assertions where CSS-independent behavior can be tested.

### Task 6: Verify Function, Build Quality, And Visual Layout

**Files:**

- Update checkboxes in this plan as evidence is collected.
- Modify production/test files only for defects found during verification.

- [x] Run `npm test` (17 files, 207 tests passed after the startup-failure and multi-profile autosave regressions).
- [x] Run `npm run build`.
- [x] Run `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`.
- [x] Run `cargo test --manifest-path src-tauri/Cargo.toml` (263 passed, 1 ignored, plus 2 integration tests passed).
- [x] Run `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`.
- [x] Run `uv --no-config run pytest test/test_runtime_smoke.py` (26 passed).
- [x] Run `uv --no-config run pio test -e native` (57 passed).
- [x] Build both firmware targets with `KIVO_FIRMWARE_BUILD_ID=0.1.0+followups`.
- [x] Inspect Device Management at `1120x760`, `1440x900`, and `390x844`: the device column is `360px` on desktop, the workspace expands from `560px` to `880px`, mobile stacks both regions at `390px`, and no page-level horizontal overflow is present.
- [x] Inspect the shortcut Action dialog on desktop and mobile: technical modifier names, recording above search, one category panel, shared button styles, and no horizontal overflow.
- [x] Run `git diff --check` and review the final diff.
- [x] Record direct Paste evidence: runtime log timestamp `2026-08-10 00:55:01 +0800`, GPIO 9 / `K9`, run ID `71`, Paste step `1/2` and Enter step `2/2` completed; the target application received `OK.`.
- [x] Confirm the connected HID descriptor exposes keyboard Report ID 1 and Consumer Control Report ID 2, while explicitly not treating descriptor compatibility as proof of a Media action.
- [x] Query both runtime logs and `metrics.sqlite3`: no physical Delay or Media action lifecycle exists, so both remain open.

### Task 7: Close The Remaining macOS Physical Action Matrix

**Evidence sources:**

- `/Users/leo/Library/Application Support/cn.wleo.kivo/log/kivo.log`
- `/Users/leo/Library/Application Support/cn.wleo.kivo/data/log/kivo.log`
- `/Users/leo/Library/Application Support/cn.wleo.kivo/data/metrics.sqlite3`

- [x] Prove a simple physical Paste reaches the target application and correlate it with one device, button, run ID, and ordered steps.
- [x] Identify the exact pending physical controls without mutating user data: online profile `key9` has Delay on K2 and Media `volume_down` on K7; their latest recorded presses predate the profile's `2026-08-09 19:40:38 +0800` write, so neither is a qualifying sample.
- [x] Preserve the original `3key` profile and create per-device profile `3key physical test 2026-08-10` for serial `50031519384E811C`, with complex Paste on `KEY1` and Paste -> Delay 500ms -> Play/Pause plus long-press Previous and double-press Next on `KEY2`.
- [x] Transfer `/dev/cu.usbmodem1101` from the old debug process to the bundled candidate app and confirm runtime `ready` with `deviceProfileId=3key-physical-test-2026-08-10` and `hardwareProfileId=hardware-copy`.
- [x] Prove physical Paste with Chinese, newline, and symbols: `KEY1` produced multiple completed 42-character Paste runs under the dedicated profile, and the user confirmed the target behavior is correct.
- [x] Prove a physical `Paste -> Delay 500ms -> Media Play/Pause` run: run `34` remained on steps `1/3`, `2/3`, and `3/3`; the user confirmed the configured delay and resulting behavior are correct. Runtime event timestamps retain the originating input timestamp, so they are not misused as a wall-clock duration measurement.
- [x] Prove observable macOS media behavior: physical runs completed Play/Pause (`34`), Next (`11`, `13`, `15`), Volume Up (`19`, `21`), and Volume Down (`24`, `27`, `30`, `33`), with the user confirming correct behavior. Previous was not separately sampled after the test bindings were changed to volume controls.
- [x] Confirm physical testing exposed no production defect requiring another verification run; the fresh full matrix had already passed immediately before the physical sample.

### Task 8: Prevent Workspace Compatibility Errors From Crashing macOS Startup

**Files:**

- Modify: `src-tauri/src/lib.rs`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/types.ts`
- Modify: `src/styles/app.css`
- Update: `docs/superpowers/issues/2026-08-10-action-device-workspace-issue-register.md`

- [x] Reproduce installed Kivo `0.5.1` from Terminal with `RUST_BACKTRACE=full` and capture the exact setup error: `unsupported_profile_schema`.
- [x] Confirm the crash is caused by returning that workspace error through Tauri's macOS `did_finish_launching` callback, where the panic cannot unwind.
- [x] Add a Rust regression proving setup failures are consumed and converted to a stable startup-failure state.
- [x] Add a frontend regression proving an incompatible workspace renders a blocking error view without calling `get_snapshot` or subscribing to runtime events.
- [x] Install runtime logging before workspace loading and persist `application_startup_failed` with the stable error code.
- [x] Preserve incompatible user data byte-for-byte; do not reset, migrate down, or create a fallback editable workspace.
- [x] Build an isolated release macOS app and verify valid schema reaches `application_ready`, including window and tray setup.
- [x] Verify incompatible schema cold start and repeated start remain alive without panic, render the compatibility page, and create no new `.ips` crash report.

## Verification Commands

```bash
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
uv --no-config run pytest test/test_runtime_smoke.py
uv --no-config run pio test -e native
KIVO_FIRMWARE_BUILD_ID="0.1.0+followups" uv --no-config run pio run -e esp32s3
KIVO_FIRMWARE_BUILD_ID="0.1.0+followups" uv --no-config run pio run -e rp2040
git diff --check
```
