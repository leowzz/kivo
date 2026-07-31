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
