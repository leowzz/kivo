# Manual Hotkey Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users configure supported multi-modifier shortcuts without pressing keys that another application may intercept.

**Architecture:** Keep the existing `ButtonAction` shape and autosave flow. Add backend-matching modifier and ordinary-key choices directly to `ActionEditor`; both manual changes and keyboard recording continue to replace the same hotkey action.

**Tech Stack:** React 19, TypeScript, native checkbox/select controls, Testing Library, Vitest, existing CSS.

## Global Constraints

- Support `cmd`, `ctrl`, `alt`, and `shift` in canonical recording order, followed by exactly one ordinary key.
- Ordinary keys are A-Z, 0-9, Enter, Escape, Backspace, Tab, Space, Delete, arrows, Home, End, Page Up, and Page Down.
- Keep keyboard recording available.
- Do not add Fn, multiple ordinary keys, dependencies, schema changes, backend changes, or firmware changes.
- Preserve unrelated local changes in `Makefile`, `docs/superpowers/specs/2026-07-29-helper-kill-design.md`, and `test/test_helper_kill.sh`.

---

### Task 1: Add Inline Manual Hotkey Controls

**Files:**
- Modify: `src/App.test.tsx`
- Modify: `src/ActionEditor.tsx`
- Modify: `src/App.css`

**Interfaces:**
- Consumes: existing `{ type: "hotkey"; keys: string[] }`, `formatHotkey(keys)`, and `ActionEditorProps.onChange(actions)`.
- Produces: accessible checkboxes named `Cmd`, `Ctrl`, `Alt`, and `Shift`, plus an ordinary-key combobox named by `behavior.shortcut`.

- [ ] **Step 1: Write the failing integration test**

Add this test after `records a shortcut from the application window` in `src/App.test.tsx`:

```tsx
test("manually selects a multi-modifier shortcut", async () => {
  const user = userEvent.setup();
  render(<App />);
  const editor = await screen.findByRole("complementary", { name: "2" });

  await user.click(screen.getByRole("button", { name: "按下按键" }));
  await user.click(within(editor).getByRole("checkbox", { name: "Cmd" }));
  await user.click(within(editor).getByRole("checkbox", { name: "Ctrl" }));
  await user.click(within(editor).getByRole("checkbox", { name: "Shift" }));
  await user.selectOptions(within(editor).getByRole("combobox", { name: "按键" }), "k");

  expect(within(editor).getByText("Command + Control + Shift + K")).toBeInTheDocument();
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_model", {
    model: expect.objectContaining({
      actions: { DIGIT_2: [{ type: "hotkey", keys: ["cmd", "ctrl", "shift", "k"] }] },
    }),
  }), { timeout: 1600 });
});
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
rtk npm test -- src/App.test.tsx -t "manually selects a multi-modifier shortcut"
```

Expected: FAIL because no checkbox named `Cmd` exists.

- [ ] **Step 3: Add the supported choices and modifier update helper**

Add above `move` in `src/ActionEditor.tsx`:

```tsx
const HOTKEY_MODIFIERS = [
  { value: "cmd", label: "Cmd" },
  { value: "ctrl", label: "Ctrl" },
  { value: "alt", label: "Alt" },
  { value: "shift", label: "Shift" },
] as const;

const HOTKEY_KEYS = [
  ..."abcdefghijklmnopqrstuvwxyz",
  ..."0123456789",
  "enter", "escape", "backspace", "tab", "space", "delete",
  "up", "down", "left", "right", "home", "end", "page_up", "page_down",
];

function setHotkeyModifier(keys: string[], modifier: string, enabled: boolean) {
  const ordinaryKey = keys.at(-1) ?? "enter";
  return [
    ...HOTKEY_MODIFIERS
      .filter(({ value }) => value === modifier ? enabled : keys.includes(value))
      .map(({ value }) => value),
    ordinaryKey,
  ];
}
```

This reuses the backend's accepted key names and preserves exactly one final ordinary key.

- [ ] **Step 4: Render native manual controls**

Add this block between the existing hotkey `<output>` and record button in `src/ActionEditor.tsx`:

```tsx
<div className="hotkey-manual">
  <div className="hotkey-modifiers">
    {HOTKEY_MODIFIERS.map((modifier) => (
      <label key={modifier.value}>
        <input
          type="checkbox"
          checked={action.keys.includes(modifier.value)}
          onChange={(event) => replace(index, {
            type: "hotkey",
            keys: setHotkeyModifier(action.keys, modifier.value, event.target.checked),
          })}
        />
        <span>{modifier.label}</span>
      </label>
    ))}
  </div>
  <select
    aria-label={t(language, "behavior.shortcut")}
    value={action.keys.at(-1) ?? "enter"}
    onChange={(event) => replace(index, {
      type: "hotkey",
      keys: [...action.keys.slice(0, -1), event.target.value],
    })}
  >
    {HOTKEY_KEYS.map((key) => (
      <option value={key} key={key}>{formatHotkey([key])}</option>
    ))}
  </select>
</div>
```

- [ ] **Step 5: Add compact, stable layout styles**

Add after `.hotkey-field output` in `src/App.css`:

```css
.hotkey-manual { grid-column: 1 / -1; display: grid; gap: 7px; }
.hotkey-modifiers { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 6px; }
.hotkey-modifiers label { min-height: 30px; display: flex; align-items: center; justify-content: center; gap: 4px; border: 1px solid #d4dbd8; border-radius: 5px; color: #48524f; font-size: 11px; }
.hotkey-modifiers input { margin: 0; }
.hotkey-manual select { width: 100%; min-width: 0; height: 32px; padding: 0 7px; }
```

- [ ] **Step 6: Run focused manual and recording tests**

Run:

```bash
rtk npm test -- src/App.test.tsx -t "shortcut"
```

Expected: both manual selection and application-window recording tests PASS.

- [ ] **Step 7: Run full verification**

Run:

```bash
rtk npm test
rtk npm run build
rtk git diff --check
```

Expected: all tests PASS, production build succeeds, and diff check reports no errors.

- [ ] **Step 8: Commit only the feature files**

```bash
rtk git add src/App.test.tsx src/ActionEditor.tsx src/App.css
rtk git commit -m "feat: support manual hotkey selection"
```
