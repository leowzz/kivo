import { ChevronDown, Keyboard, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  HOTKEY_CATEGORIES,
  canonicalHotkeyToken,
  formatHotkey,
  isModifierToken,
  keyboardCodeToToken,
  validateHotkey,
} from "./hotkey";
import { t, type MessageKey } from "./i18n";
import type { Language } from "./types";

export interface HotkeyPickerProps {
  value: string[];
  onChange(value: string[]): void;
  language: Language;
  error?: string | null;
  onRecordingChange?(recording: boolean): void;
}

export function hotkeyValidationMessage(language: Language, error: string | null | undefined) {
  switch (error) {
    case "empty_hotkey": return t(language, "behavior.hotkeyEmpty");
    case "duplicate_key": return t(language, "behavior.hotkeyDuplicate");
    case "unsupported_key": return t(language, "behavior.hotkeyUnsupported");
    case "too_many_keys": return t(language, "behavior.hotkeyTooMany");
    default: return error ?? null;
  }
}

const COMPACT_MODIFIERS = [
  { token: "primary" }, { token: "cmd" }, { token: "ctrl" }, { token: "alt" }, { token: "shift" },
] as const;

const PHYSICAL_MODIFIERS = [
  "left_cmd", "right_cmd", "left_ctrl", "right_ctrl", "left_alt", "right_alt", "left_shift", "right_shift",
] as const;

const CATEGORY_LABELS: Record<string, MessageKey> = {
  Common: "behavior.keyCategory.common",
  "Function Keys F1-F24": "behavior.keyCategory.function",
  Letters: "behavior.keyCategory.letters",
  Numbers: "behavior.keyCategory.numbers",
  Symbols: "behavior.keyCategory.symbols",
  Navigation: "behavior.keyCategory.navigation",
  "Numeric Keypad": "behavior.keyCategory.numpad",
};

const KEY_LABELS: Record<string, MessageKey> = {
  primary: "behavior.key.primary", cmd: "behavior.key.command", ctrl: "behavior.key.control", alt: "behavior.key.option", option: "behavior.key.option", shift: "behavior.key.shift",
  left_cmd: "behavior.key.leftCommand", right_cmd: "behavior.key.rightCommand", left_ctrl: "behavior.key.leftControl", right_ctrl: "behavior.key.rightControl",
  left_alt: "behavior.key.leftOption", right_alt: "behavior.key.rightOption", left_shift: "behavior.key.leftShift", right_shift: "behavior.key.rightShift",
};

function displayLabel(language: Language, token: string) {
  const label = KEY_LABELS[token];
  if (label) return t(language, label);
  if (language === "zh-CN") {
    const labels: Record<string, string> = { enter: "回车", escape: "Esc", backspace: "退格", tab: "Tab", space: "空格", up: "上", down: "下", left: "左", right: "右", delete: "删除", home: "首页", end: "末尾", page_up: "上页", page_down: "下页", caps_lock: "大写锁定", insert: "插入", application: "菜单", num_lock: "数字锁定" };
    return labels[token] ?? formatHotkey([token]);
  }
  return formatHotkey([token]);
}

function hasUsage(value: string[], token: string) {
  const usage = canonicalHotkeyToken(token);
  return value.some((item) => canonicalHotkeyToken(item) === usage);
}

function ordinaryCount(value: string[]) {
  return value.filter((token) => !isModifierToken(token)).length;
}

