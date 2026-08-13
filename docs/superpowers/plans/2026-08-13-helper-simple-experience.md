# Kivo Helper Simple Experience Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Kivo open directly onto the selected physical keyboard, guide new devices through a three-step setup, and move low-frequency technical controls behind a clear Advanced Settings entry.

**Architecture:** Keep `AppSnapshot`, Device Profile, Hardware Profile, Runtime Assignment, and the existing Tauri commands authoritative. Add one physical-device selection context in `App`, compose the existing `Keypad` and `ActionEditor` into a focused daily workspace, and extract settings/advanced controls into focused components instead of growing `App.tsx` or duplicating state. Every profile mutation continues through the existing draft/autosave path, with an explicit edit-scope gate before changing a profile shared by multiple devices.

**Tech Stack:** React 19, TypeScript 7, Vitest, Testing Library, lucide-react, Tauri 2, existing Rust workspace/runtime, existing CSS token system.

## Global Constraints

- Preserve all existing Device Profile, Hardware Profile, Runtime Assignment, firmware protocol, and Workspace schema contracts.
- Candidate devices cannot receive a Runtime Assignment before HELLO and identity validation pass.
- The top device switcher changes only the visible physical-device context; it must not call `save_settings`, `save_runtime_assignment`, or `clear_runtime_assignment`.
- Persist the last selected Device ID as UI preference key `kivo:selected-device-id` in `localStorage`; it is not Workspace data and never replaces Device validation.
- `editorProfile` remains the advanced configuration-file editor target and is not reused as the selected physical device.
- Existing profiles, layouts, actions, I/O mappings, trigger thresholds, import/export, backup/restore, duplicate, and delete capabilities remain accessible.
- Valid inline edits use the existing autosave queue; Action dialogs retain explicit Save/Cancel semantics for incomplete drafts.
- A shared profile cannot be mutated until the user chooses current-device copy or shared edit. Current-device copy must use the existing atomic `duplicate_profile_for_device` command.
- Ordinary pages do not show configuration IDs, Hardware Profile IDs, Runtime Assignment, board IDs, ports, or protocol versions.
- Use the existing green/neutral visual language, lucide-react icons, 8px-or-less control radii, explicit focus states, and no decorative cards or new frontend dependencies.
- Preserve unrelated Workbench One work. Stage only files named by each task.
- Run repository commands through `rtk`; run Node commands through `rtk direnv exec .` so `.nvmrc` supplies Node 24.

## File Map

**Create:**

- `src/deviceSelection.ts` — deterministic physical-device selection and assigned-profile lookup.
- `src/deviceSelection.test.ts` — pure selection-order and fallback tests.
- `src/DeviceSwitcher.tsx` / `src/DeviceSwitcher.test.tsx` — accessible top-bar physical-device selector.
- `src/KeyboardWorkspace.tsx` / `src/KeyboardWorkspace.test.tsx` — default keyboard, empty, offline, and setup states.
- `src/SharedProfileEditDialog.tsx` / `src/SharedProfileEditDialog.test.tsx` — shared-edit scope decision UI.
- `src/inputMapping.ts` / `src/inputMapping.test.ts` — physical-input-to-button resolution shared by runtime highlighting and setup testing.
- `src/ProfileManager.tsx` / `src/ProfileManager.test.tsx` — configuration lifecycle UI extracted from `App`.
- `src/SettingsWorkspace.tsx` / `src/SettingsWorkspace.test.tsx` — basic language/backup settings and Advanced Settings entry.
- `src/AdvancedSettings.tsx` / `src/AdvancedSettings.test.tsx` — profile, layout, I/O, trigger, and technical advanced workspace.

**Modify:**

- `src/App.tsx` / `src/App.test.tsx` — device context, routes, edit orchestration, setup input events, and component composition.
- `src/ActionEditor.tsx` / `src/ActionEditor.test.tsx` — progressive trigger disclosure and common-action shortcuts.
- `src/ActionDialog.tsx` / `src/ActionDialog.test.tsx` — accept explicit create drafts for common actions.
- `src/Keypad.tsx` / `src/Keypad.test.tsx` — visible unconfigured state without replacing the key label.
- `src/DeviceSetupWizard.tsx` / `src/DeviceSetupWizard.test.tsx` — three-step recognized/profile/test flow.
- `src/DeviceManagement.tsx` / `src/DeviceManagement.test.tsx` — simplify to device status and management.
- `src/i18n.ts` / `src/i18n.test.ts` — user-facing labels for the new information architecture and states.
- `src/styles/app.css` and `src/styles/views.css` — stable desktop and narrow layouts.
- `src/preview.ts` — fixtures for ready, offline, unassigned, and shared-profile visual states.

---

### Task 1: Establish the Physical-Device Context and Top Switcher

