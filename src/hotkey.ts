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

const HOTKEY_LABELS: Record<string, string> = {
  alt: "Option / Alt",
  option: "Option / Alt",
  cmd: "Command",
  ctrl: "Control",
  shift: "Shift",
  primary: "Primary (Command / Control)",
  left_cmd: "Left Command",
  right_cmd: "Right Command",
  left_ctrl: "Left Control",
  right_ctrl: "Right Control",
  left_alt: "Left Option / Alt",
  right_alt: "Right Option / Alt",
  left_shift: "Left Shift",
  right_shift: "Right Shift",
  enter: "Enter",
  escape: "Escape",
  backspace: "Backspace",
  tab: "Tab",
  space: "Space",
  up: "Arrow Up",
  down: "Arrow Down",
  left: "Arrow Left",
  right: "Arrow Right",
  delete: "Delete",
  home: "Home",
  end: "End",
  pageup: "Page Up",
  page_up: "Page Up",
  pagedown: "Page Down",
  page_down: "Page Down",
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
  caps_lock: "Caps Lock",
  print_screen: "Print Screen",
  scroll_lock: "Scroll Lock",
  pause: "Pause",
  insert: "Insert",
  application: "Menu",
  num_lock: "Num Lock",
  numpad_divide: "Numpad /",
  numpad_multiply: "Numpad *",
  numpad_subtract: "Numpad -",
  numpad_add: "Numpad +",
  numpad_enter: "Numpad Enter",
  numpad_decimal: "Numpad .",
  numpad_equal: "Numpad =",
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

export function hotkeyDisplayLabel(token: string) {
  return HOTKEY_LABELS[token.toLowerCase()] ?? token.toUpperCase();
}

export function formatHotkey(keys: string[]) {
  return keys.map(hotkeyDisplayLabel).join(" + ");
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
