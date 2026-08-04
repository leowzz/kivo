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
