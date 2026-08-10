import { useEffect, useState } from "react";
import { HotkeyPicker, hotkeyValidationMessage } from "./HotkeyPicker";
import { validateHotkey } from "./hotkey";
import { t, type MessageKey } from "./i18n";
import type { ActionTrigger, ButtonAction, Language, MediaCommand } from "./types";

export type ActionDraft = { trigger: ActionTrigger; action: ButtonAction };

export interface ActionDialogProps {
  open: boolean;
  language: Language;
  mode: "create" | "edit";
  initial?: ActionDraft;
  onSave(value: ActionDraft): void;
  onDelete?(): void;
  onCancel(): void;
}

const TRIGGERS: Array<{ value: ActionTrigger; label: "behavior.trigger.press" | "behavior.trigger.release" | "behavior.trigger.longPress" | "behavior.trigger.doublePress" }> = [
  { value: "press", label: "behavior.trigger.press" },
  { value: "release", label: "behavior.trigger.release" },
  { value: "long_press", label: "behavior.trigger.longPress" },
  { value: "double_press", label: "behavior.trigger.doublePress" },
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

function defaultAction(type: ButtonAction["type"]): ButtonAction {
  switch (type) {
    case "paste": return { type, text: "" };
    case "hotkey": return { type, keys: [] };
    case "delay": return { type, duration_ms: 100 };
    case "media": return { type, command: "play_pause" };
    case "open": return { type, target: "" };
  }
}

function validateAction(action: ButtonAction, language: Language): string | null {
  switch (action.type) {
    case "paste": return action.text ? null : t(language, "behavior.textRequired");
    case "hotkey": {
      const error = validateHotkey(action.keys);
      return hotkeyValidationMessage(language, error);
    }
    case "delay": return Number.isInteger(action.duration_ms) && action.duration_ms >= 1 && action.duration_ms <= 60_000 ? null : t(language, "behavior.durationInvalid");
    case "open":
      if (!action.target.trim()) return t(language, "behavior.openTargetRequired");
      if (action.target.length > 2_048) return t(language, "behavior.openTargetTooLong");
      if (action.target.includes("\0")) return t(language, "behavior.openTargetNul");
      return null;
    case "media": return null;
  }
}

export function ActionDialog({ open, language, mode, initial, onSave, onDelete, onCancel }: ActionDialogProps) {
  const [draft, setDraft] = useState<ActionDraft>(initial ?? { trigger: "press", action: { type: "hotkey", keys: [] } });
  const [error, setError] = useState<string | null>(null);
  const [recording, setRecording] = useState(false);

  useEffect(() => {
    if (!open) return;
    setDraft(initial ?? { trigger: "press", action: { type: "hotkey", keys: [] } });
    setError(null);
  }, [initial, open]);

  useEffect(() => {
    if (!open) return;
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (recording) return;
      event.preventDefault();
      onCancel();
    };
    window.addEventListener("keydown", handleEscape, true);
    return () => window.removeEventListener("keydown", handleEscape, true);
  }, [onCancel, open, recording]);

  if (!open) return null;

  const save = () => {
    const validation = validateAction(draft.action, language);
    if (validation) {
      setError(validation);
      return;
    }
    setError(null);
    onSave(draft);
  };

  const changeType = (type: ButtonAction["type"]) => {
    setDraft((current) => ({ ...current, action: defaultAction(type) }));
    setError(null);
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="action-dialog" role="dialog" aria-modal="true" aria-labelledby="action-dialog-title">
        <div className="dialog-heading">
          <h2 id="action-dialog-title">{t(language, mode === "edit" ? "behavior.editAction" : "behavior.add")}</h2>
        </div>
        <div className="action-dialog-fields">
          <label className="field-stack">
            <span>{t(language, "behavior.trigger")}</span>
            <select aria-label={t(language, "behavior.trigger")} value={draft.trigger} onChange={(event) => setDraft((current) => ({ ...current, trigger: event.target.value as ActionTrigger }))}>
              {TRIGGERS.map(({ value, label }) => <option value={value} key={value}>{t(language, label)}</option>)}
            </select>
          </label>
          <label className="field-stack">
            <span>{t(language, "behavior.actionType")}</span>
            <select aria-label={t(language, "behavior.actionType")} value={draft.action.type} onChange={(event) => changeType(event.target.value as ButtonAction["type"])}>
              <option value="hotkey">{t(language, "behavior.hotkey")}</option>
              <option value="paste">{t(language, "behavior.paste")}</option>
              <option value="delay">{t(language, "behavior.delay")}</option>
              <option value="media">{t(language, "behavior.media")}</option>
              <option value="open">{t(language, "behavior.open")}</option>
            </select>
          </label>

          {draft.action.type === "hotkey" && (
            <HotkeyPicker value={draft.action.keys} language={language} error={error} onRecordingChange={setRecording} onChange={(keys) => { setDraft((current) => ({ ...current, action: { type: "hotkey", keys } })); setError(null); }} />
          )}
          {draft.action.type === "paste" && (
            <label className="field-stack">
              <span>{t(language, "behavior.text")}</span>
              <textarea aria-label={t(language, "behavior.text")} rows={5} value={draft.action.text} onChange={(event) => { setDraft((current) => ({ ...current, action: { type: "paste", text: event.target.value } })); setError(null); }} />
            </label>
          )}
          {draft.action.type === "delay" && (
            <label className="field-stack">
              <span>{t(language, "behavior.duration")}</span>
              <input aria-label={t(language, "behavior.duration")} type="number" min={1} max={60_000} step={10} value={draft.action.duration_ms || ""} onChange={(event) => { setDraft((current) => ({ ...current, action: { type: "delay", duration_ms: event.target.valueAsNumber || 0 } })); setError(null); }} />
            </label>
          )}
          {draft.action.type === "media" && (
            <label className="field-stack">
              <span>{t(language, "behavior.mediaCommand")}</span>
              <select aria-label={t(language, "behavior.mediaCommand")} value={draft.action.command} onChange={(event) => { setDraft((current) => ({ ...current, action: { type: "media", command: event.target.value as MediaCommand } })); setError(null); }}>
                {MEDIA_COMMANDS.map(({ value, label }) => <option value={value} key={value}>{t(language, label)}</option>)}
              </select>
            </label>
          )}
          {draft.action.type === "open" && (
            <label className="field-stack">
              <span>{t(language, "behavior.openTarget")}</span>
              <input aria-label={t(language, "behavior.openTarget")} value={draft.action.target} onChange={(event) => { setDraft((current) => ({ ...current, action: { type: "open", target: event.target.value } })); setError(null); }} />
            </label>
          )}
          {error && draft.action.type !== "hotkey" && <small className="field-error">{error}</small>}
        </div>
        <div className="dialog-actions">
          {mode === "edit" && <button type="button" className="danger-button" aria-label={t(language, "behavior.deleteAction")} onClick={onDelete}>{t(language, "behavior.deleteAction")}</button>}
          <span className="dialog-actions-spacer" />
          <button type="button" className="secondary-button" onClick={onCancel}>{t(language, "behavior.cancel")}</button>
          <button type="button" className="primary-button" onClick={save}>{t(language, "behavior.save")}</button>
        </div>
      </section>
    </div>
  );
}