**Files:**
- Create: `src/deviceSelection.ts`
- Create: `src/deviceSelection.test.ts`
- Create: `src/DeviceSwitcher.tsx`
- Create: `src/DeviceSwitcher.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/i18n.ts`
- Modify: `src/styles/app.css`

**Interfaces:**
- Produces `selectDeviceId(currentId: string | null, devices: readonly DeviceStatus[]): string | null`.
- Produces `assignedProfile(device: DeviceStatus | null, profiles: readonly DeviceProfile[]): DeviceProfile | undefined`.
- Produces `DeviceSwitcher({ devices, selectedDeviceId, language, onChange }: DeviceSwitcherProps)`.
- Later tasks consume the selected `DeviceStatus` and its assigned `DeviceProfile`; `editorProfile` is unchanged.

- [ ] **Step 1: Write failing pure-selection tests**

Add fixtures covering retained selection and fallback order:

```ts
expect(selectDeviceId("offline", [ready, offline])).toBe("offline");
expect(selectDeviceId("missing", [offline, attention, ready])).toBe("ready");
expect(selectDeviceId(null, [offline, unassigned])).toBe("unassigned");
expect(selectDeviceId(null, [])).toBeNull();
expect(assignedProfile(ready, [profile])).toBe(profile);
expect(assignedProfile(unassigned, [profile])).toBeUndefined();
```

The fallback rank is exact: online `ready`, online `configuring`/`learning`, online `unassigned` or error, online inactive, then offline; source order breaks ties.

- [ ] **Step 2: Run the pure tests and confirm RED**

Run: `rtk direnv exec . npm test -- src/deviceSelection.test.ts --run`

Expected: FAIL because `deviceSelection.ts` does not exist.

- [ ] **Step 3: Implement deterministic selection and assigned-profile lookup**

Implement these signatures without storing UI state in the helper:

```ts
export function selectDeviceId(
  currentId: string | null,
  devices: readonly DeviceStatus[],
): string | null;

export function assignedProfile(
  device: DeviceStatus | null,
  profiles: readonly DeviceProfile[],
): DeviceProfile | undefined;
```

Return `currentId` whenever that exact device still exists, including offline devices. Use `runtimeAssignment?.device_profile_id` for profile lookup and never fall back to `editorProfile`.

- [ ] **Step 4: Write failing DeviceSwitcher tests**

Render two same-board devices and assert user-visible physical identity:

```tsx
render(
  <DeviceSwitcher
    devices={[ready, offline]}
    selectedDeviceId={ready.deviceId}
    language="zh-CN"
    onChange={onChange}
  />,
);
expect(screen.getByRole("combobox", { name: "当前键盘" })).toHaveValue(ready.deviceId);
await user.selectOptions(screen.getByRole("combobox", { name: "当前键盘" }), offline.deviceId);
expect(onChange).toHaveBeenCalledWith(offline.deviceId);
expect(screen.getByRole("option", { name: "备用键盘 · 离线" })).toBeInTheDocument();
```

Also assert the no-device state renders “连接键盘” without an enabled empty select.

- [ ] **Step 5: Implement DeviceSwitcher and connect it to App**

Use a native select for predictable keyboard access. Add one App state:

```ts
const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);
const selectedDeviceIdValue = selectDeviceId(selectedDeviceId, devices);
const selectedDevice = devices.find(({ deviceId }) => deviceId === selectedDeviceIdValue) ?? null;
const selectedDeviceProfile = assignedProfile(selectedDevice, deviceProfiles);
```

Initialize the state from `localStorage.getItem("kivo:selected-device-id")`, reconcile it after snapshots, and write only valid explicit user selections back to that key. Do not write Kivo settings or Runtime Assignment. Replace the top aggregate `device-summary` with `DeviceSwitcher`; keep autosave status on the right. Make Device Management consume the same controlled selected ID instead of its separate `selectedManagedDeviceId` state.

Project pressed state to the current device instead of flattening every owner:

```ts
function pressedButtonsForDevice(
  owners: Map<string, PressedOwner>,
  selectedDeviceId: string | null,
): Set<string> {
  return new Set(selectedDeviceId ? owners.get(selectedDeviceId)?.buttonIds ?? [] : []);
}
```

Recompute this projection when the selected device or runtime events change, so same-profile devices cannot highlight one another.

- [ ] **Step 6: Prove switching has no persistence side effect**

In `App.test.tsx`, select another keyboard and assert its layout context changes while none of these commands run:

```ts
expect(invokedCommands()).not.toContain("save_settings");
expect(invokedCommands()).not.toContain("save_runtime_assignment");
expect(invokedCommands()).not.toContain("clear_runtime_assignment");
```

- [ ] **Step 7: Run focused tests and commit**

Run: `rtk direnv exec . npm test -- src/deviceSelection.test.ts src/DeviceSwitcher.test.tsx src/App.test.tsx --run`

