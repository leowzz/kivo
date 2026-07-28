import { ArrowDown, ArrowUp, Keyboard, Plus, TextCursorInput, Trash2 } from "lucide-react";
import { useState } from "react";
import { formatHotkey, normalizeHotkey } from "./hotkey";
import { t } from "./i18n";
import type { ButtonAction, Language, ModelButton } from "./types";

interface ActionEditorProps {
  language: Language;
  button: ModelButton | null;
  actions: ButtonAction[];
  onChange(actions: ButtonAction[]): void;
}

function move(actions: ButtonAction[], index: number, offset: -1 | 1) {
  const next = [...actions];
  [next[index], next[index + offset]] = [next[index + offset], next[index]];
  return next;
}

export function ActionEditor({ language, button, actions, onChange }: ActionEditorProps) {
  const [recording, setRecording] = useState<number | null>(null);

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
        {actions.map((action, index) => (
          <section className="action-item" key={`${action.type}-${index}`}>
            <div className="action-item-header">
              <span>{index + 1}. {t(language, action.type === "paste" ? "behavior.paste" : "behavior.hotkey")}</span>
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

            {action.type === "paste" ? (
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
            ) : (
              <div className="hotkey-field">
                <span>{t(language, "behavior.shortcut")}</span>
                <output>{action.keys.length ? formatHotkey(action.keys) : "-"}</output>
                <button
                  className={recording === index ? "record-button is-recording" : "record-button"}
                  type="button"
                  aria-label={t(language, "behavior.record")}
                  onClick={() => setRecording(index)}
                  onBlur={() => setRecording(null)}
                  onKeyDown={(event) => {
                    if (recording !== index) return;
                    event.preventDefault();
                    try {
                      const keys = normalizeHotkey(event.nativeEvent);
                      if (keys) {
                        replace(index, { type: "hotkey", keys });
                        setRecording(null);
                      }
                    } catch {
                      // Keep recording until a supported key is pressed.
                    }
                  }}
                >
                  <Keyboard size={16} />
                  {t(language, "behavior.record")}
                </button>
              </div>
            )}
          </section>
        ))}
      </div>

      <div className="add-actions" aria-label={t(language, "behavior.add")}>
        <button type="button" onClick={() => onChange([...actions, { type: "paste", text: "" }])}>
          <TextCursorInput size={16} />
          <Plus size={13} />
          {t(language, "behavior.paste")}
        </button>
        <button type="button" onClick={() => onChange([...actions, { type: "hotkey", keys: ["enter"] }])}>
          <Keyboard size={16} />
          <Plus size={13} />
          {t(language, "behavior.hotkey")}
        </button>
      </div>
    </aside>
  );
}
