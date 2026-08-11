# Device Management Selection Flicker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent the device management page from alternating between the first device and a previously selected non-first device when the page mounts.

**Architecture:** Keep `App`'s `selectedDeviceId` as the authority whenever it identifies a visible row, while retaining `DeviceManagement`'s local discriminated selection for candidates and nearest-row fallback. Resolve the effective selection during render, publish user choices synchronously, and use one reconciliation effect only for automatic fallback.

**Tech Stack:** React 19, TypeScript 7, Vitest 4, Testing Library

## Global Constraints

- Preserve device sorting, filtering, search, live status refresh, and detail rendering.
- Preserve the existing nearest-visible-row fallback when the selected row disappears.
- Do not change backend discovery or status reporting.
- Make no unrelated refactors.

---

### Task 1: Make controlled device selection authoritative

**Files:**
- Modify: `src/DeviceManagement.tsx:264-299,563-607`
- Test: `src/DeviceManagement.test.tsx:225-245`

**Interfaces:**
- Consumes: `selectedDeviceId?: string | null` and `onSelectedDeviceChange?(deviceId: string | null): void` from `DeviceManagementProps`.
- Produces: an effective `Selection | null` that uses the controlled device immediately, falls back to the nearest visible row, and publishes device or candidate row choices without an effect race.

- [x] **Step 1: Add regression tests for controlled mount and row publishing**

Add these tests beside the existing selection-preservation tests:

```tsx
test("applies a controlled non-first device before publishing selection", () => {
  const onSelectedDeviceChange = vi.fn();
  renderManagement({
    devices: [
      device(),
      device({ deviceId: "rp-b", name: "RP2040 B", hardwareSerial: "RP-B-002" }),
    ],
    selectedDeviceId: "rp-b",
    onSelectedDeviceChange,
  });

  expect(screen.getByRole("button", { name: /RP2040 B/ })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(onSelectedDeviceChange).not.toHaveBeenCalledWith("rp-a");
});

test("publishes explicit device and candidate row selections", async () => {
  const user = userEvent.setup();
  const onSelectedDeviceChange = vi.fn();
  renderManagement({ onSelectedDeviceChange });

  await user.click(screen.getByRole("button", { name: /RP2040 B/ }));
  expect(onSelectedDeviceChange).toHaveBeenLastCalledWith("rp-b");

  await user.click(screen.getByRole("button", { name: /AD-001/ }));
  expect(onSelectedDeviceChange).toHaveBeenLastCalledWith(null);
});
```

- [x] **Step 2: Run the focused tests and verify RED**

Run:

```bash
rtk npm test -- src/DeviceManagement.test.tsx
```

Expected: FAIL because the mount publishes `rp-a` before applying `rp-b`, and candidate selection does not publish `null`.

- [x] **Step 3: Resolve one effective selection and publish explicit row choices**

In `DeviceManagement`, replace the current row-reconciliation block, `activeSelection` fallback, controlled-selection effect, and selected-device publishing effect with logic equivalent to:

```tsx
const requestedSelection: Selection | null = controlledDeviceId
  ? { kind: "device", id: controlledDeviceId }
  : selection;
const requestedExists = requestedSelection && rows.some(
  (row) =>
    row.selection.kind === requestedSelection.kind &&
    row.selection.id === requestedSelection.id,
);
const previousIndex = requestedSelection
  ? previous.current.findIndex(
      (row) =>
        row.selection.kind === requestedSelection.kind &&
        row.selection.id === requestedSelection.id,
    )
  : 0;
const activeSelection = requestedExists
  ? requestedSelection
  : (rows[Math.max(0, Math.min(previousIndex, rows.length - 1))]?.selection ?? null);

useEffect(() => {
  if (
    selection?.kind !== activeSelection?.kind ||
    selection?.id !== activeSelection?.id
  ) {
    setSelection(activeSelection);
  }
  previous.current = rows;
}, [activeSelection?.id, activeSelection?.kind, rows, selection?.id, selection?.kind]);

const activeDeviceId =
  activeSelection?.kind === "device" ? activeSelection.id : null;
useEffect(() => {
  if (activeDeviceId !== (controlledDeviceId ?? null)) {
    onSelectedDeviceChange?.(activeDeviceId);
  }
}, [activeDeviceId, controlledDeviceId, onSelectedDeviceChange]);
```

Use a local row-selection helper so both row types update local state and publish the matching controlled value synchronously:

```tsx
const selectRow = (next: Selection) => {
  setSelection(next);
  onSelectedDeviceChange?.(next.kind === "device" ? next.id : null);
};
```

Replace both row `onClick` handlers with `selectRow(...)`. Do not change sorting, filter predicates, or row markup.

- [x] **Step 4: Run the focused tests and verify GREEN**

Run:

```bash
rtk npm test -- src/DeviceManagement.test.tsx
```

Expected: all `DeviceManagement` tests PASS, including the existing live-replacement and nearest-row tests.

- [x] **Step 5: Run related frontend regression and build checks**

Run:

```bash
rtk npm test -- src/App.test.tsx src/DeviceManagement.test.tsx
rtk npm run build
rtk git diff --check
```

Expected: both test files PASS, TypeScript and Vite build successfully, and `git diff --check` produces no output.

- [x] **Step 6: Commit the focused fix**

```bash
rtk git add src/DeviceManagement.tsx src/DeviceManagement.test.tsx docs/superpowers/plans/2026-08-11-device-management-selection-flicker.md
rtk git commit -m "fix: stabilize managed device selection"
```
