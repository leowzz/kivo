# Frontend Task 4 Report

## RED

`rtk npm test -- src/DeviceManagement.test.tsx` failed with 4 new failures because the Runtime Assignment controls did not exist: no Device Profile combobox, no Hardware Profile combobox, no Save control, and no Clear control.

`rtk npm test -- src/App.test.tsx` failed with 2 new failures because the UI could not reach `save_runtime_assignment`: no assignment combobox or Save control existed.

The App-boundary test later failed as expected when it required the draft to reflect the returned snapshot; the initial implementation cleared the draft when the callback resolved rather than when the snapshot confirmed the assignment.

## GREEN

`rtk npm test -- src/DeviceManagement.test.tsx` passed: 15 tests.

`rtk npm test -- src/App.test.tsx` passed: 37 tests.

`rtk npm test` passed: 7 files, 78 tests.

`rtk git diff --check` passed with no whitespace errors.

## Build Diagnostic

`rtk npm run build` remains blocked only by the known later-owned type errors in `src/HomeDashboard.tsx` and `src/HardwareMapping.tsx`. The new `src/App.tsx` callback type error found during this task was corrected; no Task 4 file is reported by the final build diagnostic.

## Self-Review

- Assignment drafts are local to the selected `deviceId`; no board, controller, serial, or port fan-out exists.
- Compatibility uses `compatibleHardwareProfiles` with exact Board Profile IDs.
- Save and clear await their callbacks, disable mutation controls while pending, preserve the current row on rejection, and use returned App snapshots in `App.tsx`.
- Invalid stored IDs remain visible and no fallback is selected; users can repair or explicitly clear them.
- Save and clear confirmations name the Device, Device Profile, and Hardware Profile.

## Commit

`feat: assign profiles to one device at a time`

## Concerns

The full TypeScript build remains non-green because of the explicitly excluded `HomeDashboard.tsx` and `HardwareMapping.tsx` diagnostics. No compatibility aliases were added.

## Review Fix RED/GREEN

RED: `rtk npm test -- src/DeviceManagement.test.tsx` failed 5 new cases for partial-invalid raw IDs, valid clear confirmation names, and duplicate save/clear activation. `rtk npm test -- src/ConfirmDialog.test.tsx` failed because pending dialog actions were enabled.

GREEN: `rtk npm test -- src/DeviceManagement.test.tsx src/App.test.tsx src/ConfirmDialog.test.tsx src/i18n.test.ts` passed: 4 files, 61 tests. `rtk npm test` passed: 8 files, 84 tests. `rtk git diff --check` passed.

Build diagnostic: `rtk npm run build` remains blocked only by the known out-of-scope `src/HomeDashboard.tsx` and `src/HardwareMapping.tsx` TypeScript errors; no owned file is reported.
