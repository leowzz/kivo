# Device Management UI Verification

Verified on 2026-08-01 from frozen base `741f267` using the checked-in `?preview` fixture.

## Automated Verification

- `rtk cargo test --manifest-path src-tauri/Cargo.toml full_backup_preview_counts_devices_assignments_metric_rows_and_activity`: 1 passed.
- `rtk npm test -- src/DeviceManagement.test.tsx src/App.test.tsx`: 72 passed.
- `rtk cargo test --manifest-path src-tauri/Cargo.toml`: 107 passed.
- `rtk npm test`: 114 passed across 9 files.
- `rtk npm run build`: TypeScript and Vite production build passed.
- `rtk git diff --check`: passed.

The focused tests cover Editor Device Profile Home metrics, aggregate connection state, selected-Device metrics, raw event-time Device name attribution, separate profile/full-backup preview contracts, and all v2 backup counts.

## Visual Verification

Playwright 1.62.1 used the installed system Google Chrome executable. The preview server ran only for this check and was stopped afterward.

| Viewport | Page width | Layout | Stable rows | Selected ID |
| --- | --- | --- | --- | --- |
| 1120x760 | `1120 <= 1120` | list `x=184..780`; detail `x=780..1120` | `46px` min/max | `191 <= 191`, `overflow-wrap: anywhere` |
| 760x560 | `760 <= 760` | list `y=52..472`; detail starts at `y=472` | `46px` min/max | `444 <= 444`, `overflow-wrap: anywhere` |

Both runs asserted a nonblank root, `document.scrollWidth <= document.clientWidth`, `body.scrollWidth <= body.clientWidth`, separate selection for the two ESP32-S3 Devices, correct selected detail identity, copyable long IDs, and list/detail stacking below 900px. The preview showed four known Devices and one diagnostic candidate; selecting one same-board Device did not select or relabel the other.

Screenshots:

- `docs/verification/screenshots/device-management-1120x760.png`
- `docs/verification/screenshots/device-management-760x560.png`

Pixel inspection found no overlapping text or controls, clipped filter/assignment controls, decorative/nested cards, or page-level horizontal overflow. At 760px the detail follows the list and remains reachable through the Device Management workspace scroll.

## Important Fix Follow-up (2026-08-01)

System Google Chrome was launched directly with Playwright 1.62.1 against `http://127.0.0.1:1420/?preview`. The checked-in preview data supplied the registry, while the browser command boundary returned a selected-Device snapshot containing two stored event-time assignments. The existing initial viewport screenshots above were retained.

| Viewport | Page width | Detail geometry | Selected ID | Activity |
| --- | --- | --- | --- | --- |
| 1120x760 | document/body `1120/1120` | `340x708`, `scrollHeight=805` | `191/191`, `overflow-wrap:anywhere`, `user-select:auto` | wrapper contains a `560px` scan table |
| 760x560 | document/body `760/760` | stacked at `y=472`, internal scroller `508/1174` | `444/444`, `overflow-wrap:anywhere`, `user-select:auto` | metrics at `y=1007`, activity at `y=1092` |

Both viewports kept all five Device/candidate rows at exactly `46px`. At 760x560 the list ended at `y=472` and detail began at `y=472`; the assignment section measured `556x122`. The opened confirmation dialog measured `420x192` at `(170,184)` with right/bottom `(590,376)`, fully inside the viewport.

Additional inspected screenshots:

- `docs/verification/screenshots/device-management-1120x760-detail.png`
- `docs/verification/screenshots/device-management-760x560-detail-id-assignment.png`
- `docs/verification/screenshots/device-management-760x560-detail-metrics-activity.png`
- `docs/verification/screenshots/device-management-760x560-assignment-dialog.png`

The mobile detail captures show the long copyable Device ID, current assignment controls, compact metrics, and activity rows with their stored Device name, Device Profile ID, and Hardware Profile ID. Pixel inspection at original resolution found no overlap, clipped controls, or page-level horizontal overflow. The activity table's small horizontal overflow remains contained by its dedicated scroll wrapper. The dialog capture shows both actions and its content without clipping.

Fresh automated verification after the fixes:

- Focused metrics tests: 6 passed; focused command-boundary tests: 2 passed.
- `rtk npm test -- src/App.test.tsx src/DeviceManagement.test.tsx`: 72 passed.
- `rtk cargo test --manifest-path src-tauri/Cargo.toml`: 107 passed across 3 suites.
- `rtk npm test`: 114 passed across 9 files.
- `rtk npm run build`: TypeScript and Vite passed; 1,800 modules transformed.
- `rtk git diff --check`: passed.

## Mobile Device ID Wrap Follow-up (2026-08-01)

The selected Device ID now has a dedicated `28ch` maximum width while retaining `overflow-wrap:anywhere` and selectable text. At 760x560, Playwright measured the ID output as `201.59375x34px` with two text-line rectangles. Document and body widths both remained `760/760`.

The refreshed `docs/verification/screenshots/device-management-760x560-detail-id-assignment.png` visibly shows the canonical ID wrapping across two lines above the assignment controls, with no overlap or page-level horizontal overflow.

Final checks after this follow-up: `rtk npm test` passed 115 tests across 9 files, `rtk cargo test --manifest-path src-tauri/Cargo.toml` passed 107 tests across 3 suites, and `rtk npm run build` passed with 1,800 modules transformed.

## Integration Acceptance Boundary (2026-08-01)

The checked-in `?preview` fixture renders representative multi-Device states; it does not execute state-changing workflows. The workflow results below come from command-boundary/component tests that invoke the mutations. Preview screenshots are listed only where they show the corresponding visible state.