Expected: PASS.

Commit:

```bash
rtk git add src/deviceSelection.ts src/deviceSelection.test.ts src/DeviceSwitcher.tsx src/DeviceSwitcher.test.tsx src/App.tsx src/App.test.tsx src/i18n.ts src/styles/app.css
rtk git commit -m "feat: add physical keyboard context"
```

### Task 2: Make the Clickable Keyboard the Default Workspace

**Files:**
- Create: `src/KeyboardWorkspace.tsx`
- Create: `src/KeyboardWorkspace.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/Keypad.tsx`
- Modify: `src/Keypad.test.tsx`
- Modify: `src/i18n.ts`
- Modify: `src/styles/app.css`
- Modify: `src/styles/views.css`

**Interfaces:**
- Consumes the selected physical device/profile from Task 1.
- Produces `KeyboardWorkspaceProps` with explicit callbacks:

```ts
interface KeyboardWorkspaceProps {
  language: Language;
  device: DeviceStatus | null;
  profile: DeviceProfile | undefined;
  hasCandidates: boolean;
  selectedButtonId: string | null;
  pressedButtonIds: Set<string>;
  onSelectButton(buttonId: string): void;
  onChangeActions(buttonId: string, actions: TriggerActions): void;
  onRenameButton(buttonId: string, label: string): void;
  onOpenSetup(deviceId: string | null): void;
}
```

- [ ] **Step 1: Write failing workspace-state tests**

Cover the exact state matrix:

```tsx
expect(renderWorkspace({ device: null }).getByText("连接你的键盘")).toBeInTheDocument();
expect(renderWorkspace({ device: unassigned }).getByRole("button", { name: "继续设置" })).toBeInTheDocument();
expect(renderWorkspace({ device: ready, profile }).getByRole("button", { name: /复制/ })).toBeInTheDocument();
expect(renderWorkspace({ device: offline, profile }).getByText("未连接")).toBeInTheDocument();
expect(renderWorkspace({ device: invalidAssignment }).getByRole("button", { name: "修复设置" })).toBeInTheDocument();
```

For a ready device, click a key and assert the right-side ActionEditor heading changes without resizing/remounting the keypad. Rerender with pressed IDs attributed to another Device and assert the selected keyboard does not highlight; only the selected Device's runtime events may populate `pressedButtonIds`.

- [ ] **Step 2: Run the workspace tests and confirm RED**

Run: `rtk direnv exec . npm test -- src/KeyboardWorkspace.test.tsx --run`

Expected: FAIL because `KeyboardWorkspace` does not exist.

- [ ] **Step 3: Implement KeyboardWorkspace as a pure composition component**

Use `Keypad` and `ActionEditor`; do not fetch, persist, or derive another profile target inside this component. Ready and offline-assigned devices render the same layout. Unassigned/invalid devices render a focused explanation and one primary setup action. No device renders an illustration-free connection empty state with automatic-detection copy.

Wire ActionEditor callbacks to the explicit selected button ID:

```tsx
<ActionEditor
  language={language}
  button={selectedButton}
  actions={selectedActions}
  onChange={(actions) => selectedButton && onChangeActions(selectedButton.id, actions)}
  onRename={onRenameButton}
/>
```

- [ ] **Step 4: Add the visible unconfigured-key state**

Keep the user label as the main text and add a secondary state marker when action count is zero:

```tsx
<span>{button.label}</span>
{count === 0
  ? <small className="key-state" aria-hidden="true">{unconfiguredLabel}</small>
  : <small aria-hidden="true">{count}</small>}
```

Extend `KeypadProps` with `unconfiguredLabel: string` and add `is-unconfigured` to the key class. Do not replace labels or IDs.

- [ ] **Step 5: Replace HomeDashboard on the home route**

Render `KeyboardWorkspace` for the existing `home` route first; retain the old behavior route temporarily for regression comparison. Use the selected device’s assigned profile, never `editorProfileConfig`. Change successful setup navigation from Device Management to `home` so the configured keyboard is immediately visible.

- [ ] **Step 6: Run focused tests and commit**

Run: `rtk direnv exec . npm test -- src/KeyboardWorkspace.test.tsx src/Keypad.test.tsx src/ActionEditor.test.tsx src/App.test.tsx --run`

Expected: PASS, including offline editing and setup-state tests.

Commit:

```bash
rtk git add src/KeyboardWorkspace.tsx src/KeyboardWorkspace.test.tsx src/Keypad.tsx src/Keypad.test.tsx src/App.tsx src/App.test.tsx src/i18n.ts src/styles/app.css src/styles/views.css
rtk git commit -m "feat: make keyboard the default workspace"
```

### Task 3: Gate Shared-Profile Edits Before Autosave

**Files:**
- Create: `src/SharedProfileEditDialog.tsx`
- Create: `src/SharedProfileEditDialog.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/i18n.ts`
- Modify: `src/styles/views.css`

