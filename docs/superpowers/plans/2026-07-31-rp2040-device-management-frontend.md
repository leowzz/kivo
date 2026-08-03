# RP2040 Device Management Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a dense Device Management workspace that keeps multiple ESP32-S3 and RP2040 Devices distinct, edits one atomic Runtime Assignment at a time, supports board-specific Hardware Profiles and targeted learning, and remains unchanged when a new compiled Board Profile or Controller Family appears.

**Architecture:** Backend snapshots remain authoritative. `types.ts` mirrors structured Device dimensions and version-2 profile documents. Pure selectors in `deviceStatus.ts` derive list labels, counts, filtering, compatibility, and safe GPIO intersections. `DeviceManagement.tsx` owns row selection and staged detail edits. `HardwareMapping.tsx` edits one Hardware Profile within the Editor Profile. `App.tsx` coordinates snapshots and routes Device-attributed runtime events.

**Tech Stack:** React 19, TypeScript 7, Tauri invoke/events, Vitest, Testing Library, lucide-react, existing CSS design system.

## Global Constraints

- A row represents one Device ID, never one Controller Family, Board Profile, or serial port.
- Device status retains connection, mode, identity, assignment, and runtime dimensions. Primary labels and summary counts are pure derivations.
- Device Management contains no family- or board-specific conditional. It renders backend registry metadata.
- Assignment mutation is one explicit Device Profile plus Hardware Profile pair for one Device ID. There is no bulk action.
- Unknown/missing/duplicate identity candidates are diagnostic only.
- Hardware learning always names Device ID, Device Profile ID, Hardware Profile ID, editing revision, and pins.
- UI text remains Simplified Chinese. Existing app styling, 8px-or-less radii, compact typography, and lucide icons are preserved.
- Visible terminology is fixed: Device Profile = `设备配置`, Editor Profile = `当前编辑配置`, Hardware Profile = `硬件配置`, Runtime Assignment = `运行分配`, Board Profile = `板型`. Remove user-facing `Model`/`型号` wording.
- Every test, build, and Git command is prefixed with `rtk`.

---

### Task 1: Mirror Version-2 Profiles And Structured Device State

**Files:**
- Modify: `src/types.ts`
- Create: `src/deviceStatus.ts`
- Create: `src/deviceStatus.test.ts`
- Modify: `src/preview.ts`

**Interfaces:**
- Produces: exact frontend DTOs plus pure selectors `primaryDeviceLabel`, `deviceSummary`, `matchesDeviceFilter`, `compatibleHardwareProfiles`, and `editablePins`.
- Removes: singleton `ConnectionStatus`, `supportedGpios`, global `runtimeError`, global `learning`, `activeModel`, and `ModelConfig.hardware` assumptions.

- [ ] **Step 1: Write failing selector tests**

Cover zero/one/many summaries, attention priority, bootloader, invalid identity, invalid assignment, configuring, ready, offline, search by name/serial/board/port, multiple same-board rows, compatible Hardware Profiles, and GPIO intersection.

```ts
test("keeps five status dimensions and derives attention priority", () => {
  const device = fixtureDevice({
    connection: "online",
    mode: "runtime",
    identity: "valid",
    assignment: "invalid_assignment",
    runtime: "inactive",
  });
  expect(primaryDeviceLabel(device)).toBe("分配需要修复");
  expect(deviceSummary([device])).toEqual({ ready: 0, attention: 1, offline: 0, progress: 0 });
});

test("intersects board safety with one selected device capability", () => {
  expect(editablePins([0, 1, 2, 22], [0, 2, 11])).toEqual([0, 2]);
  expect(editablePins([0, 1, 2, 22], null)).toEqual([0, 1, 2, 22]);
});
```

- [ ] **Step 2: Run tests and verify the new DTOs are missing**

Run: `rtk npm test -- src/deviceStatus.test.ts`

Expected: FAIL importing the new selectors and types.

- [ ] **Step 3: Replace the TypeScript domain model**

Keep persisted Device Profile document fields in `snake_case`, matching YAML and the current command boundary. Keep live snapshot/status DTO fields in `camelCase`, matching Rust `#[serde(rename_all = "camelCase")]` response types:

