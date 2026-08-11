import { t, type Language, type MessageKey } from "./i18n";

const NAMED_KEYS: Record<string, string> = {
  Enter: "enter",
  Escape: "escape",
  Backspace: "backspace",
  Tab: "tab",
  Space: "space",
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  Delete: "delete",
  Home: "home",
  End: "end",
  PageUp: "page_up",
  PageDown: "page_down",
  Backquote: "backtick",
  Minus: "minus",
  Equal: "equal",
  BracketLeft: "left_bracket",
  BracketRight: "right_bracket",
  Backslash: "backslash",
  Semicolon: "semicolon",
  Quote: "quote",
  Comma: "comma",
  Period: "period",
  Slash: "slash",
  CapsLock: "caps_lock",
  PrintScreen: "print_screen",
  ScrollLock: "scroll_lock",
  Pause: "pause",
  Insert: "insert",
  ContextMenu: "application",
  NumLock: "num_lock",
  NumpadDivide: "numpad_divide",
  NumpadMultiply: "numpad_multiply",
  NumpadSubtract: "numpad_subtract",
  NumpadAdd: "numpad_add",
  NumpadEnter: "numpad_enter",
  NumpadDecimal: "numpad_decimal",
  NumpadEqual: "numpad_equal",
};

const MODIFIER_TOKENS = new Set([
  "primary", "cmd", "ctrl", "alt", "option", "shift",
  "left_cmd", "right_cmd", "left_ctrl", "right_ctrl",
  "left_alt", "right_alt", "left_shift", "right_shift",
]);

const MODIFIER_CODES: Record<string, string> = {
  MetaLeft: "left_cmd",
  MetaRight: "right_cmd",
  ControlLeft: "left_ctrl",
  ControlRight: "right_ctrl",
  AltLeft: "left_alt",
  AltRight: "right_alt",
  ShiftLeft: "left_shift",
  ShiftRight: "right_shift",
};

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

export const HOTKEY_CATEGORIES = [
  {
    name: "Common",
    tokens: [
      "primary", "cmd", "ctrl", "alt", "option", "shift",
      "left_cmd", "right_cmd", "left_ctrl", "right_ctrl",
      "left_alt", "right_alt", "left_shift", "right_shift",
      "enter", "escape", "backspace", "tab", "space", "caps_lock",
    ],
  },
  { name: "Function Keys F1-F24", tokens: Array.from({ length: 24 }, (_, index) => `f${index + 1}`) },
  { name: "Letters", tokens: [..."abcdefghijklmnopqrstuvwxyz"] },
  { name: "Numbers", tokens: [..."0123456789"] },
  {
    name: "Symbols",
    tokens: [
      "backtick", "minus", "equal", "left_bracket", "right_bracket", "backslash",
      "semicolon", "quote", "comma", "period", "slash",
    ],
  },
  {
    name: "Navigation",
    tokens: [
      "insert", "delete", "home", "end", "page_up", "page_down", "up", "down", "left", "right",
      "print_screen", "scroll_lock", "pause", "application",
    ],
  },
  {
    name: "Numeric Keypad",
    tokens: [
      "num_lock", ...Array.from({ length: 10 }, (_, index) => `numpad_${index}`),
      "numpad_decimal", "numpad_divide", "numpad_multiply", "numpad_subtract",
      "numpad_add", "numpad_enter", "numpad_equal",
    ],
  },
] as const;

const ORDINARY_TOKENS = new Set(HOTKEY_CATEGORIES.flatMap((category) => category.tokens)
  .filter((token) => !MODIFIER_TOKENS.has(token)));

function canonicalOrdinaryToken(token: string) {
  if (token === "pageup") return "page_up";
  if (token === "pagedown") return "page_down";
  return token;
}

/** Returns the canonical usage identity used by the picker for selection state. */
export function canonicalHotkeyToken(token: string) {
  const normalized = token.toLowerCase();
  switch (normalized) {
    case "primary":
      return navigator.platform.includes("Mac") ? "left_cmd" : "left_ctrl";
    case "cmd":
      return "left_cmd";
    case "ctrl":
      return "left_ctrl";
    case "alt":
    case "option":
      return "left_alt";
    case "shift":
      return "left_shift";
    default:
      return canonicalOrdinaryToken(normalized);
  }
}

function modifierBit(token: string): number | null {
  switch (token) {
    case "primary": return navigator.platform.includes("Mac") ? 0x08 : 0x01;
    case "cmd":
    case "left_cmd": return 0x08;
    case "right_cmd": return 0x80;
    case "ctrl":
    case "left_ctrl": return 0x01;
    case "right_ctrl": return 0x10;
    case "shift":
    case "left_shift": return 0x02;
    case "right_shift": return 0x20;
    case "alt":
    case "option":
    case "left_alt": return 0x04;
    case "right_alt": return 0x40;
    default: return null;
  }
}

export function keyboardCodeToToken(code: string): string | null {
  if (MODIFIER_CODES[code]) return MODIFIER_CODES[code];
  if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(code)) return code.toLowerCase();
  if (/^Numpad[0-9]$/.test(code)) return `numpad_${code.slice(6)}`;
  return NAMED_KEYS[code] ?? null;
}

export function isModifierToken(token: string) {
  return MODIFIER_TOKENS.has(token.toLowerCase());
}

export function validateHotkey(keys: string[]): "empty_hotkey" | "duplicate_key" | "too_many_keys" | "unsupported_key" | null {
  if (keys.length === 0) return "empty_hotkey";

  const modifiers = new Set<number>();
  const ordinary = new Set<string>();
  for (const key of keys) {
    const token = key.toLowerCase();
    const bit = modifierBit(token);
    if (bit !== null) {
      if (modifiers.has(bit)) return "duplicate_key";
      modifiers.add(bit);
      continue;
    }
    const ordinaryToken = canonicalOrdinaryToken(token);
    if (!ORDINARY_TOKENS.has(ordinaryToken)) return "unsupported_key";
    if (ordinary.has(ordinaryToken)) return "duplicate_key";
    ordinary.add(ordinaryToken);
  }
  return ordinary.size > 6 ? "too_many_keys" : null;
}

export function hotkeyDisplayLabel(language: Language, token: string) {
  const normalized = token.toLowerCase();
  const messageKey = HOTKEY_LABEL_KEYS[normalized];
  if (messageKey) return t(language, messageKey);
  return HOTKEY_LITERAL_LABELS[normalized] ?? token.toUpperCase();
}

export function formatHotkey(keys: string[], language: Language) {
  return keys.map((token) => hotkeyDisplayLabel(language, token)).join(" + ");
}

export function normalizeHotkey(event: KeyboardEvent): string[] | null {
  const key = keyboardCodeToToken(event.code);
  if (key && isModifierToken(key)) return null;
  if (!key) throw new Error(`Unsupported shortcut key: ${event.code}`);

  const keys: string[] = [];
  if (event.metaKey) keys.push("cmd");
  if (event.ctrlKey) keys.push("ctrl");
  if (event.altKey) keys.push("alt");
  if (event.shiftKey) keys.push("shift");
  keys.push(key);
  return keys;
}
