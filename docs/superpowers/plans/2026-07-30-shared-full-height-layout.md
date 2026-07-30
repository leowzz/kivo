# Shared Full-Height Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every Kivo page fill the Tauri window through one shared height and scrolling contract.

**Architecture:** Reserve the shell's second grid row for the optional error banner, place the workspace explicitly in the flexible third row, size the shell with the WebView's native `100dvh`, and keep scrolling at leaf content regions.

**Tech Stack:** React 19, TypeScript, CSS Grid/Flexbox, Vitest, Testing Library

## Global Constraints

- Do not add a UI component library or any other dependency.
- Keep the existing visual design, navigation, responsive breakpoints, and business behavior.
- Cover home, button behavior, hardware mapping, button layout, and the no-model empty state.
- Preserve unrelated working-tree changes and keep commits scoped.

---

### Task 1: Shared Window Height Contract

**Files:**
- Modify: `src/App.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.css`

**Interfaces:**
- Consumes: the WebView's native dynamic viewport.
- Produces: one shared `100dvh` full-height layout contract.

- [ ] **Step 1: Add native viewport regression coverage**

Assert that the application does not script a root height override:

```tsx
test("does not override the WebView viewport height", async () => {
  render(<App />);

  await screen.findByRole("heading", { name: "按键概览" });
  expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");
});
```

- [ ] **Step 2: Prove the regression test fails against `HEAD` without altering the working tree**

Run:

```bash
layout_tmp=$(rtk proxy mktemp -d /tmp/kivo-layout-red.XXXXXX)
rtk proxy git archive HEAD | rtk proxy tar -x -C "$layout_tmp"
rtk proxy ln -s /Users/leo/work/kivo/node_modules "$layout_tmp/node_modules"
rtk proxy cp src/App.test.tsx "$layout_tmp/src/App.test.tsx"
rtk npm --prefix "$layout_tmp" test -- src/App.test.tsx
case "$layout_tmp" in /tmp/kivo-layout-red.*) rtk proxy rm -rf -- "$layout_tmp" ;; *) exit 1 ;; esac
```

Expected: FAIL while the shell still uses `var(--app-height, 100dvh)`.

- [ ] **Step 3: Use the native WebView viewport**

Remove the Tauri window size effect from `src/App.tsx` and use native CSS sizing:

```css
.product-shell { height: 100dvh; min-height: 0; }
```

- [ ] **Step 4: Apply the shared CSS height and scroll contract**

Use these exact layout rules in `src/App.css`, preserving existing visual declarations:

```css
html, body, #root { width: 100%; height: 100%; overflow: hidden; }
.product-shell { height: 100dvh; min-height: 0; }
.product-workspace { grid-row: 3; min-height: 0; height: 100%; }
.content-panel { min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
.home-dashboard { min-height: 0; flex: 1; overflow: hidden; }
.home-main { min-width: 0; min-height: 0; overflow: auto; }
.keypad-stage { min-height: 0; flex: 1; overflow: auto; }
.hardware-view { min-height: 0; flex: 1; display: flex; flex-direction: column; overflow: hidden; }
.source-list { min-height: 0; flex: 1; overflow: auto; }
.empty-workspace { min-height: 0; flex: 1; }
```

Remove the page-specific `calc(100vh - 118px)`, `height: 100%`, and `min-height: 100%` sizing rules that conflict with this contract. Keep the existing `@media` layout overrides, changing only the obsolete home-dashboard `height: auto` declaration to `flex: initial`. Verify that `.product-workspace`, `.sidebar`, `.content-panel`, and `.action-panel` have a bottom edge equal to the viewport height; on the home page, verify `.home-main` scrolls without stretching `.activity-log`.

- [ ] **Step 5: Run the focused test**

Run: `rtk npm test -- src/App.test.tsx`

Expected: all `src/App.test.tsx` tests pass, including the native viewport contract test.

- [ ] **Step 6: Run full automated verification**

Run:

```bash
rtk npm test
rtk npm run build
rtk git diff --check
```

Expected: 5 test files pass with 0 failures, the TypeScript/Vite production build exits 0, and `git diff --check` reports no whitespace errors.

- [ ] **Step 7: Commit the scoped implementation**

Run:

```bash
rtk git add src/App.tsx src/App.css src/App.test.tsx
rtk git commit -m "fix: fill Kivo window across pages"
```

Expected: the commit contains only the three frontend layout files.

### Task 2: Real Window Verification

**Files:**
- Verify only: `src/App.tsx`, `src/App.css`, `src/App.test.tsx`

**Interfaces:**
- Consumes: the shared `100dvh` contract from Task 1.
- Produces: visual evidence that every page reaches the Tauri window bottom edge.

- [ ] **Step 1: Start the Tauri development application**

Run: `rtk npm run tauri dev`

Expected: Vite listens on `http://localhost:1420` and the Kivo window opens without frontend or Rust build errors.

- [ ] **Step 2: Inspect the default window**

At 1120 x 760, open 首页, 按键行为, 硬件映射, and 按键布局. On each page, verify the sidebar background and main page frame reach the bottom edge. On 按键行为 and 按键布局, also verify the right action panel reaches the bottom edge.

The existing `deletes the last model and keeps import and restore available` component test covers the no-model render path. Do not delete a real local model solely for visual verification.

- [ ] **Step 3: Inspect an enlarged window and scrolling**

Enlarge the window vertically, revisit all four pages, and verify no blank band appears below the application shell. Confirm long hardware and action content scrolls inside its panel rather than extending the page frame.

- [ ] **Step 4: Re-run final checks after visual inspection**

Run:

```bash
rtk npm test
rtk npm run build
rtk git diff --check
rtk git status --short
```

Expected: tests and build pass, no whitespace errors appear, and only unrelated pre-existing files such as `output/` remain outside the scoped implementation commit.
