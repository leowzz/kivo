import {
  ArrowDown,
  ArrowUp,
  AudioLines,
  Clock3,
  ExternalLink,
  Keyboard,
  Plus,
  TextCursorInput,
  Trash2,
} from "lucide-react";
import { useEffect, useState } from "react";
import { formatHotkey, normalizeHotkey } from "./hotkey";
import { t, type MessageKey } from "./i18n";
import type { ButtonAction, Language, MediaCommand, ModelButton } from "./types";

interface ActionEditorProps {
  language: Language;
  button: ModelButton | null;
  actions: ButtonAction[];
  onChange(actions: ButtonAction[]): void;
}

const HOTKEY_MODIFIERS = [
  { value: "primary", label: "Cmd/Ctrl" },
  { value: "cmd", label: "Cmd" },
  { value: "ctrl", label: "Ctrl" },
  { value: "alt", label: "Option" },
  { value: "shift", label: "Shift" },
] as const;

const HOTKEY_MODIFIER_VALUES = new Set<string>(HOTKEY_MODIFIERS.map(({ value }) => value));

const HOTKEY_KEY_GROUPS: Array<{ label: MessageKey; keys: string[] }> = [
  {
    label: "behavior.keyGroup.basic",
    keys: [..."abcdefghijklmnopqrstuvwxyz", ..."0123456789"],
  },
  {
    label: "behavior.keyGroup.function",
    keys: Array.from({ length: 24 }, (_, index) => `f${index + 1}`),
  },
  {
    label: "behavior.keyGroup.editing",
    keys: [
      "enter", "escape", "backspace", "tab", "space", "insert", "delete",
      "up", "down", "left", "right", "home", "end", "page_up", "page_down",
      "print_screen", "scroll_lock", "pause", "caps_lock", "application",
    ],
  },
  {
    label: "behavior.keyGroup.punctuation",
    keys: [
      "backtick", "minus", "equal", "left_bracket", "right_bracket", "backslash",
      "semicolon", "quote", "comma", "period", "slash",
    ],
  },
  {
    label: "behavior.keyGroup.numpad",
    keys: [
      "num_lock", ...Array.from({ length: 10 }, (_, index) => `numpad_${index}`),
      "numpad_decimal", "numpad_divide", "numpad_multiply", "numpad_subtract",
      "numpad_add", "numpad_enter", "numpad_equal",
    ],
  },
];

const MEDIA_COMMANDS: Array<{ value: MediaCommand; label: MessageKey }> = [
  { value: "play_pause", label: "behavior.media.playPause" },
  { value: "previous_track", label: "behavior.media.previousTrack" },
  { value: "next_track", label: "behavior.media.nextTrack" },
  { value: "stop", label: "behavior.media.stop" },
  { value: "volume_up", label: "behavior.media.volumeUp" },
  { value: "volume_down", label: "behavior.media.volumeDown" },
  { value: "mute", label: "behavior.media.mute" },
];

function ordinaryHotkeyKey(keys: string[]) {
  return keys.find((key) => !HOTKEY_MODIFIER_VALUES.has(key)) ?? "enter";
}

function hotkeyModifiers(keys: string[]) {
  return HOTKEY_MODIFIERS.filter(({ value }) => keys.includes(value)).map(({ value }) => value);
}

function setHotkeyModifier(keys: string[], modifier: string, enabled: boolean) {
  const selected = new Set(hotkeyModifiers(keys));
  if (enabled) {
    if (modifier === "primary") {
      selected.delete("cmd");
      selected.delete("ctrl");
    } else if (modifier === "cmd" || modifier === "ctrl") {
      selected.delete("primary");
    }
    selected.add(modifier as (typeof HOTKEY_MODIFIERS)[number]["value"]);
  } else {
    selected.delete(modifier as (typeof HOTKEY_MODIFIERS)[number]["value"]);
  }
  return [
    ...HOTKEY_MODIFIERS
      .filter(({ value }) => selected.has(value))
      .map(({ value }) => value),
    ordinaryHotkeyKey(keys),
  ];
}

function move(actions: ButtonAction[], index: number, offset: -1 | 1) {
  const next = [...actions];
  [next[index], next[index + offset]] = [next[index + offset], next[index]];
  return next;
}