**Interfaces:**
- Consumes Task 2’s `onChangeActions` and `onRenameButton` callbacks.
- Produces one App-level mutation entrypoint:

```ts
type ProfileMutation = (profile: DeviceProfile) => DeviceProfile;
function requestDeviceProfileMutation(mutation: ProfileMutation): void;
```

- Dialog emits `onChoose("device" | "shared")` or `onCancel()`.

- [ ] **Step 1: Write failing dialog tests**

Assert the prompt names the current keyboard, profile, affected count, and exposes exactly these choices:

```tsx
expect(screen.getByRole("button", { name: "仅修改这台键盘" })).toBeInTheDocument();
expect(screen.getByRole("button", { name: "同步修改 2 台键盘" })).toBeInTheDocument();
expect(screen.getByText("修改会影响使用此设置的其他键盘")).toBeInTheDocument();
```

- [ ] **Step 2: Write failing App integration tests for both branches**

For two devices sharing one profile:

1. Attempt a key rename and assert no profile changes and no save command occurs before a choice.
2. Choose shared edit and assert `save_device_profile` receives the renamed original profile.
3. In a fresh render, choose current-device edit and assert one `duplicate_profile_for_device` call receives a `source_profile` containing the rename while the original profile remains unchanged.
4. Reject the duplicate promise and assert the visible profile/assignment remain original and the error is retryable.

- [ ] **Step 3: Run focused tests and confirm RED**

Run: `rtk direnv exec . npm test -- src/SharedProfileEditDialog.test.tsx src/App.test.tsx --run`

Expected: FAIL because edits currently enter autosave without the new scope gate.

- [ ] **Step 4: Implement the scope gate**

Store only one pending mutation and one confirmed relationship key:

```ts
type PendingProfileMutation = {
  deviceId: string;
  profileId: string;
  apply: ProfileMutation;
};

const relationshipKey = `${selectedDevice.deviceId}:${profile.profile.id}`;
```

Rules:

- One consuming device: call `updateProfile(profile.id, mutation)` immediately.
- Shared profile with a confirmed `shared` scope for the same relationship: update immediately.
- Shared profile without a decision: store the mutation and open the dialog; do not modify the draft.
- An advanced editor profile not assigned to the selected Device has no `device` scope. Offer only an explicit shared-edit confirmation naming the affected count; never clone or assign it to an unrelated device.
- Choose `shared`: remember the relationship key and apply the mutation through `updateProfile`.
- Choose `device`: call `duplicate_profile_for_device` with `source_profile: mutation(profile)` and name `${profile.profile.name} (${device.name})`. This command atomically clones the edited content and reassigns only that device.
- Cancel: discard the pending mutation and leave profile/assignment untouched.
- Assignment/profile/device change: clear the remembered decision and stale pending mutation.

- [ ] **Step 5: Route every daily edit through the gate**

Replace direct `updateEditorProfile` use in KeyboardWorkspace callbacks with `requestDeviceProfileMutation`. Keep hardware learning’s captured draft path unchanged until Advanced Settings uses the same gate in Task 6.

- [ ] **Step 6: Run focused tests and commit**

Run: `rtk direnv exec . npm test -- src/SharedProfileEditDialog.test.tsx src/KeyboardWorkspace.test.tsx src/App.test.tsx --run`

Expected: PASS with no premature autosave.

Commit:

```bash
rtk git add src/SharedProfileEditDialog.tsx src/SharedProfileEditDialog.test.tsx src/App.tsx src/App.test.tsx src/i18n.ts src/styles/views.css
rtk git commit -m "feat: confirm shared keyboard edits"
```

### Task 4: Add Progressive Action Editing and Common Choices

**Files:**
- Modify: `src/ActionEditor.tsx`
- Modify: `src/ActionEditor.test.tsx`
- Modify: `src/ActionDialog.tsx`
- Modify: `src/ActionDialog.test.tsx`
- Modify: `src/i18n.ts`
- Modify: `src/styles/views.css`

**Interfaces:**
- Preserve existing `ActionEditorProps` callback signatures.
- Extend ActionDialog create initialization only through its existing `initial?: ActionDraft` prop; do not add a second draft model.

- [ ] **Step 1: Write failing common-action and disclosure tests**

For an unconfigured key, assert the editor shows “按下时 · 未设置” plus six choices. Test exact outcomes:

```ts
await user.click(screen.getByRole("button", { name: "复制" }));
expect(onChange).toHaveBeenCalledWith({
  press: [{ type: "hotkey", keys: ["primary", "c"] }],
  release: [], long_press: [], double_press: [],
});
```

