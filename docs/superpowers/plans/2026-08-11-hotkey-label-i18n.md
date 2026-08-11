# Hotkey Label Internationalization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show short localized names for every named key in the Action editor while preserving canonical hotkey tokens and current English labels.

**Architecture:** Keep token identity, validation, and serialization in `hotkey.ts`. Add typed i18n messages for named keys, then make the existing shared formatter language-aware so the Picker, chips, accessible names, search, and Action summaries all consume one label rule.

**Tech Stack:** TypeScript 7, React 19, Vitest 4, Testing Library, existing typed `i18n.ts` catalog.

## Global Constraints

- Chinese labels are short and direct, including exact examples `方向左` and `右cmd`.
- English labels remain compatible with the current UI.
- Letters, top-row digits, `F1` through `F24`, and symbol glyphs remain unchanged.
- Left and right physical modifiers remain distinguishable.
- Picker, selected chips, remove-button accessible names, recording results, search, and Action summaries use the same formatter.
- Internal tokens, key ordering, saved configuration, HID mapping, firmware protocol, and persisted schemas do not change.
- Unknown tokens retain the existing uppercase fallback.

---

## File Structure

- `src/i18n.ts`: owns Chinese and English named-key messages and compile-time dictionary completeness.
- `src/hotkey.ts`: owns the token-to-message mapping, literal symbol labels, language-aware formatter, and chord formatter.
- `src/HotkeyPicker.tsx`: passes the current language to the formatter for all visible, searchable, and accessible labels.
- `src/ActionEditor.tsx`: passes the current language when formatting hotkey summaries.
- `src/hotkey.test.ts`: locks the formatter contract and unchanged canonical tokens.
- `src/HotkeyPicker.test.tsx`: locks Chinese picker, chip, physical-modifier, and search behavior.
- `src/ActionEditor.test.tsx`: locks localized summaries and unchanged action data.
- `src/i18n.test.ts`: retains the structural-completeness check for both dictionaries.

### Task 1: Build the language-aware label formatter

**Files:**
- Modify: `src/i18n.ts:135-171,450-478`
- Modify: `src/hotkey.ts:60-130,243-250`
- Test: `src/hotkey.test.ts:45-65`

**Interfaces:**
- Consumes: `t(language: Language, key: MessageKey)` from `src/i18n.ts`.
- Produces: `hotkeyDisplayLabel(language: Language, token: string): string` and `formatHotkey(keys: string[], language: Language): string`.

- [ ] **Step 1: Write the failing formatter tests**

Add `hotkeyDisplayLabel` to the import from `./hotkey`, replace the existing label-formatting tests with:

```ts
test("keeps current English and literal hotkey labels", () => {
  expect(hotkeyDisplayLabel("en-US", "right_cmd")).toBe("Right Command");
  expect(hotkeyDisplayLabel("en-US", "numpad_0")).toBe("NUMPAD_0");
  expect(formatHotkey(["backtick", "left_bracket"], "en-US")).toBe("` + [");
});