export function HotkeyPicker({ value, onChange, language, error, onRecordingChange }: HotkeyPickerProps) {
  const [search, setSearch] = useState("");
  const [showPhysicalModifiers, setShowPhysicalModifiers] = useState(false);
  const [recording, setRecording] = useState(false);
  const [recordingError, setRecordingError] = useState<string | null>(null);
  const onChangeRef = useRef(onChange);
  const normalizedSearch = search.trim().toLowerCase();

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    onRecordingChange?.(recording);
    return () => {
      if (recording) onRecordingChange?.(false);
    };
  }, [onRecordingChange, recording]);

  useEffect(() => {
    if (!recording) return;
    const captured = new Map<string, string>();
    const handleDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (captured.has(event.code)) return;
      const token = keyboardCodeToToken(event.code);
      if (!token) {
        setRecordingError("unsupported_key");
        setRecording(false);
        return;
      }
      captured.set(event.code, token);
    };
    const handleUp = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (!captured.has(event.code)) return;
      captured.delete(event.code);
      if (captured.size > 0) return;
      const keys = Array.from(lastCaptured.values());
      const validation = validateHotkey(keys);
      if (validation) {
        setRecordingError(validation);
        setRecording(false);
      } else {
        onChangeRef.current(keys);
        setRecordingError(null);
        setRecording(false);
      }
    };
    // Keep a second map so keyup can wait for all keys while retaining insertion order.
    const lastCaptured = new Map<string, string>();
    const down = (event: KeyboardEvent) => {
      const before = captured.size;
      handleDown(event);
      if (captured.size > before) lastCaptured.set(event.code, captured.get(event.code)!);
    };
    const up = (event: KeyboardEvent) => {
      handleUp(event);
    };
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", up, true);
    return () => {
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", up, true);
    };
  }, [recording]);

  const filteredCategories = useMemo(() => HOTKEY_CATEGORIES.map((category) => ({
    ...category,
    tokens: category.tokens.filter((token) => !isModifierToken(token) && (!normalizedSearch || `${displayLabel(language, token)} ${t(language, CATEGORY_LABELS[category.name])}`.toLowerCase().includes(normalizedSearch))),
  })).filter((category) => category.tokens.length > 0 || !normalizedSearch), [language, normalizedSearch]);

  const toggle = (token: string) => {
    const index = value.findIndex((item) => canonicalHotkeyToken(item) === canonicalHotkeyToken(token));
    if (index >= 0) {
      onChange(value.filter((_, itemIndex) => itemIndex !== index));
      return;
    }
    if (!isModifierToken(token) && ordinaryCount(value) >= 6) return;
    onChange([...value, token]);
  };

  const startRecording = () => {
    setRecordingError(null);
    setRecording((active) => !active);
  };

  return (
    <div className="hotkey-picker">
      <div className="hotkey-chips" aria-label={t(language, "behavior.shortcut")}>
        {value.length === 0 && <span className="hotkey-empty">-</span>}
        {value.map((token) => (
          <button className="hotkey-chip" type="button" key={token} aria-label={t(language, "behavior.removeKey", { key: displayLabel(language, token) })} onClick={() => toggle(token)}>
            {displayLabel(language, token)} <X size={12} aria-hidden="true" />
          </button>
        ))}
      </div>
      <label className="hotkey-search">
        <span>{t(language, "behavior.searchKeys")}</span>
        <input type="search" role="searchbox" aria-label={t(language, "behavior.searchKeys")} value={search} onChange={(event) => setSearch(event.target.value)} />
      </label>
      <div className="hotkey-modifier-row">
        {COMPACT_MODIFIERS.map(({ token }) => (
          <label key={token}>
            <input type="checkbox" checked={hasUsage(value, token)} onChange={() => toggle(token)} aria-label={displayLabel(language, token)} />
            <span>{displayLabel(language, token)}</span>
          </label>
        ))}
        <button type="button" className="hotkey-disclosure" aria-expanded={showPhysicalModifiers} onClick={() => setShowPhysicalModifiers((open) => !open)}>
          <ChevronDown size={14} aria-hidden="true" />{t(language, "behavior.moreModifiers")}
        </button>
      </div>
      {showPhysicalModifiers && (
        <div className="hotkey-physical-modifiers">
          {PHYSICAL_MODIFIERS.map((token) => (
            <label key={token}>
              <input type="checkbox" checked={hasUsage(value, token)} onChange={() => toggle(token)} aria-label={displayLabel(language, token)} />
              <span>{displayLabel(language, token)}</span>
            </label>
          ))}
        </div>
      )}
      <div className="hotkey-category-list">
        {filteredCategories.map((category) => (
          <fieldset key={category.name} className="hotkey-category">
            <legend>{t(language, CATEGORY_LABELS[category.name])}</legend>
            <div className="hotkey-key-grid">
              {category.tokens.map((token) => {
                const selected = hasUsage(value, token);
                const disabled = !selected && !isModifierToken(token) && ordinaryCount(value) >= 6;
                return (
                  <label key={token} className={disabled ? "is-disabled" : undefined}>
                    <input type="checkbox" checked={selected} disabled={disabled} onChange={() => toggle(token)} aria-label={displayLabel(language, token)} />
                    <span>{displayLabel(language, token)}</span>
                  </label>
                );
              })}
            </div>
          </fieldset>
        ))}
      </div>
      <button type="button" className={recording ? "record-button is-recording" : "record-button"} aria-label={t(language, recording ? "behavior.stopRecording" : "behavior.recordShortcut")} onClick={startRecording}>
        <Keyboard size={16} aria-hidden="true" />{t(language, recording ? "behavior.stopRecording" : "behavior.recordShortcut")}
      </button>
      {(error || recordingError) && <small className="field-error">{hotkeyValidationMessage(language, recordingError ?? error)}</small>}
    </div>
  );
}