Repeat for “粘贴” with `["primary", "v"]`. Assert “输入文字”, “快捷键”, “打开应用”, and “媒体控制” open ActionDialog with `press` plus action types `paste`, `hotkey`, `open`, and `media`. Configured advanced trigger groups remain visible; empty release/long/double groups remain hidden until “添加其他行为” is used.

- [ ] **Step 2: Run focused tests and confirm RED**

Run: `rtk direnv exec . npm test -- src/ActionEditor.test.tsx src/ActionDialog.test.tsx --run`

Expected: FAIL because the editor currently exposes only generic Add Action.

- [ ] **Step 3: Implement one preset table and draft-aware opening**

Use a single typed table:

```ts
type CommonAction = {
  key: "copy" | "paste" | "text" | "hotkey" | "open" | "media";
  draft: ActionDraft;
  commitImmediately: boolean;
};
```

Copy/paste append their complete hotkey Actions immediately. The other four set `dialogDraft` and `editingTarget = "create"`; ActionDialog already resets from `initial` whenever it opens. Keep “添加其他行为” as the generic create path with the existing default hotkey draft and trigger selector.

- [ ] **Step 4: Render Press as the primary section**

Always render a Press heading. If empty, render `未设置` and the common-action grid directly below it. Render release, long press, and double press only when configured. The generic button label becomes “添加其他行为”. Keep move/edit/delete behavior and Action summary formatting unchanged.

- [ ] **Step 5: Run focused tests and commit**

Run: `rtk direnv exec . npm test -- src/ActionEditor.test.tsx src/ActionDialog.test.tsx src/KeyboardWorkspace.test.tsx --run`

Expected: PASS.

Commit:

```bash
rtk git add src/ActionEditor.tsx src/ActionEditor.test.tsx src/ActionDialog.tsx src/ActionDialog.test.tsx src/i18n.ts src/styles/views.css
rtk git commit -m "feat: simplify common key actions"
```

### Task 5: Convert Device Setup into Recognize, Preset, and Test Steps

**Files:**
- Create: `src/inputMapping.ts`
- Create: `src/inputMapping.test.ts`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/DeviceSetupWizard.tsx`
- Modify: `src/DeviceSetupWizard.test.tsx`
- Modify: `src/i18n.ts`
- Modify: `src/styles/views.css`

**Interfaces:**
- Produces `resolveButton(hardware: HardwareProfile | undefined, input: PhysicalInput): string | null` by extracting the existing App helper unchanged.
- Adds a frontend-only setup event:

```ts
export interface SetupInputEvent {
  timestampMs: number;
  deviceId: string;
  input: PhysicalInput;
  pressed: boolean;
}
```

- Adds `inputEvent: SetupInputEvent | null` to `DeviceSetupWizardProps`.

- [ ] **Step 1: Extract and test physical-input resolution**

Move the existing direct/contact-matrix traversal from `App.tsx` to `inputMapping.ts`. Test direct GPIO, normalized contact pair, runtime source indexing, unknown input, and missing hardware. Keep the same return semantics used by normal pressed-key highlighting.

Run: `rtk direnv exec . npm test -- src/inputMapping.test.ts --run`

Expected before implementation: FAIL; after extraction: PASS and existing `App.test.tsx` remains green.

- [ ] **Step 2: Write failing three-step wizard tests**

For an online valid unassigned Device:

1. Assert step indicator “第 1 步，共 3 步”, recognized device name, and recommended profile.
2. Continue to preset selection; only exact-board compatible profiles appear and the first is selected.
3. Continue to test; the chosen layout appears.
4. Rerender with a matching `SetupInputEvent` Down/Up and assert the mapped key gains/loses `is-pressed`.
5. “跳过测试” and “完成设置” both call `onComplete` with the exact device/profile/hardware assignment.
6. Disconnect and reconnect the same Device ID on step 3; preserve the selected profile and resume testing.

Keep existing Candidate error tests: friendly issue, retry, create profile first, technical-details disclosure, and no identity bypass.

- [ ] **Step 3: Run wizard tests and confirm RED**

Run: `rtk direnv exec . npm test -- src/DeviceSetupWizard.test.tsx --run`

Expected: FAIL because the current flow has profile/confirmation rather than recognized/preset/test.

- [ ] **Step 4: Implement the explicit wizard state machine**

Use:

```ts
type SetupStep = "recognized" | "preset" | "test";
const [step, setStep] = useState<SetupStep>("recognized");
const [testPressedButtonIds, setTestPressedButtonIds] = useState<Set<string>>(new Set());
```

Reset to `recognized` only when the stable target Device ID changes. Candidate screens remain pre-wizard diagnostics. Profile creation returns to `preset`. In `test`, resolve `inputEvent.input` against the selected compatible Hardware Profile and update only the matching key’s pressed state. Do not write Runtime Assignment before `onComplete`.

- [ ] **Step 5: Feed real unassigned input events from App**

In the existing runtime event listener, publish a new setup event whenever `payload.input` and `payload.pressed` are non-null and the payload’s Device ID is the open setup target:

```ts
setSetupInputEvent({
  timestampMs: payload.timestampMs,
  deviceId: payload.deviceId,
  input: payload.input,
  pressed: payload.pressed,
});
```

This uses the existing `input_state` emitted even when the device is unassigned; the worker already skips Actions in that state. Clear setup input on close/target change. Do not start learning or assign a profile for this test.

- [ ] **Step 6: Run focused tests and commit**

Run: `rtk direnv exec . npm test -- src/inputMapping.test.ts src/DeviceSetupWizard.test.tsx src/deviceSetupSession.test.ts src/App.test.tsx --run`

Expected: PASS, including exact Device targeting and no early assignment.

Commit:

```bash
rtk git add src/inputMapping.ts src/inputMapping.test.ts src/App.tsx src/App.test.tsx src/DeviceSetupWizard.tsx src/DeviceSetupWizard.test.tsx src/i18n.ts src/styles/views.css
rtk git commit -m "feat: add guided keyboard setup"
```

### Task 6: Build Settings and the Advanced Settings Workspace

**Files:**
- Create: `src/ProfileManager.tsx`
- Create: `src/ProfileManager.test.tsx`
- Create: `src/SettingsWorkspace.tsx`
- Create: `src/SettingsWorkspace.test.tsx`
- Create: `src/AdvancedSettings.tsx`
- Create: `src/AdvancedSettings.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/i18n.ts`
- Modify: `src/styles/app.css`
- Modify: `src/styles/views.css`

**Interfaces:**
- `ProfileManager` owns no Tauri calls; it receives profile lifecycle callbacks.
- `SettingsWorkspace` emits `onLanguageChange`, `onBackup`, `onRestore`, and `onOpenAdvanced`.
- `AdvancedSettings` consumes the selected physical device/profile and emits all edits through Task 3’s `requestDeviceProfileMutation`.

Use these component contracts:

```ts
interface ProfileManagerProps {
  language: Language;
  profiles: DeviceProfile[];
  editorProfileId: string | null;
  devices: DeviceStatus[];
  onCreate(sourceProfileId?: string): void;
  onSelect(profileId: string): void;
  onImport(): void;
  onExport(profile: DeviceProfile): void;
  onDelete(profile: DeviceProfile): void;
}