test("formats named hotkeys in Chinese without changing their tokens", () => {
  expect(hotkeyDisplayLabel("zh-CN", "right_cmd")).toBe("右cmd");
  expect(hotkeyDisplayLabel("zh-CN", "left")).toBe("方向左");
  expect(formatHotkey(
    ["left_cmd", "right_ctrl", "left", "enter", "numpad_enter"],
    "zh-CN",
  )).toBe("左cmd + 右ctrl + 方向左 + 回车 + 小键盘回车");
});
```

Keep the normalization and validation tests unchanged; they prove display localization does not alter token identity.

- [ ] **Step 2: Run the formatter tests to verify RED**

Run: `rtk npm test -- src/hotkey.test.ts`

Expected: FAIL because `hotkeyDisplayLabel` and `formatHotkey` do not accept a language and still return English labels.

- [ ] **Step 3: Add complete typed key messages**

Add these entries beside the existing `behavior.keyCategory.*` entries in `zhCN`:

```ts
  "behavior.key.primary": "cmd/ctrl",
  "behavior.key.cmd": "cmd",
  "behavior.key.ctrl": "ctrl",
  "behavior.key.optionAlt": "option/alt",
  "behavior.key.shift": "shift",
  "behavior.key.leftCmd": "左cmd",
  "behavior.key.rightCmd": "右cmd",
  "behavior.key.leftCtrl": "左ctrl",
  "behavior.key.rightCtrl": "右ctrl",
  "behavior.key.leftAlt": "左option/alt",
  "behavior.key.rightAlt": "右option/alt",
  "behavior.key.leftShift": "左shift",
  "behavior.key.rightShift": "右shift",
  "behavior.key.enter": "回车",
  "behavior.key.escape": "Esc",
  "behavior.key.backspace": "退格",
  "behavior.key.tab": "Tab",
  "behavior.key.space": "空格",
  "behavior.key.arrowUp": "方向上",
  "behavior.key.arrowDown": "方向下",
  "behavior.key.arrowLeft": "方向左",
  "behavior.key.arrowRight": "方向右",
  "behavior.key.delete": "删除",
  "behavior.key.home": "行首",
  "behavior.key.end": "行尾",
  "behavior.key.pageUp": "上翻页",
  "behavior.key.pageDown": "下翻页",
  "behavior.key.capsLock": "大写锁定",
  "behavior.key.printScreen": "截屏",
  "behavior.key.scrollLock": "滚动锁定",
  "behavior.key.pause": "暂停",
  "behavior.key.insert": "插入",
  "behavior.key.application": "菜单",
  "behavior.key.numLock": "数字锁定",
  "behavior.key.numpad0": "小键盘0",
  "behavior.key.numpad1": "小键盘1",
  "behavior.key.numpad2": "小键盘2",
  "behavior.key.numpad3": "小键盘3",
  "behavior.key.numpad4": "小键盘4",
  "behavior.key.numpad5": "小键盘5",
  "behavior.key.numpad6": "小键盘6",
  "behavior.key.numpad7": "小键盘7",
  "behavior.key.numpad8": "小键盘8",
  "behavior.key.numpad9": "小键盘9",
  "behavior.key.numpadDivide": "小键盘/",
  "behavior.key.numpadMultiply": "小键盘*",
  "behavior.key.numpadSubtract": "小键盘-",
  "behavior.key.numpadAdd": "小键盘+",
  "behavior.key.numpadEnter": "小键盘回车",
  "behavior.key.numpadDecimal": "小键盘.",
  "behavior.key.numpadEqual": "小键盘=",
```

Add the same keys to `enUS`, using the exact current labels, including the
existing uppercase fallback for keypad digits:

```ts
  "behavior.key.primary": "Primary (Command / Control)",
  "behavior.key.cmd": "Command",
  "behavior.key.ctrl": "Control",
  "behavior.key.optionAlt": "Option / Alt",
  "behavior.key.shift": "Shift",
  "behavior.key.leftCmd": "Left Command",
  "behavior.key.rightCmd": "Right Command",
  "behavior.key.leftCtrl": "Left Control",
  "behavior.key.rightCtrl": "Right Control",
  "behavior.key.leftAlt": "Left Option / Alt",
  "behavior.key.rightAlt": "Right Option / Alt",
  "behavior.key.leftShift": "Left Shift",
  "behavior.key.rightShift": "Right Shift",
  "behavior.key.enter": "Enter",
  "behavior.key.escape": "Escape",
  "behavior.key.backspace": "Backspace",
  "behavior.key.tab": "Tab",
  "behavior.key.space": "Space",
  "behavior.key.arrowUp": "Arrow Up",
  "behavior.key.arrowDown": "Arrow Down",
  "behavior.key.arrowLeft": "Arrow Left",
  "behavior.key.arrowRight": "Arrow Right",
  "behavior.key.delete": "Delete",
  "behavior.key.home": "Home",
  "behavior.key.end": "End",
  "behavior.key.pageUp": "Page Up",
  "behavior.key.pageDown": "Page Down",
  "behavior.key.capsLock": "Caps Lock",
  "behavior.key.printScreen": "Print Screen",
  "behavior.key.scrollLock": "Scroll Lock",
  "behavior.key.pause": "Pause",
  "behavior.key.insert": "Insert",
  "behavior.key.application": "Menu",
  "behavior.key.numLock": "Num Lock",
  "behavior.key.numpad0": "NUMPAD_0",
  "behavior.key.numpad1": "NUMPAD_1",
  "behavior.key.numpad2": "NUMPAD_2",
  "behavior.key.numpad3": "NUMPAD_3",
  "behavior.key.numpad4": "NUMPAD_4",
  "behavior.key.numpad5": "NUMPAD_5",
  "behavior.key.numpad6": "NUMPAD_6",
  "behavior.key.numpad7": "NUMPAD_7",
  "behavior.key.numpad8": "NUMPAD_8",
  "behavior.key.numpad9": "NUMPAD_9",
  "behavior.key.numpadDivide": "Numpad /",
  "behavior.key.numpadMultiply": "Numpad *",
  "behavior.key.numpadSubtract": "Numpad -",
  "behavior.key.numpadAdd": "Numpad +",
  "behavior.key.numpadEnter": "Numpad Enter",
  "behavior.key.numpadDecimal": "Numpad .",
  "behavior.key.numpadEqual": "Numpad =",
