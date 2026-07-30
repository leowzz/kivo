# 首页指标布局 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render the seven-day metrics as the selected model's keypad, place the model selector in the global top-right corner, and remove the language selector from the Chinese-first UI.

**Architecture:** Keep metrics transport unchanged. `HomeDashboard` will join heatmap data to `model.groups`, render each group with its declared column count, and format only the known button log message at display time. `App` will always render with `zh-CN`, keep one model selector in the top bar, and remove the language control.

**Tech Stack:** React 19, TypeScript, Vitest, Testing Library, CSS Grid.

## Global Constraints

- Do not alter saved model documents, settings schema, device protocol, or runtime event payloads.
- Use the model's existing group order, button order, and `columns` value.
- Use a low-contrast green color scale for entries with a press count.
- Do not add dependencies or a language-selection replacement.
- The model selector appears at the top-right on every page.

---

### Task 1: Lock and Implement Home Metrics UI

**Files:**
- Modify: `src/App.test.tsx`
- Modify: `src/HomeDashboard.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.css`

**Interfaces:**
- Consumes: `ModelConfig.model.groups`, `HomeMetricsSnapshot.heatmap`, and `HomeMetricsSnapshot.logs`.
- Produces: `.heatmap-group` grids with `gridTemplateColumns` set from each group, `.heat-cell` elements in real button order, Chinese log copy, and one `.topbar-model-picker`.

- [x] **Step 1: Write the failing test**

  In `src/App.test.tsx`, change the fixture model to two groups and add this test after the first home test:

  ```tsx
  test("renders seven-day metrics in model order with Chinese logs and one global model selector", async () => {
    currentSnapshot.models[0].model.groups = [
      { id: "digits", columns: 2, buttons: [{ id: "DIGIT_2", label: "2" }, { id: "DIGIT_5", label: "5" }] },
      { id: "actions", columns: 1, buttons: [{ id: "ENTER", label: "确认" }] },
    ];
    currentSnapshot.homeMetrics = {
      ...baseSnapshot.homeMetrics!,
      heatmap: [{ buttonId: "DIGIT_5", day: "2026-07-30", presses: 3 }],
      logs: [{ timestampMs: 1785396000000, kind: "button", message: "DIGIT_5 pressed" }],
    };
    render(<App />);

    await screen.findByRole("heading", { name: "按键概览" });
    expect([...document.querySelectorAll(".heat-cell")].map((item) => item.textContent)).toEqual([
      expect.stringContaining("2"), expect.stringContaining("5"), expect.stringContaining("确认"),
    ]);
    expect(screen.getByText("按下 DIGIT_5")).toBeInTheDocument();
    expect(screen.getByLabelText("选择设备型号")).toHaveClass("topbar-model-picker");
    expect(document.querySelector(".home-model-picker")).toBeNull();
    expect(screen.queryByLabelText("语言")).toBeNull();
  });
  ```

- [x] **Step 2: Run test to verify it fails**

  Run: `rtk npm run test -- src/App.test.tsx`

  Expected: FAIL because the heatmap contains only metric entries, raw `DIGIT_5 pressed` is displayed, and the model selector is not in the top bar.

- [x] **Step 3: Write minimal implementation**

  In `src/HomeDashboard.tsx`, remove `activeModel`, `loaded`, `models`, and `onModelChange` from `Props` and replace the heatmap body with the existing layout groups joined to a button-id lookup:

  ```tsx
  const heatmapByButton = new Map(metrics.heatmap.map((entry) => [entry.buttonId, entry]));
  {model?.model.groups.map((group) => (
    <div className="heatmap-group" key={group.id} style={{ gridTemplateColumns: `repeat(${group.columns}, minmax(0, 1fr))` }}>
      {group.buttons.map((button) => {
        const entry = heatmapByButton.get(button.id);
        const presses = entry?.presses ?? 0;
        return <div className="heat-cell" key={button.id} style={presses ? { backgroundColor: `rgba(23, 116, 87, ${.08 + (presses / maxHeat) * .24})` } : undefined}>
          <span>{button.label}</span>{presses > 0 && <><strong>{presses}</strong><small>{entry?.day.slice(5)}</small></>}
        </div>;
      })}
    </div>
  ))}
  ```

  Add a small local formatter and use it for the activity log:

  ```tsx
  function formatLog(message: string) {
    const match = /^(\S+) pressed$/.exec(message);
    return match ? `按下 ${match[1]}` : message;
  }
  ```

  In `src/App.tsx`, move the existing model `<select>` into the top bar with class `topbar-model-picker`, remove both the home and sidebar copies plus their `HomeDashboard` props, and set `setLanguage("zh-CN")` in `applySnapshot` rather than taking `snapshot.language`. Keep `saveSettings` unchanged so its persisted language remains `zh-CN`.

  In `src/App.css`, add `.topbar-model-picker` to the top bar's right side, replace the flat `.heatmap` declaration with a vertical grid of `.heatmap-group` elements, set each `.heatmap-group` to `display: grid`, and set `.heat-cell` to the existing key-like border plus a near-white default background. Delete selectors that only style `.home-model-picker`, `.model-picker`, and `.language-picker`.

- [x] **Step 4: Run test to verify it passes**

  Run: `rtk npm run test -- src/App.test.tsx`

  Expected: PASS.

- [x] **Step 5: Run the full frontend validation**

  Run: `rtk npm run test && rtk npm run build && rtk git diff --check`

  Expected: all tests and TypeScript/Vite build pass; diff check produces no output.

- [x] **Step 6: Commit**

  ```bash
  rtk git add src/App.test.tsx src/HomeDashboard.tsx src/App.tsx src/App.css
  rtk git commit -m "feat: render home metrics as keypad"
  ```