interface SettingsWorkspaceProps {
  language: Language;
  onLanguageChange(language: Language): void;
  onBackup(): void;
  onRestore(): void;
  onOpenAdvanced(): void;
}

type AdvancedSection = "profiles" | "layout" | "io" | "technical";
```

- [ ] **Step 1: Write failing SettingsWorkspace tests**

Assert the ordinary settings page contains only application language, backup, restore, and one “高级设置” command. Assert it does not contain “I/O 映射”, “按键布局”, any port, or profile IDs. Selecting English calls `onLanguageChange("en-US")`; backup/restore call only their callbacks.

- [ ] **Step 2: Write failing ProfileManager extraction tests**

Move the current data-page behavior behind props and cover create, duplicate, import, export, delete, usage count, and editor badge. Selecting an editor profile must call `onSelect(profileId)` and must not alter a Runtime Assignment.

- [ ] **Step 3: Write failing AdvancedSettings tests**

Assert four tabs in fixed order: 配置文件, 按键布局, I/O 映射, 技术信息. Verify:

- layout renders `LayoutEditor` for the selected assigned profile;
- I/O renders `HardwareMapping` with the assigned Hardware Profile selected;
- trigger-threshold settings remain reachable from the layout section through `ConfigurationSettingsDialog`;
- technical info contains full Device ID, serial, board, firmware, protocol, and port;
- no assigned device leaves ProfileManager available; selecting an editor profile enables offline layout/I/O editing against that profile, while entity-bound learning, live test, and device technical details remain disabled with a clear message;
- layout and I/O mutations call the supplied gated mutation callback, never a direct save.

- [ ] **Step 4: Run component tests and confirm RED**

Run: `rtk direnv exec . npm test -- src/SettingsWorkspace.test.tsx src/ProfileManager.test.tsx src/AdvancedSettings.test.tsx --run`

Expected: FAIL because the components do not exist.

- [ ] **Step 5: Extract ProfileManager and implement SettingsWorkspace**

Move the current `data-page` JSX out of `App.tsx` without changing lifecycle callbacks. Implement SettingsWorkspace as unframed full-width sections separated by borders; do not nest cards. Route language changes through existing `saveSettings(editorProfile, nextLanguage)`, and backup/restore through existing dialogs and commands.

- [ ] **Step 6: Implement AdvancedSettings by composing existing editors**

Use controlled `AdvancedSection` state. The advanced edit target is the selected Device's assigned profile when present, otherwise the explicit ProfileManager `editorProfile` selection. Pass layout edits as:

```ts
onRequestProfileMutation((current) => ({ ...current, profile: nextLayout }));
```

Pass I/O edits as:

```ts
onRequestProfileMutation((current) => ({
  ...current,
  hardware_profiles: nextHardwareProfiles,
}));
```

Keep learning callbacks device-scoped. Use `ConfigurationSettingsDialog` for trigger thresholds and current-device duplication; its save callback also enters the shared-edit gate. Technical details are read-only. Configuration IDs appear only in ProfileManager/technical sections.

- [ ] **Step 7: Add temporary App routes without removing legacy routes**

Extend `View` with `settings` and `advanced`. Render the new components and wire existing callbacks. Keep legacy `behavior` and `data` routes until Task 7 removes their navigation, allowing focused regression during extraction.

- [ ] **Step 8: Run focused tests and commit**

Run: `rtk direnv exec . npm test -- src/SettingsWorkspace.test.tsx src/ProfileManager.test.tsx src/AdvancedSettings.test.tsx src/LayoutEditor.test.tsx src/HardwareMapping.test.tsx src/ConfigurationSettingsDialog.test.tsx src/App.test.tsx --run`

Expected: PASS with all lifecycle and advanced controls reachable.

Commit:

```bash
rtk git add src/ProfileManager.tsx src/ProfileManager.test.tsx src/SettingsWorkspace.tsx src/SettingsWorkspace.test.tsx src/AdvancedSettings.tsx src/AdvancedSettings.test.tsx src/App.tsx src/App.test.tsx src/i18n.ts src/styles/app.css src/styles/views.css
rtk git commit -m "feat: organize settings by user intent"
```

### Task 7: Simplify Navigation and Device Management

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/DeviceManagement.tsx`
- Modify: `src/DeviceManagement.test.tsx`
- Modify: `src/i18n.ts`
- Modify: `src/i18n.test.ts`
- Modify: `src/styles/app.css`
- Modify: `src/styles/views.css`
- Modify: `src/preview.ts`
- Delete: `src/HomeDashboard.tsx`

