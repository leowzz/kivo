import { expect, test } from "vitest";
import {
  HOTKEY_CATEGORIES,
  formatHotkey,
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

test("formats the backtick key as its symbol", () => {
  expect(formatHotkey(["backtick"])).toBe("`");
});

test("formats sided modifiers using their canonical label", () => {
  expect(formatHotkey(["left_cmd"])).toBe("Left Command");
});

test("formats protocol modifiers with the same technical labels as the picker", () => {
  expect(formatHotkey(["primary", "alt", "left_alt", "right_alt"])).toBe(
    "Primary (Command / Control) + Option / Alt + Left Option / Alt + Right Option / Alt",
  );
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
