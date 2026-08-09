import { expect, test } from "vitest";
import { formatHotkey, normalizeHotkey } from "./hotkey";

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
