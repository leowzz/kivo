# Shared Full-Height Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every Kivo page fill the Tauri window through one shared height and scrolling contract.

**Architecture:** Synchronize the Tauri window's logical inner height into the `--app-height` CSS property, with `100dvh` as the browser and error fallback. Carry that height through the existing shell and make each page root flex into the shared content panel while keeping scrolling at leaf content regions.

**Tech Stack:** React 19, TypeScript, Tauri 2 window API, CSS Grid/Flexbox, Vitest, Testing Library

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
- Consumes: Tauri `getCurrentWindow()`, `innerSize()`, `scaleFactor()`, `onResized()`, and `onScaleChanged()`.
- Produces: root CSS property `--app-height: <logical-pixels>px` and one shared full-height layout contract.

- [ ] **Step 1: Isolate the Tauri window test double and add viewport lifecycle coverage**

Replace the inline window mock with an accessible hoisted test double:

```tsx
const appWindow = vi.hoisted(() => ({
  innerSize: vi.fn<() => Promise<{ width: number; height: number }>>(),
  scaleFactor: vi.fn<() => Promise<number>>(),
  onResized: vi.fn<(callback: () => void) => Promise<() => void>>(),
  onScaleChanged: vi.fn<(callback: () => void) => Promise<() => void>>(),
}));

vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => appWindow }));
```

Reset the test double in `beforeEach`:

```tsx
document.documentElement.style.removeProperty("--app-height");
appWindow.innerSize.mockResolvedValue({ width: 2240, height: 1520 });
appWindow.scaleFactor.mockResolvedValue(2);
appWindow.onResized.mockResolvedValue(vi.fn());
appWindow.onScaleChanged.mockResolvedValue(vi.fn());
```

Add focused behavior and cleanup tests:

```tsx
test("tracks the Tauri window's logical height", async () => {
  render(<App />);

  await waitFor(() => expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("760px"));
  await waitFor(() => expect(appWindow.onResized).toHaveBeenCalledOnce());

  appWindow.innerSize.mockResolvedValue({ width: 2240, height: 1800 });
  appWindow.onResized.mock.calls[0][0]();

  await waitFor(() => expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("900px"));
  await waitFor(() => expect(appWindow.onScaleChanged).toHaveBeenCalledOnce());

  appWindow.scaleFactor.mockResolvedValue(3);
  appWindow.onScaleChanged.mock.calls[0][0]();

  await waitFor(() => expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("600px"));
});

test("releases the Tauri window listeners on unmount", async () => {
  const unlistenResize = vi.fn();
  const unlistenScale = vi.fn();
  appWindow.onResized.mockResolvedValue(unlistenResize);
  appWindow.onScaleChanged.mockResolvedValue(unlistenScale);
  const { unmount } = render(<App />);

  await waitFor(() => expect(appWindow.onScaleChanged).toHaveBeenCalledOnce());
  unmount();

  expect(unlistenResize).toHaveBeenCalledOnce();
  expect(unlistenScale).toHaveBeenCalledOnce();
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

Expected: FAIL because `HEAD` never sets `--app-height`; the assertion receives an empty string instead of `760px`.

- [ ] **Step 3: Synchronize the logical Tauri window height**

In `src/App.tsx`, import `getCurrentWindow` and add this effect before snapshot loading:

```tsx
useEffect(() => {
  if (PREVIEW_MODE) return;
  const appWindow = getCurrentWindow();
  let active = true;
  let unlistenResize: (() => void) | undefined;
  let unlistenScale: (() => void) | undefined;
  const syncHeight = async () => {
    const [size, scaleFactor] = await Promise.all([appWindow.innerSize(), appWindow.scaleFactor()]);
    if (active) document.documentElement.style.setProperty("--app-height", `${size.height / scaleFactor}px`);
  };

  void syncHeight().catch(() => undefined);
  void appWindow.onResized(() => void syncHeight()).then((unlisten) => { unlistenResize = unlisten; });
  void appWindow.onScaleChanged(() => void syncHeight()).then((unlisten) => { unlistenScale = unlisten; });
  return () => {
    active = false;
    unlistenResize?.();
    unlistenScale?.();
  };
}, []);
```

- [ ] **Step 4: Apply the shared CSS height and scroll contract**

Use these exact layout rules in `src/App.css`, preserving existing visual declarations:

```css
html, body, #root { width: 100%; height: 100%; overflow: hidden; }
.product-shell { height: var(--app-height, 100dvh); min-height: 0; }
.product-workspace { min-height: 0; height: 100%; }
.content-panel { min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
.home-dashboard { min-height: 0; flex: 1; overflow: hidden; }
.keypad-stage { min-height: 0; flex: 1; overflow: auto; }
.hardware-view { min-height: 0; flex: 1; display: flex; flex-direction: column; overflow: hidden; }
.source-list { min-height: 0; flex: 1; overflow: auto; }
.empty-workspace { min-height: 0; flex: 1; }
```

Remove the page-specific `calc(100vh - 118px)`, `height: 100%`, and `min-height: 100%` sizing rules that conflict with this contract. Keep the existing `@media` layout overrides, changing only the obsolete home-dashboard `height: auto` declaration to `flex: initial`.

- [ ] **Step 5: Run the focused test**

Run: `rtk npm test -- src/App.test.tsx`

Expected: all `src/App.test.tsx` tests pass, including the logical-height and listener-cleanup tests.

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
- Consumes: the shared `--app-height` contract from Task 1.
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