function actionPresentation(type: ButtonAction["type"]): {
  icon: typeof Keyboard;
  label: MessageKey;
} {
  switch (type) {
    case "paste": return { icon: TextCursorInput, label: "behavior.paste" };
    case "hotkey": return { icon: Keyboard, label: "behavior.hotkey" };
    case "delay": return { icon: Clock3, label: "behavior.delay" };
    case "media": return { icon: AudioLines, label: "behavior.media" };
    case "open": return { icon: ExternalLink, label: "behavior.open" };
  }
}

export function ActionEditor({ language, button, actions, onChange }: ActionEditorProps) {
  const [recording, setRecording] = useState<number | null>(null);

  useEffect(() => {
    if (recording === null) return;
    const handler = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      try {
        const keys = normalizeHotkey(event);
        if (!keys) return;
        onChange(actions.map((item, index) => index === recording
          ? { type: "hotkey", keys }
          : item));
        setRecording(null);
      } catch {
        // Keep recording until a supported key is pressed.
      }
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [actions, onChange, recording]);

  if (!button) {
    return (
      <aside className="action-panel" aria-labelledby="action-title">
        <h2 id="action-title">{t(language, "behavior.title")}</h2>
        <div className="panel-empty">{t(language, "behavior.empty")}</div>
      </aside>
    );
  }

  const replace = (index: number, action: ButtonAction) => {
    onChange(actions.map((item, itemIndex) => itemIndex === index ? action : item));
  };

  return (
    <aside className="action-panel" aria-labelledby="action-title">
      <div className="panel-title">
        <div>
          <span>{t(language, "behavior.title")}</span>
          <h2 id="action-title">{button.label}</h2>
        </div>
        <strong>{actions.length}</strong>
      </div>

      <div className="action-list">
        {actions.length === 0 && (
          <div className="panel-empty">{t(language, "behavior.noActions")}</div>
        )}
        {actions.map((action, index) => {
          const presentation = actionPresentation(action.type);
          const TypeIcon = presentation.icon;
          return (
            <section className="action-item" key={`${action.type}-${index}`}>
              <div className="action-item-header">
                <span><TypeIcon size={13} aria-hidden="true" />{index + 1}. {t(language, presentation.label)}</span>
                <div className="icon-row">
                  <button
                    className="icon-button"
                    type="button"
                    aria-label={t(language, "behavior.moveUp")}
                    title={t(language, "behavior.moveUp")}
                    disabled={index === 0}
                    onClick={() => onChange(move(actions, index, -1))}
                  >
                    <ArrowUp size={16} />
                  </button>
                  <button
                    className="icon-button"
                    type="button"
                    aria-label={t(language, "behavior.moveDown")}
                    title={t(language, "behavior.moveDown")}
                    disabled={index === actions.length - 1}
                    onClick={() => onChange(move(actions, index, 1))}
                  >
                    <ArrowDown size={16} />
                  </button>
                  <button
                    className="icon-button is-danger"
                    type="button"
                    aria-label={t(language, "behavior.remove")}
                    title={t(language, "behavior.remove")}
                    onClick={() => onChange(actions.filter((_, itemIndex) => itemIndex !== index))}
                  >
                    <Trash2 size={16} />
                  </button>
                </div>
              </div>

              {action.type === "paste" && (
                <label className="field-stack">
                  <span>{t(language, "behavior.text")}</span>
                  <textarea
                    aria-label={t(language, "behavior.text")}
                    value={action.text}
                    rows={4}
                    onChange={(event) => replace(index, { type: "paste", text: event.target.value })}
                  />
                  {!action.text && <small className="field-error">{t(language, "behavior.textRequired")}</small>}
                </label>
              )}

              {action.type === "hotkey" && (
                <div className="hotkey-field">
                  <span>{t(language, "behavior.shortcut")}</span>
                  <output>{action.keys.length ? formatHotkey(action.keys) : "-"}</output>
                  <div className="hotkey-manual">
                    <div className="hotkey-modifiers">
                      {HOTKEY_MODIFIERS.map((modifier) => (
                        <label key={modifier.value}>
                          <input
                            type="checkbox"
                            checked={action.keys.includes(modifier.value)}
                            onChange={(event) => replace(index, {
                              type: "hotkey",
                              keys: setHotkeyModifier(action.keys, modifier.value, event.target.checked),
                            })}
                          />
                          <span>{modifier.label}</span>
                        </label>
                      ))}
                    </div>
                    <select
                      aria-label={t(language, "behavior.shortcut")}
                      value={ordinaryHotkeyKey(action.keys)}
                      onChange={(event) => replace(index, {
                        type: "hotkey",
                        keys: [...hotkeyModifiers(action.keys), event.target.value],
                      })}
                    >
                      {HOTKEY_KEY_GROUPS.map((group) => (
                        <optgroup label={t(language, group.label)} key={group.label}>
                          {group.keys.map((key) => (
                            <option value={key} key={key}>{formatHotkey([key])}</option>
                          ))}
                        </optgroup>
                      ))}
                    </select>
                  </div>
                  <button
                    className={recording === index ? "record-button is-recording" : "record-button"}
                    type="button"
                    aria-label={t(language, "behavior.record")}
                    onClick={() => setRecording((current) => current === index ? null : index)}
                    onBlur={() => setRecording(null)}
                  >
                    <Keyboard size={16} />
                    {t(language, "behavior.record")}
                  </button>
                </div>
              )}

              {action.type === "delay" && (
                <label className="field-stack">
                  <span>{t(language, "behavior.duration")}</span>
                  <input
                    aria-label={t(language, "behavior.duration")}
                    type="number"
                    min={1}
                    max={60_000}
                    step={10}
                    value={action.duration_ms || ""}
                    onChange={(event) => replace(index, {
                      type: "delay",
                      duration_ms: event.target.valueAsNumber || 0,
                    })}
                  />
                  {(!Number.isInteger(action.duration_ms) || action.duration_ms < 1 || action.duration_ms > 60_000) && (
                    <small className="field-error">{t(language, "behavior.durationInvalid")}</small>
                  )}
                </label>
              )}

              {action.type === "media" && (
                <label className="field-stack">
                  <span>{t(language, "behavior.mediaCommand")}</span>
                  <select
                    aria-label={t(language, "behavior.mediaCommand")}
                    value={action.command}
                    onChange={(event) => replace(index, {
                      type: "media",
                      command: event.target.value as MediaCommand,
                    })}
                  >
                    {MEDIA_COMMANDS.map((command) => (
                      <option value={command.value} key={command.value}>{t(language, command.label)}</option>
                    ))}
                  </select>
                </label>
              )}

              {action.type === "open" && (
                <label className="field-stack">
                  <span>{t(language, "behavior.openTarget")}</span>
                  <input
                    aria-label={t(language, "behavior.openTarget")}
                    type="text"
                    value={action.target}
                    spellCheck={false}
                    onChange={(event) => replace(index, { type: "open", target: event.target.value })}
                  />
                  {!action.target.trim() && (
                    <small className="field-error">{t(language, "behavior.openTargetRequired")}</small>
                  )}
                </label>
              )}
            </section>
          );
        })}
      </div>

      <div className="add-actions" aria-label={t(language, "behavior.add")}>
        <button type="button" onClick={() => onChange([...actions, { type: "paste", text: "" }])}>
          <TextCursorInput size={16} /><Plus size={13} />{t(language, "behavior.paste")}
        </button>
        <button type="button" onClick={() => onChange([...actions, { type: "hotkey", keys: ["enter"] }])}>
          <Keyboard size={16} /><Plus size={13} />{t(language, "behavior.hotkey")}
        </button>
        <button type="button" onClick={() => onChange([...actions, { type: "delay", duration_ms: 200 }])}>
          <Clock3 size={16} /><Plus size={13} />{t(language, "behavior.delay")}
        </button>
        <button type="button" onClick={() => onChange([...actions, { type: "media", command: "play_pause" }])}>
          <AudioLines size={16} /><Plus size={13} />{t(language, "behavior.media")}
        </button>
        <button type="button" onClick={() => onChange([...actions, { type: "open", target: "" }])}>
          <ExternalLink size={16} /><Plus size={13} />{t(language, "behavior.open")}
        </button>
      </div>
    </aside>
  );
}
