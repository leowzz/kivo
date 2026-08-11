import { expect, test } from "vitest";
import {
  HOTKEY_CATEGORIES,
  formatHotkey,
  hotkeyDisplayLabel,
  keyboardCodeToToken,
  normalizeHotkey,
  validateHotkey,
} from "./hotkey";

test("normalizes command shift letter", () => {
  expect(normalizeHotkey({
    code: "KeyK",
    metaKey: true,
    shiftKey: true,
    ctrlKey: false,
    altKey: false,
  } as KeyboardEvent)).toEqual(["cmd", "shift", "k"]);
});

test("waits when only a modifier is pressed", () => {
  expect(normalizeHotkey({
    code: "MetaLeft",
    metaKey: true,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
  } as KeyboardEvent)).toBeNull();
});

test("uses backend key names", () => {
  expect(normalizeHotkey({
    code: "ArrowUp",
    metaKey: false,
    shiftKey: false,
    ctrlKey: false,
    altKey: true,
  } as KeyboardEvent)).toEqual(["alt", "up"]);
});

test("normalizes the backtick key", () => {
  expect(normalizeHotkey({
    code: "Backquote",
    metaKey: false,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
  } as KeyboardEvent)).toEqual(["backtick"]);
});

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

test("maps physical modifier codes without collapsing sides", () => {
  expect(keyboardCodeToToken("MetaLeft")).toBe("left_cmd");
  expect(keyboardCodeToToken("MetaRight")).toBe("right_cmd");
  expect(keyboardCodeToToken("AltRight")).toBe("right_alt");
});

test("validates modifier-only and six-key chords", () => {
  expect(validateHotkey(["right_cmd"])).toBeNull();
  expect(validateHotkey(["a", "b", "c", "d", "e", "f"])).toBeNull();
  expect(validateHotkey(["a", "b", "c", "d", "e", "f", "g"])).toBe("too_many_keys");
});

test("returns actionable errors for empty, duplicate, and unsupported hotkeys", () => {
  expect(validateHotkey([])).toBe("empty_hotkey");
  expect(validateHotkey(["cmd", "left_cmd"])).toBe("duplicate_key");
  expect(validateHotkey(["fn"])).toBe("unsupported_key");
  expect(validateHotkey(["pageup"])).toBeNull();
  expect(validateHotkey(["pageup", "page_up"])).toBe("duplicate_key");
});

test("exports the canonical key categories without laptop Fn", () => {
  expect(HOTKEY_CATEGORIES.map((category) => category.name)).toEqual([
    "Common",
    "Function Keys F1-F24",
    "Letters",
    "Numbers",
    "Symbols",
    "Navigation",
    "Numeric Keypad",
  ]);
  expect(HOTKEY_CATEGORIES.flatMap((category) => category.tokens)).not.toContain("fn");
});

test("normalizes function, punctuation, and numpad keys", () => {
  const event = (code: string) => ({
    code,
    metaKey: false,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
  } as KeyboardEvent);

  expect(normalizeHotkey(event("F24"))).toEqual(["f24"]);
  expect(normalizeHotkey(event("BracketLeft"))).toEqual(["left_bracket"]);
  expect(normalizeHotkey(event("NumpadAdd"))).toEqual(["numpad_add"]);
  expect(normalizeHotkey(event("Numpad0"))).toEqual(["numpad_0"]);
});

test("rejects unsupported keys", () => {
  expect(() => normalizeHotkey({
    code: "IntlRo",
    metaKey: false,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
  } as KeyboardEvent)).toThrow("Unsupported shortcut key");
});