```

- [ ] **Step 4: Replace the fixed label map with localized named keys plus literal symbols**

Import the typed i18n API at the top of `src/hotkey.ts`:

```ts
import { t, type Language, type MessageKey } from "./i18n";
```

Replace `HOTKEY_LABELS` with a `Record<string, MessageKey>` containing every named token and alias:

```ts
const HOTKEY_LABEL_KEYS: Record<string, MessageKey> = {
  alt: "behavior.key.optionAlt",
  option: "behavior.key.optionAlt",
  cmd: "behavior.key.cmd",
  ctrl: "behavior.key.ctrl",
  shift: "behavior.key.shift",
  primary: "behavior.key.primary",
  left_cmd: "behavior.key.leftCmd",
  right_cmd: "behavior.key.rightCmd",
  left_ctrl: "behavior.key.leftCtrl",
  right_ctrl: "behavior.key.rightCtrl",
  left_alt: "behavior.key.leftAlt",
  right_alt: "behavior.key.rightAlt",
  left_shift: "behavior.key.leftShift",
  right_shift: "behavior.key.rightShift",
  enter: "behavior.key.enter",
  escape: "behavior.key.escape",
  backspace: "behavior.key.backspace",
  tab: "behavior.key.tab",
  space: "behavior.key.space",
  up: "behavior.key.arrowUp",
  down: "behavior.key.arrowDown",
  left: "behavior.key.arrowLeft",
  right: "behavior.key.arrowRight",
  delete: "behavior.key.delete",
  home: "behavior.key.home",
  end: "behavior.key.end",
  pageup: "behavior.key.pageUp",
  page_up: "behavior.key.pageUp",
  pagedown: "behavior.key.pageDown",
  page_down: "behavior.key.pageDown",
  caps_lock: "behavior.key.capsLock",
  print_screen: "behavior.key.printScreen",
  scroll_lock: "behavior.key.scrollLock",
  pause: "behavior.key.pause",
  insert: "behavior.key.insert",
  application: "behavior.key.application",
  num_lock: "behavior.key.numLock",
  ...Object.fromEntries(Array.from(
    { length: 10 },
    (_, index) => [`numpad_${index}`, `behavior.key.numpad${index}` as MessageKey],
  )),
  numpad_divide: "behavior.key.numpadDivide",
  numpad_multiply: "behavior.key.numpadMultiply",
  numpad_subtract: "behavior.key.numpadSubtract",
  numpad_add: "behavior.key.numpadAdd",
  numpad_enter: "behavior.key.numpadEnter",
  numpad_decimal: "behavior.key.numpadDecimal",
  numpad_equal: "behavior.key.numpadEqual",
};

const HOTKEY_LITERAL_LABELS: Record<string, string> = {
  backtick: "`",
  minus: "-",
  equal: "=",
  left_bracket: "[",
  right_bracket: "]",
  backslash: "\\",
  semicolon: ";",
  quote: "'",
  comma: ",",
  period: ".",
  slash: "/",
};
```

Replace the formatter functions with:

```ts
export function hotkeyDisplayLabel(language: Language, token: string) {
  const normalized = token.toLowerCase();
  const messageKey = HOTKEY_LABEL_KEYS[normalized];
  if (messageKey) return t(language, messageKey);
  return HOTKEY_LITERAL_LABELS[normalized] ?? token.toUpperCase();
}

