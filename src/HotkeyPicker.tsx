import { ChevronDown, Keyboard, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import {
  HOTKEY_CATEGORIES,
  canonicalHotkeyToken,
  hotkeyDisplayLabel,
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

function hasUsage(value: string[], token: string) {
  const usage = canonicalHotkeyToken(token);
  return value.some((item) => canonicalHotkeyToken(item) === usage);
}

function ordinaryCount(value: string[]) {
  return value.filter((token) => !isModifierToken(token)).length;
}

export function HotkeyPicker({ value, onChange, language, error, onRecordingChange }: HotkeyPickerProps) {
  const [search, setSearch] = useState("");
  const [activeCategoryName, setActiveCategoryName] = useState<(typeof HOTKEY_CATEGORIES)[number]["name"]>(HOTKEY_CATEGORIES[0].name);
  const [showPhysicalModifiers, setShowPhysicalModifiers] = useState(false);
  const [recording, setRecording] = useState(false);
  const [recordingError, setRecordingError] = useState<string | null>(null);
  const onChangeRef = useRef(onChange);
  const categoryTabRefs = useRef<Array<HTMLButtonElement | null>>([]);
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

  const activeCategory = HOTKEY_CATEGORIES.find(({ name }) => name === activeCategoryName) ?? HOTKEY_CATEGORIES[0];
  const filteredTokens = useMemo(() => activeCategory.tokens.filter((token) =>
    !isModifierToken(token) &&
    (!normalizedSearch || hotkeyDisplayLabel(language, token).toLowerCase().includes(normalizedSearch))
  ), [activeCategory, language, normalizedSearch]);

  const handleCategoryKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>, index: number) => {
    let nextIndex: number | null = null;
    if (event.key === "ArrowRight") nextIndex = (index + 1) % HOTKEY_CATEGORIES.length;
    if (event.key === "ArrowLeft") nextIndex = (index - 1 + HOTKEY_CATEGORIES.length) % HOTKEY_CATEGORIES.length;
    if (event.key === "Home") nextIndex = 0;
    if (event.key === "End") nextIndex = HOTKEY_CATEGORIES.length - 1;
    if (nextIndex === null) return;
    event.preventDefault();
    setActiveCategoryName(HOTKEY_CATEGORIES[nextIndex].name);
    categoryTabRefs.current[nextIndex]?.focus();
  };

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
          <button className="hotkey-chip" type="button" key={token} aria-label={t(language, "behavior.removeKey", { key: hotkeyDisplayLabel(language, token) })} onClick={() => toggle(token)}>
            {hotkeyDisplayLabel(language, token)} <X size={12} aria-hidden="true" />
          </button>
        ))}
      </div>
      <button type="button" className={recording ? "record-button is-recording" : "record-button"} aria-label={t(language, recording ? "behavior.stopRecording" : "behavior.recordShortcut")} onClick={startRecording}>
        <Keyboard size={16} aria-hidden="true" />{t(language, recording ? "behavior.stopRecording" : "behavior.recordShortcut")}
      </button>
      <label className="hotkey-search">
        <span>{t(language, "behavior.searchKeys")}</span>
        <input type="search" role="searchbox" aria-label={t(language, "behavior.searchKeys")} value={search} onChange={(event) => setSearch(event.target.value)} />
      </label>
      <div className="hotkey-modifier-row">
        {COMPACT_MODIFIERS.map(({ token }) => (
          <label key={token}>
            <input type="checkbox" checked={hasUsage(value, token)} onChange={() => toggle(token)} aria-label={hotkeyDisplayLabel(language, token)} />
            <span>{hotkeyDisplayLabel(language, token)}</span>
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
              <input type="checkbox" checked={hasUsage(value, token)} onChange={() => toggle(token)} aria-label={hotkeyDisplayLabel(language, token)} />
              <span>{hotkeyDisplayLabel(language, token)}</span>
            </label>
          ))}
        </div>
      )}
      <div className="hotkey-category-tabs" role="tablist" aria-label={t(language, "behavior.keyCategories")}>
        {HOTKEY_CATEGORIES.map((category, index) => {
          const selected = category.name === activeCategory.name;
          return (
            <button
              key={category.name}
              ref={(element) => { categoryTabRefs.current[index] = element; }}
              id={`hotkey-category-tab-${index}`}
              type="button"
              role="tab"
              tabIndex={selected ? 0 : -1}
              aria-selected={selected}
              aria-controls={selected ? `hotkey-category-panel-${index}` : undefined}
              onClick={() => setActiveCategoryName(category.name)}
              onKeyDown={(event) => handleCategoryKeyDown(event, index)}
            >
              {t(language, CATEGORY_LABELS[category.name])}
            </button>
          );
        })}
      </div>
      <div
        className="hotkey-category-panel"
        id={`hotkey-category-panel-${HOTKEY_CATEGORIES.indexOf(activeCategory)}`}
        role="tabpanel"
        aria-labelledby={`hotkey-category-tab-${HOTKEY_CATEGORIES.indexOf(activeCategory)}`}
      >
        <div className="hotkey-key-grid">
          {filteredTokens.map((token) => {
            const selected = hasUsage(value, token);
            const disabled = !selected && ordinaryCount(value) >= 6;
            return (
              <label key={token} className={disabled ? "is-disabled" : undefined}>
                <input type="checkbox" checked={selected} disabled={disabled} onChange={() => toggle(token)} aria-label={hotkeyDisplayLabel(language, token)} />
                <span>{hotkeyDisplayLabel(language, token)}</span>
              </label>
            );
          })}
        </div>
      </div>
      {(error || recordingError) && <small className="field-error">{hotkeyValidationMessage(language, recordingError ?? error)}</small>}
    </div>
  );
}