```ts
export interface HardwareProfile {
  id: string;
  name: string;
  board_profile_id: string;
  debounce_ms: number;
  inputs: InputSource[];
}

export interface DeviceProfile {
  schema_version: 2;
  profile: ModelLayout;
  hardware_profiles: HardwareProfile[];
  actions: Record<string, ButtonAction[]>;
}

export interface RuntimeAssignment {
  device_profile_id: string;
  hardware_profile_id: string;
}

export interface BoardProfileSummary {
  id: string;
  controllerFamilyId: string;
  displayName: string;
  runtimeUsb: string;
  bootloaderUsb: string | null;
  safePins: number[];
}

export interface DeviceStatus {
  deviceId: string;
  name: string;
  hardwareSerial: string;
  controllerFamilyId: string;
  boardProfileId: string;
  connection: "online" | "offline";
  mode: "runtime" | "bootloader" | null;
  identity: "validating" | "valid" | "invalid_identity" | "duplicate_identity";
  assignment: "unassigned" | "valid" | "invalid_assignment";
  runtime: "inactive" | "configuring" | "learning" | "ready" | "runtime_error";
  port: string | null;
  firmwareBuildId: string | null;
  capabilities: number[];
  runtimeAssignment: RuntimeAssignment | null;
  latestError: RuntimeActivity | null;
  learning: LearningSession | null;
}
```

Add `CandidateStatus` for non-enrolled invalid identities and unknown bootloaders. Update `AppSnapshot` to `deviceProfiles`, `editorProfile`, `boardProfiles`, `devices`, `candidates`, `language`, and `homeMetrics`. Extend metrics/activity DTOs with Device attribution and backup preview counts.

- [ ] **Step 4: Implement pure, registry-driven selectors**

Attention priority is identity problem, invalid assignment, runtime error, bootloader, then connected unassigned. Ready requires runtime `ready`. Offline counts only known disconnected Devices not already attention. Validating/configuring/learning count as progress, not Ready. Filter tabs are `all`, `attention`, `ready`, `offline`.

`compatibleHardwareProfiles` compares exact `board_profile_id`. `editablePins` returns board safe pins offline and their intersection with selected Device capabilities online. Do not compare Controller Family for assignment compatibility.

- [ ] **Step 5: Add representative preview fixtures**

Make `preview.ts` expose two ESP32-S3 Devices, two RP2040 Devices, one unknown RP2040 bootloader candidate, mixed Ready/Offline/Needs Attention states, and two Device Profiles with multiple Hardware Profiles. Preview data must use the same DTOs as production and contain no alternate UI-only device shape.

- [ ] **Step 6: Run tests and commit**

Run: `rtk npm test -- src/deviceStatus.test.ts`

Expected: PASS.

```bash
rtk git add src/types.ts src/deviceStatus.ts src/deviceStatus.test.ts src/preview.ts
rtk git commit -m "refactor: model structured device state in React"
```

---