**Interfaces:**
- Final `View` is exactly `"keyboard" | "devices" | "settings" | "advanced"`.
- The sidebar exposes only keyboard/devices/settings; Advanced Settings is opened from Settings and has an explicit Back command.
- DeviceManagement retains device/candidate status and lifecycle callbacks, but no profile assignment, layout, I/O, or trigger editor props.

- [ ] **Step 1: Write failing final-navigation tests**

Assert the sidebar contains exactly these navigation buttons:

```ts
expect(sidebarButtons()).toEqual(["我的键盘", "设备", "设置"]);
```

Assert “首页”, “按键行为”, and “配置文件” are absent from the sidebar. Opening Advanced Settings must happen through Settings; Back returns to Settings. After setup completion the active page is “我的键盘”.

- [ ] **Step 2: Write failing simplified-device tests**

Keep coverage for selection, filters, candidate retry, add/continue setup, rename, offline-confirmed forget, selected metrics, and technical-details disclosure. Assert ordinary device details do not show assignment controls, layout tabs, I/O mapping, configuration settings, or ports before technical details expand.

- [ ] **Step 3: Run focused tests and confirm RED**

Run: `rtk direnv exec . npm test -- src/DeviceManagement.test.tsx src/App.test.tsx src/i18n.test.ts --run`

Expected: FAIL because legacy routes and advanced Device Management controls are still visible.

- [ ] **Step 4: Remove professional controls from DeviceManagement**

Delete assignment draft state, configuration tabs, LayoutEditor, HardwareMapping, ConfigurationSettingsDialog, shared-edit warning, and their props. Preserve:

- device/candidate list and filters;
- friendly primary status;
- rename, add, continue setup, retry, and forget;
- current-device metrics/activity;
- collapsed technical details.

Use the shared top-level selected Device ID from Task 1; do not create a second selection source.

- [ ] **Step 5: Replace final navigation and remove legacy page branches**

Rename the home route to `keyboard`, render only three text sidebar commands, and remove standalone behavior/data branches. ProfileManager remains accessible under Advanced Settings. Remove `HomeDashboard` import and file after all tests stop referencing it. Keep `homeMetrics` only where device metrics/runtime updates still require it; do not remove backend metrics.

- [ ] **Step 6: Update preview states and copy**

Ensure preview data demonstrates:

- one online ready selected keyboard;
- one offline assigned keyboard;
- two devices sharing one profile for the edit-scope dialog;
- one unassigned online device for setup;
- one Candidate error for recovery.

Update i18n tests so every new key exists in Chinese and English and remove only keys proven unused by `rtk rg`.

- [ ] **Step 7: Run focused tests and commit**

