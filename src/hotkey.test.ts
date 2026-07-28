import { expect, test } from "vitest";
import { normalizeHotkey } from "./hotkey";

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

test("rejects unsupported keys", () => {
  expect(() => normalizeHotkey({
    code: "NumpadAdd",
    metaKey: false,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
  } as KeyboardEvent)).toThrow("Unsupported shortcut key");
});