### Task 2: Replace Singleton Connection UI With Registry Summary And Navigation

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/App.css`
- Modify: `src/i18n.ts`
- Modify: `src/i18n.test.ts`
- Modify: `src/App.test.tsx`

**Interfaces:**
- Produces: `devices` workspace navigation and top-bar `Ready · Needs Attention · Offline` summary.
- Consumes: `deviceSummary(snapshot.devices)` plus candidate attention count.

- [ ] **Step 1: Write failing summary/navigation tests**

Assert zero Devices renders `0 就绪 · 0 需处理 · 0 离线`; mixed fixtures render exact counts; candidates add Needs Attention but never Ready/Offline; Device Management is a first-class navigation button with a `Usb` icon; changing Editor Profile does not call an assignment command. Assert navigation/dialog labels use the fixed glossary terms and contain no visible `型号`.

- [ ] **Step 2: Run the focused App tests**

Run: `rtk npm test -- src/App.test.tsx`

Expected: FAIL because App still renders one `connection` and has no Devices view.

- [ ] **Step 3: Refactor snapshot state around Device Profiles**

Rename React state and helpers consistently: `models -> deviceProfiles`, `activeModel -> editorProfile`, `activeConfig -> editorProfileConfig`. `applySnapshot` replaces the registry arrays atomically. Keep local editable Device Profile drafts and autosave semantics.

Extend `View` with `devices`. Put the Device Management command in the work navigation, not the profile-file action page. Selecting an Editor Profile invokes only `save_settings({ settings: { schema_version: 2, editor_profile: editorProfile, language } })` and never changes a Runtime Assignment. The backend patch command preserves authoritative Device records and assignments.

- [ ] **Step 4: Render the global summary**

Replace the single-port badge with three compact text counters and semantic status dots. Include candidates in attention. Keep fixed-height layout so count changes do not move navigation. Tooltips explain only icon buttons; visible text stays operational rather than instructional.

- [ ] **Step 5: Run tests and commit**

Run: `rtk npm test -- src/App.test.tsx src/i18n.test.ts`

Expected: PASS for summaries, navigation, and Editor Profile isolation.

```bash
rtk git add src/App.tsx src/App.css src/i18n.ts src/i18n.test.ts src/App.test.tsx
rtk git commit -m "feat: summarize the complete device registry"
```

---

### Task 3: Build The Dense Device List And Diagnostic Detail Panel

**Files:**
- Create: `src/DeviceManagement.tsx`
- Create: `src/DeviceManagement.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.css`
- Modify: `src/i18n.ts`

**Interfaces:**
- Consumes: known Devices, invalid candidates, Device Profiles, Board Profiles, filtered metrics, and mutation callbacks.
- Produces: stable row selection, four filters, search, diagnostic detail, rename, clear, forget, and staged assignment.

- [ ] **Step 1: Write failing list/filter/selection tests**

Render two RP2040 and two ESP32-S3 rows. Assert same-board Devices remain separate, All/Needs Attention/Ready/Offline filters produce exact row sets, search matches name/serial/board/port, selection survives live status replacement by Device ID, and selection moves to the nearest remaining row when a candidate disappears.

- [ ] **Step 2: Write failing detail mutation tests**

Assert invalid candidates expose only diagnostics; rename calls one Device ID; assignment is staged until explicit save; clear targets one Device; online forget is disabled; offline forget opens confirmation naming that Device; and no bulk checkbox/action exists.

- [ ] **Step 3: Run the component tests**

Run: `rtk npm test -- src/DeviceManagement.test.tsx`

Expected: FAIL because the component does not exist.

- [ ] **Step 4: Implement the dense list**

Use a two-column page layout: flexible list and a constrained 340px detail panel, collapsing into vertically ordered regions below 900px. The list header contains one search input and tab-style segmented filter. Columns are Device name, Board Profile display name, derived primary status, Runtime Assignment label, and port. Use one stable `<button>` row per Device ID with accessible selected state; do not use cards.

Append invalid candidates in a visually separated Needs Attention section. Candidate keys include board/serial/port observation identity and never collide with persisted Device IDs.

- [ ] **Step 5: Implement the detail panel**

Show name, raw serial, Device ID, Controller Family, Board Profile, mode, port, firmware build, reported pins, assignment, latest error, and filtered metrics/activity. Rename uses `Pencil`/`Check`/`X` icon buttons. Assignment controls and clear are implemented in Task 4. Offline Forget uses `Trash2`, a confirmation dialog, and `forget_device({ deviceId })`; online and candidate views never enable it.

- [ ] **Step 6: Integrate commands in App**

Pass callbacks that invoke `rename_device`, `clear_runtime_assignment`, `forget_device`, and `get_device_metrics`, then apply the returned authoritative snapshot. Device-specific metrics refresh on selected Device change and on matching runtime events.

- [ ] **Step 7: Run tests and commit**

Run: `rtk npm test -- src/DeviceManagement.test.tsx src/App.test.tsx`

Expected: PASS for multi-Device rows, filters/search, diagnostics, rename, forget gating, and stable selection.

```bash
rtk git add src/DeviceManagement.tsx src/DeviceManagement.test.tsx src/App.tsx src/App.css src/i18n.ts
rtk git commit -m "feat: add device management workspace"
```

---

### Task 4: Stage And Save One Exact Runtime Assignment

**Files:**
- Modify: `src/DeviceManagement.tsx`
- Modify: `src/DeviceManagement.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/i18n.ts`

**Interfaces:**
- Produces: atomic `save_runtime_assignment({ deviceId, assignment })` calls and explicit clear.
- Consumes: exact Board Profile compatibility from `compatibleHardwareProfiles`.

- [ ] **Step 1: Add failing zero/one/many compatibility tests**

For zero compatible Hardware Profiles, disable save and show `没有兼容的硬件配置`. For exactly one, preselect it but require Save. For multiple, select the Device Profile but leave Hardware Profile empty until the user explicitly chooses one. Changing the Device Profile resets an incompatible staged Hardware Profile.

- [ ] **Step 2: Add failing isolation and invalid-assignment tests**

Save one RP2040 assignment and assert the other RP2040 row remains unchanged until an authoritative backend snapshot says otherwise. For an invalid stored assignment, show both retained IDs and allow repair or explicit clear; never auto-select a fallback.

- [ ] **Step 3: Implement atomic staged state**

Keep `{ deviceProfileId, hardwareProfileId }` local to the selected Device ID. Reset the draft only when selection changes or an authoritative snapshot confirms save. Save invokes once with the complete pair. Disable while saving and show inline backend validation errors without changing the row's current assignment.

The confirmation text names the target Device and both selected profiles. There is no same-board or same-family fan-out option.

- [ ] **Step 4: Run tests and commit**

Run: `rtk npm test -- src/DeviceManagement.test.tsx`

Expected: PASS for zero/one/many choices, one-pair save, repair, clear, and Device isolation.

```bash
rtk git add src/DeviceManagement.tsx src/DeviceManagement.test.tsx src/App.tsx src/i18n.ts
rtk git commit -m "feat: assign profiles to one device at a time"
```

---

### Task 5: Edit Multiple Board-Specific Hardware Profiles

**Files:**
- Modify: `src/HardwareMapping.tsx`
- Create: `src/HardwareMapping.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/App.css`
- Modify: `src/i18n.ts`

**Interfaces:**
- Produces: add/name/duplicate/delete/select Hardware Profile operations inside the Editor Profile and compiled Board Profile selection.
- Consumes: Board Profile safe pins and optional selected Device capabilities.

- [ ] **Step 1: Write failing Hardware Profile management tests**

Assert add creates a stable ID and selected compiled Board Profile; duplicate copies topology with a new ID/name; delete requires confirmation and does not repair Device assignments; Board Profile change keeps invalid pins visible with validation messages; and multiple profiles for one board remain distinct.

- [ ] **Step 2: Write failing pin-source tests**

Offline editing shows the exact board safe set. Online editing accepts an explicitly selected compatible Device and shows the intersection with reported capabilities. A Device from another Board Profile cannot be selected. GPIO23-29 never appear for `vccgnd-yd-rp2040`.

- [ ] **Step 3: Run the component tests**

Run: `rtk npm test -- src/HardwareMapping.test.tsx`

Expected: FAIL because `HardwareMapping` still edits singleton `model.hardware`.

- [ ] **Step 4: Refactor the editor around one Hardware Profile**

Add a compact Hardware Profile selector plus `Plus`, `Copy`, `Pencil`, and `Trash2` icon commands. Board Profile is a labeled `<select>` populated entirely from `boardProfiles`. Existing direct/contact topology editors receive the selected `HardwareProfile` and `editablePins`. They update only that item in `DeviceProfile.hardware_profiles`.

Changing Board Profile does not delete invalid inputs. Mark every invalid pin, disable autosave while validation fails, and let the user repair or revert. Deleting a referenced Hardware Profile persists the profile change; backend Device status then reports the affected assignments invalid.

- [ ] **Step 5: Run tests and commit**

Run: `rtk npm test -- src/HardwareMapping.test.tsx src/App.test.tsx`

Expected: PASS for multiple profiles, exact Board Profile choice, offline/online pins, and visible invalid mappings.

```bash
rtk git add src/HardwareMapping.tsx src/HardwareMapping.test.tsx src/App.tsx src/App.test.tsx src/App.css src/i18n.ts
rtk git commit -m "feat: edit board-specific hardware profiles"
```

---

### Task 6: Target Learning And Runtime Feedback To The Correct Device

**Files:**
- Modify: `src/HardwareMapping.tsx`
- Modify: `src/HardwareMapping.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/types.ts`

**Interfaces:**
- Produces: exact learning command arguments, Device-local learning state, editor-only captured drafts, and Device Profile-aware press animation.

- [ ] **Step 1: Write failing learning isolation tests**

With two Devices assigned to one Hardware Profile, begin learning on one and assert the command includes the complete target tuple; only that row shows Learning; captured input updates only the selected editor draft; cancel/end does not call save; disconnect ends runtime learning while retaining the draft; and explicit profile save later shows both affected Devices independently configuring.

- [ ] **Step 2: Write failing press-attribution tests**

Emit a runtime event assigned to a non-Editor Device Profile and assert no keypad highlight. Emit the same Device Profile as the Editor Profile and assert only the matching button animates. Metrics and activity still update for both events.

- [ ] **Step 3: Implement exact learning selection**

The learning section chooses only online, identity-valid runtime Devices whose Board Profile exactly equals the selected Hardware Profile. With none, learning is disabled while ordinary offline editing remains enabled. Begin invokes:

```ts
invoke("begin_learning", {
  deviceId,
  deviceProfileId: editorProfile.profile.id,
  hardwareProfileId: hardware.id,
  editingRevision,
  pins,
});
```

Track captured signatures in the autosave draft but suppress autosave while the learning session is active. End restores runtime only. The user's subsequent explicit mapping edit/save uses the normal profile save path.

- [ ] **Step 4: Route Device-attributed events**

Index current Device status by Device ID. Clear pressed feedback only for a disconnected event's assigned Editor Profile context. Apply `input_state` highlighting only when `event.deviceProfileId === editorProfile`; never infer from selected Device Management row.

- [ ] **Step 5: Run tests and commit**

Run: `rtk npm test -- src/HardwareMapping.test.tsx src/App.test.tsx`

Expected: PASS for exact target tuple, Device-local learning, draft behavior, disconnect, save fan-out status, and press attribution.

```bash
rtk git add src/HardwareMapping.tsx src/HardwareMapping.test.tsx src/App.tsx src/App.test.tsx src/types.ts
rtk git commit -m "feat: isolate learning and feedback by device"
```

---

### Task 7: Finish Metrics, Backup Preview, Responsive Layout, And UI Verification

**Files:**
- Modify: `src/HomeDashboard.tsx`
- Modify: `src/DeviceManagement.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/App.css`
- Modify: `src/i18n.ts`
- Create: `docs/verification/2026-07-31-device-management-ui.md`

**Interfaces:**
- Produces: Editor Profile aggregate metrics, selected-Device metrics/activity, complete backup preview counts, and verified desktop/mobile WebView layout.

- [ ] **Step 1: Add failing metrics and backup-preview tests**

Assert Home displays aggregate metrics for the Editor Profile across Devices; Device Management displays the selected Device filter; reassignment does not relabel history; activity rows show event-time Device name; and backup preview includes Device, assignment, metric-row, and activity counts while profile import/export preview does not.

- [ ] **Step 2: Implement the remaining data surfaces**

Keep HomeDashboard profile-scoped. Render selected-Device metrics in the detail panel with compact totals and activity table, not nested cards. Expand the existing restore confirmation summary with all v2 counts. Profile import/export buttons remain on the profile page and never mention physical Devices or history.

- [ ] **Step 3: Run the full frontend suite**

Run: `rtk npm test`

Expected: PASS.

Run: `rtk npm run build`

Expected: PASS TypeScript and Vite production build.

- [ ] **Step 4: Verify the real layout at desktop and minimum window sizes**

Run: `rtk npm run dev -- --host 127.0.0.1 --port 1420` and open preview fixtures at `http://127.0.0.1:1420`.

Inspect at 1120x760 and 760x560. Confirm no text/control overlap, no horizontal page scroll, stable row heights, usable selected detail panel, full visibility of long Device IDs via wrapping/copyable text, list/detail vertical collapse below 900px, and operable filters/assignment controls. Confirm each same-board Device remains separately selectable and no button resizes when statuses change.

- [ ] **Step 5: Record UI evidence**

Create the verification document with viewport sizes, fixture state, test/build commands, observed filter counts, assignment isolation result, and screenshot paths captured by the implementation agent.

- [ ] **Step 6: Commit the completed frontend**

```bash
rtk git add src/HomeDashboard.tsx src/DeviceManagement.tsx src/App.tsx src/App.test.tsx src/App.css src/i18n.ts docs/verification/2026-07-31-device-management-ui.md
rtk git commit -m "feat: finish multi-device management experience"
```