export function formatHotkey(keys: string[], language: Language) {
  return keys.map((token) => hotkeyDisplayLabel(language, token)).join(" + ");
}
```

- [ ] **Step 5: Run unit and dictionary tests to verify GREEN**

Run: `rtk npm test -- src/hotkey.test.ts src/i18n.test.ts`

Expected: PASS. The dictionary-completeness test proves every new Chinese key also exists in English.

- [ ] **Step 6: Commit the formatter contract**

Run:

```bash
rtk git add src/i18n.ts src/hotkey.ts src/hotkey.test.ts
rtk git commit -m "feat: localize hotkey label formatting"
```

### Task 2: Use localized labels throughout the Hotkey Picker

**Files:**
- Modify: `src/HotkeyPicker.tsx:130-240`
- Test: `src/HotkeyPicker.test.tsx:75-115`

**Interfaces:**
- Consumes: `hotkeyDisplayLabel(language, token)` from Task 1.
- Produces: localized picker text, checkbox accessible names, chip text, remove-button names, and search matching without changing `onChange` values.

- [ ] **Step 1: Replace the old stable-English test with failing Chinese surface tests**

Replace `localizes categories but keeps protocol key names stable` with:

```tsx
test("localizes named keys across picker controls and selected chips", async () => {
  const user = userEvent.setup();
  render(<HotkeyPicker value={["right_cmd", "left"]} onChange={vi.fn()} language="zh-CN" />);

  expect(screen.getByRole("button", { name: "移除 右cmd" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "移除 方向左" })).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "回车" })).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "更多修饰键" }));
  expect(screen.getByRole("checkbox", { name: "右cmd" })).toBeChecked();

  await user.click(screen.getByRole("tab", { name: "导航" }));
  expect(screen.getByRole("checkbox", { name: "方向左" })).toBeChecked();
  expect(screen.getByRole("checkbox", { name: "方向右" })).toBeInTheDocument();
  expect(screen.queryByRole("checkbox", { name: "Arrow Left" })).not.toBeInTheDocument();
});

test("searches keys by their localized display label", async () => {
  const user = userEvent.setup();
  render(<HotkeyPicker value={[]} onChange={vi.fn()} language="zh-CN" />);
  await user.click(screen.getByRole("tab", { name: "导航" }));
  await user.type(screen.getByRole("searchbox", { name: "搜索按键" }), "方向左");
  expect(screen.getByRole("checkbox", { name: "方向左" })).toBeInTheDocument();
  expect(screen.queryByRole("checkbox", { name: "方向右" })).not.toBeInTheDocument();
});
```

Update the existing `shows one category panel` test to expect `回车` and `移除 回车` in Chinese.

- [ ] **Step 2: Run the Picker tests to verify RED**

Run: `rtk npm test -- src/HotkeyPicker.test.tsx`

Expected: FAIL because every `HotkeyPicker` call still omits the language argument.

- [ ] **Step 3: Pass language through every Picker label surface**

In the search filter, call `hotkeyDisplayLabel(language, token)` and add `language` to the `useMemo` dependency list:

```ts
  const filteredTokens = useMemo(() => activeCategory.tokens.filter((token) =>
    !isModifierToken(token) &&
    (!normalizedSearch || hotkeyDisplayLabel(language, token).toLowerCase().includes(normalizedSearch))
  ), [activeCategory, language, normalizedSearch]);
