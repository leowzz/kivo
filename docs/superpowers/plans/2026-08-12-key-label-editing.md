# Key Label Editing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow the selected key's display label to be edited inline in the behavior panel while preserving its stable ID and existing actions.

**Architecture:** `ActionEditor` owns only the transient label-edit UI state and emits a dedicated label-change callback after validation. `App` applies that callback to the matching `ModelButton` by ID through the existing profile update/autosave path. The title area uses the existing icon-button conventions for save and cancel.

**Tech Stack:** React, TypeScript, Vitest, Testing Library, lucide-react, existing Kivo autosave/profile state.

## Global Constraints

- The button `id` is immutable during renaming.
- A label is trimmed before saving and must not be blank.
- Cancel never calls the profile update callback.
- Action data and action IDs remain unchanged.

### Task 1: Add failing component tests

**Files:**
- Modify: `/Users/leo/work/kivo/src/ActionEditor.test.tsx`
- Test: `/Users/leo/work/kivo/src/ActionEditor.test.tsx`

- [x] Add tests proving the current label is displayed, editing opens an accessible textbox, cancel leaves the original label and emits no label update, save trims and emits the same button ID with the new label, and blank input cannot be saved.
- [x] Run `rtk test npm test -- src/ActionEditor.test.tsx --run` and confirm the new tests fail because the callback/UI does not exist.

### Task 2: Implement inline label editing

**Files:**
- Modify: `/Users/leo/work/kivo/src/ActionEditor.tsx`
- Modify: `/Users/leo/work/kivo/src/App.tsx`
- Modify: `/Users/leo/work/kivo/src/i18n.ts` only if existing labels are insufficient.
- Modify: `/Users/leo/work/kivo/src/styles/views.css` only if the existing title/button styles do not support the inline row.

**Interfaces:**
- `ActionEditorProps.onRename(buttonId: string, label: string): void`.

- [x] Add local `editingLabel` and `labelDraft` state keyed to the currently rendered button, with cancel/reset behavior when selection changes.
- [x] Render the normal label plus a rename icon button; in edit mode render a textbox and check/X icon buttons with localized accessible names.
- [x] Disable save for blank trimmed labels and trim before calling `onRename`.
- [x] Pass `updateEditorProfile` logic from `App` that maps through layout groups, replaces only the matching button label, and leaves the ID/actions untouched.

### Task 3: Verify the focused and broader suites

**Files:**
- No additional files.

- [x] Run `rtk test npm test -- src/ActionEditor.test.tsx --run` and confirm all focused tests pass.
- [x] Run `rtk test npm test -- src/App.test.tsx src/Keypad.test.tsx --run` to cover the profile update and label rendering path.
- [x] Run `rtk git diff --check` and inspect the final diff for scope and ID preservation.