Run: `rtk direnv exec . npm test -- src/DeviceManagement.test.tsx src/KeyboardWorkspace.test.tsx src/SettingsWorkspace.test.tsx src/AdvancedSettings.test.tsx src/App.test.tsx src/i18n.test.ts --run`

Expected: PASS with only three ordinary navigation entries.

Commit:

```bash
rtk git add src/App.tsx src/App.test.tsx src/DeviceManagement.tsx src/DeviceManagement.test.tsx src/i18n.ts src/i18n.test.ts src/styles/app.css src/styles/views.css src/preview.ts src/HomeDashboard.tsx
rtk git commit -m "feat: simplify helper navigation"
```

### Task 8: Responsive, Visual, and Whole-Repository Verification

**Files:**
- Modify only if verification exposes a defect: `src/styles/app.css`, `src/styles/views.css`, and the focused component/test owning that defect.

**Interfaces:**
- No new production interfaces.
- Completion evidence must cover desktop, narrow layout, full automated suites, and the physical-device flow where hardware is available.

- [ ] **Step 1: Add CSS contract assertions for stable geometry**

Extend component CSS-source tests to require:

```css
.keyboard-workspace { grid-template-columns: minmax(0, 1fr) minmax(320px, 380px); }
@media (max-width: 980px) { .keyboard-workspace { grid-template-columns: 1fr; } }
```

Also assert the narrow sidebar becomes a three-column text navigation row, the Action panel moves below the keypad, dialogs use `max-height: calc(100dvh - 20px)`, and no page-level horizontal overflow is introduced.

- [ ] **Step 2: Run the complete frontend suite and production build**

Run:

```bash
rtk direnv exec . npm test
rtk direnv exec . npm run build
rtk git diff --check
```

Expected: all Vitest files pass, TypeScript/Vite build exits 0, and diff check emits no errors.

- [ ] **Step 3: Run the repository acceptance target**

Run: `rtk direnv exec . make test`

Expected: Python release/upload tests, PlatformIO native tests, Cargo tests/clippy checks wired by the Makefile, and npm tests all pass. If hardware-only acceptance is not part of `make test`, report it separately rather than implying it ran.

- [ ] **Step 4: Start preview and inspect three viewports**

Run: `rtk direnv exec . npm run dev -- --host 127.0.0.1 --port 1421`

Open `http://127.0.0.1:1421/?preview` with the in-app browser. Capture and inspect:

- 1440x900: selected device, full keyboard, fixed right editor, three-entry sidebar;
- 1120x760: same hierarchy without compressed labels or overlapping top bar;
- 390x844: three text navigation items remain legible, keyboard fits width, editor follows below, no horizontal scroll.

Exercise ready, offline, unassigned/setup, shared-edit prompt, Settings, and every Advanced Settings tab. Check rendered screenshots and canvas/page pixels for nonblank content and coherent boundaries.

- [ ] **Step 5: Run keyboard-only accessibility checks**

Using the preview, Tab through device switcher, three navigation entries, every virtual key, Action controls, dialogs, and setup steps. Verify focus is visible, Escape cancels dialogs, Enter commits valid fields, disabled actions are announced, and dialog focus does not leak to the page.

- [ ] **Step 6: Perform physical acceptance when a Kivo device is available**

Complete this exact flow on macOS:

1. connect an unassigned device;
2. confirm the wizard opens once;
3. select the recommended preset;
4. press physical keys and see the setup test highlight the matching virtual keys;
5. complete setup and land on My Keyboard;
6. change one key Action and wait for “已保存”;
7. press the physical key and observe the new behavior;
8. disconnect it, confirm offline editing remains available, reconnect, and confirm runtime recovery.

If no suitable device is available, mark this entire step `NOT RUN`; automated event tests are not a substitute.

- [ ] **Step 7: Commit verification-driven fixes**

Stage only the exact files changed to fix observed defects. For example, when only the two shared style sheets and a workspace test change:

```bash
rtk git add src/styles/app.css src/styles/views.css src/KeyboardWorkspace.test.tsx
rtk git commit -m "fix: polish simple helper experience"
```

Skip this commit if verification required no code changes.

## Completion Audit

Before declaring completion, map each success criterion in `docs/superpowers/specs/2026-08-13-helper-simple-experience-design.md` to evidence:

- no technical concepts in the ordinary setup flow: UI queries and screenshots;
- one-click daily editing: KeyboardWorkspace interaction test and preview;
- one-operation device switching: DeviceSwitcher/App test;
- no silent shared-profile fan-out: both edit-scope integration tests;
- offline/save-failure recovery: App and KeyboardWorkspace tests;
- all advanced capabilities retained: AdvancedSettings/ProfileManager tests;
- desktop and narrow layouts: screenshots plus CSS contracts;
- real device execution: physical acceptance result explicitly PASS or NOT RUN.

Do not treat passing tests alone as proof of the visual or physical requirements.