```

Replace every remaining `hotkeyDisplayLabel(token)` in chip text, `behavior.removeKey` parameters, compact modifiers, physical modifiers, and category checkboxes with `hotkeyDisplayLabel(language, token)`. Do not change `toggle`, `hasUsage`, `canonicalHotkeyToken`, or `onChange`.

- [ ] **Step 4: Run the Picker tests to verify GREEN**

Run: `rtk npm test -- src/HotkeyPicker.test.tsx`

Expected: PASS, including the existing English, recording, six-key, and tab/tabpanel tests.

- [ ] **Step 5: Commit localized Picker rendering**

Run:

```bash
rtk git add src/HotkeyPicker.tsx src/HotkeyPicker.test.tsx
rtk git commit -m "feat: show localized hotkey picker labels"
```

### Task 3: Localize Action summaries without changing stored actions

**Files:**
- Modify: `src/ActionEditor.tsx:50-65`
- Modify: `src/ActionEditor.test.tsx:10-70`

**Interfaces:**
- Consumes: `formatHotkey(keys, language)` from Task 1.
- Produces: localized hotkey summary strings while retaining the original `TriggerActions` token arrays.

- [ ] **Step 1: Make the test Harness language-selectable and add a failing Chinese summary test**

Import `Language` beside `TriggerActions`. Replace the current Harness function
signature:

```tsx
function Harness({ initial = emptyGroups(), onChange, language = "en-US" }: {
  initial?: TriggerActions;
  onChange?: (actions: TriggerActions) => void;
  language?: Language;
}) {
```

Inside that Harness, replace the fixed Action editor prop with:

```tsx
        language={language}
```

Keep the existing English summary test and add:

```tsx
test("localizes hotkey summaries without rewriting action tokens", () => {
  render(<Harness language="zh-CN" initial={{
    ...emptyGroups(),
    press: [{ type: "hotkey", keys: ["right_cmd", "left"] }],
  }} />);

  expect(screen.getByText("快捷键 - 右cmd + 方向左")).toBeInTheDocument();
  expect(configuredActions().press).toEqual([
    { type: "hotkey", keys: ["right_cmd", "left"] },
  ]);
});
```

- [ ] **Step 2: Run the Action editor test to verify RED**

Run: `rtk npm test -- src/ActionEditor.test.tsx`

Expected: FAIL because `actionSummary` still calls `formatHotkey(action.keys)` without the active language.

- [ ] **Step 3: Pass the existing Action editor language to the chord formatter**

Change only the hotkey case in `actionSummary`:

```ts
    case "hotkey":
      return `${prefix} - ${action.keys.length ? formatHotkey(action.keys, language) : "-"}`;
```

- [ ] **Step 4: Run Action editor and dialog tests to verify GREEN**

Run: `rtk npm test -- src/ActionEditor.test.tsx src/ActionDialog.test.tsx`

Expected: PASS. The new JSON assertion proves the localized render did not mutate persisted tokens.

- [ ] **Step 5: Commit localized Action summaries**

Run:

```bash
rtk git add src/ActionEditor.tsx src/ActionEditor.test.tsx
rtk git commit -m "feat: localize hotkey action summaries"
```

### Task 4: Full verification and visual acceptance

**Files:**
- Verify only; no planned source changes.

**Interfaces:**
- Consumes: the completed formatter, Picker, and Action-summary behavior from Tasks 1-3.
- Produces: fresh automated and visual evidence for the complete user workflow.

- [ ] **Step 1: Run focused regression tests together**

Run: `rtk npm test -- src/hotkey.test.ts src/i18n.test.ts src/HotkeyPicker.test.tsx src/ActionEditor.test.tsx src/ActionDialog.test.tsx`

Expected: all focused test files pass with zero failures.

- [ ] **Step 2: Run the full frontend suite and production build**

Run: `rtk npm test`

Expected: all Vitest suites pass.

Run: `rtk npm run build`

Expected: TypeScript and Vite complete with exit code 0 and no errors.

- [ ] **Step 3: Check repository hygiene**

Run: `rtk git diff --check`

Expected: no output and exit code 0.

Run: `rtk git status --short --branch`

Expected: only the intended commits relative to `origin/feat/action`, with no uncommitted files.

- [ ] **Step 4: Verify the real Action editing workflow in Chinese**

Start the preview server with `rtk npm run dev -- --host 127.0.0.1`, open the reported local URL with `?preview=1`, switch to Chinese, and open a hotkey Action for editing. Verify the first viewport shows localized selected chips and named keys, including `方向左` and `右cmd`; open the navigation and physical-modifier groups; search for `方向左`; save; then verify the Action summary remains localized. Confirm labels fit without overlap at desktop width and that saved token values remain `left` and `right_cmd` in the existing automated assertion.

Expected: no English named-key label remains in the Chinese workflow, no layout shifts or text overlap occur, and English mode still shows the existing labels.
