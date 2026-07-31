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