Exact focused commands and results:

```text
A1  rtk cargo test --manifest-path src-tauri/Cargo.toml --lib starts_four_independent_workers_and_enrolls_each_valid_runtime_once
    Pass: 1
A2  rtk npm test -- src/DeviceManagement.test.tsx src/App.test.tsx -t "stages one exact assignment|does not fan an assignment|preselects the one exact-board|saves one runtime assignment"
    Pass: 4
A3  rtk npm test -- src/DeviceManagement.test.tsx -t "shows no compatible hardware state|retains invalid stored IDs until repair"
    Pass: 2
A4  rtk cargo test --manifest-path src-tauri/Cargo.toml --lib enrollment_is_idempotent_and_persists_a_default_name
    Pass: 1
B1  rtk cargo test --manifest-path src-tauri/Cargo.toml --lib live_update_
    Pass: 7
B2  rtk cargo test --manifest-path src-tauri/Cargo.toml --lib broken_assignments_are_retained_without_compatible_fallback
    Pass: 1
C1  rtk cargo test --manifest-path src-tauri/Cargo.toml --lib learning_targets_one_exact_device_keeps_draft_unpersisted_and_cancels_on_disconnect
    Pass: 1
C2  rtk npm test -- src/App.test.tsx -t "isolates learning lifecycle"
    Pass: 1
D1  rtk cargo test --manifest-path src-tauri/Cargo.toml --lib clear_rename_and_forget_are_durable_transactions
    Pass: 1
D2  rtk cargo test --manifest-path src-tauri/Cargo.toml --lib attribution_is_immutable_across_reassignment_and_forgetting
    Pass: 1
D3  rtk npm test -- src/App.test.tsx -t "forgets only the confirmed offline Device"
    Pass: 1
E1  rtk cargo test --manifest-path src-tauri/Cargo.toml --lib full_backup_restore_switches_devices_assignments_and_metrics_together
    Pass: 1
E2  rtk cargo test --manifest-path src-tauri/Cargo.toml --lib full_backup_preview_counts_devices_assignments_metric_rows_and_activity
    Pass: 1
E3  rtk cargo test --manifest-path src-tauri/Cargo.toml --lib preview_export_and_button_lookup_use_complete_profiles
    Pass: 1
E4  rtk npm test -- src/App.test.tsx -t "previews a device profile before importing it|previews a full backup before restoring it"
    Pass: 2
```

| Workflow | Fixture Device IDs | Automated result and exact evidence | Physical result | Screenshot |
| --- | --- | --- | --- | --- |
| Unknown runtime enrollment; zero/one/many Hardware Profile choices; one-Device assignment isolation | Rust identities `ESP-A`, `ESP-B`, `RP-A`, `RP-B`; UI `rp-a`, `rp-b`, `esp-a` | Pass: A1-A4. A1 enrolls four exact runtime identities as Unassigned; A2/A3 execute zero/one/many selection and prove one assignment does not fan out; A4 proves deterministic default-name persistence. | Not Run | `docs/verification/screenshots/device-management-760x560-assignment-dialog.png` shows the preview confirmation state only. |
| Invalid stored references; explicit repair; action-only and topology live updates | UI `rp-a`; preview `16:vccgnd-yd-rp2040E0C9125B0D9B` | Pass: A3, B1, B2. Stored IDs remain visible until explicit repair; action-only and topology paths are independently exercised. | Not Run | `docs/verification/screenshots/device-management-1120x760-detail.png` shows the preview invalid-assignment state only. |
| Targeted learning, unsaved draft retention, cancel/restore isolation | Rust exact learning target; UI `device-second` while `device-front-desk` remains independent | Pass: C1-C2. | Not Run | N/A: headless workflow evidence; no learning screenshot is claimed. |
| Offline forget/re-enroll boundary and immutable historical attribution | Canonical backend ID `18:luatos-esp32s3-aioABCDEF123456`; UI `device-front-desk` | Pass: D1-D3. D1 forgets, reloads the absent record, re-enrolls the same ID, and asserts deterministic name `LuatOS ESP32-S3 AIO · 123456`, no Runtime Assignment, and durable persistence. D2 proves old metrics/activity attribution remains immutable. | Not Run | `docs/verification/screenshots/device-management-760x560-detail-metrics-activity.png` shows stored event-time attribution only, not the forget transition. |
| Full backup preview/restore and Device Profile-only export boundary | Restore IDs `18:luatos-esp32s3-aioAAAAAAAAAAAA` and `16:vccgnd-yd-rp2040BBBBBBBBBBBB`; UI preview reports 4 Devices / 3 assignments | Pass: E1-E4. E1 mutates the target, then atomically restores 2 Device Profiles, both Devices, both exact Runtime Assignments, language, metrics, and activity. E2 verifies full-backup counts. E3 verifies profile-only export/preview through the `DeviceProfile` boundary. | Not Run | N/A: headless workflow evidence; no backup dialog screenshot is claimed. |

Live multi-Device workflow verification was Not Run: RP2040 serial `E0C9125B0D9B` was absent, and the only attached ESP32-S3 failed the runtime v3 HELLO check after a successful explicit flash/re-enumeration. Consequently no physical assignment, action/topology update, learning, disconnect isolation, forget/re-enroll, or two-device backup/restore result is claimed.

The deterministic four-Device Rust fixture separately passed for two ESP32-S3 and two RP2040 identities, including assignments, reconnect, bootloader transition, independent runtime errors, interleaved hotkeys, and global Paste FIFO. That result is automated backend evidence, not a substitute for physical UI verification.
