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

const MODIFIER_CODES = new Set([
  "MetaLeft", "MetaRight", "ControlLeft", "ControlRight",
  "AltLeft", "AltRight", "ShiftLeft", "ShiftRight",
]);

const HOTKEY_LABELS: Record<string, string> = {
  alt: "Option",
  option: "Option",
  cmd: "Command",
  ctrl: "Control",
  shift: "Shift",
  primary: "Cmd/Ctrl",
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

export function formatHotkey(keys: string[]) {
  return keys
    .map((key) => HOTKEY_LABELS[key.toLowerCase()] ?? key.toUpperCase())
    .join(" + ");
}

export function normalizeHotkey(event: KeyboardEvent): string[] | null {
  if (MODIFIER_CODES.has(event.code)) return null;

  let key: string | undefined;
  if (/^Key[A-Z]$/.test(event.code)) key = event.code.slice(3).toLowerCase();
  else if (/^Digit[0-9]$/.test(event.code)) key = event.code.slice(5);
  else if (/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(event.code)) key = event.code.toLowerCase();
  else if (/^Numpad[0-9]$/.test(event.code)) key = `numpad_${event.code.slice(6)}`;
  else key = NAMED_KEYS[event.code];
  if (!key) throw new Error(`Unsupported shortcut key: ${event.code}`);

  const keys: string[] = [];
  if (event.metaKey) keys.push("cmd");
  if (event.ctrlKey) keys.push("ctrl");
  if (event.altKey) keys.push("alt");
  if (event.shiftKey) keys.push("shift");
  keys.push(key);
  return keys;
}
