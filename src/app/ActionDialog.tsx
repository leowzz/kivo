import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open as chooseFile } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { HotkeyPicker, hotkeyValidationMessage } from "./HotkeyPicker";
import { validateHotkey } from "./hotkey";
import { t, type MessageKey } from "./i18n";
import { normalizeApplicationPath, normalizeWebsiteUrl, validLaunchTargetLength } from "./launchTarget";
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
    case "open_app": return { type, path: "" };
    case "open_website": return { type, url: "" };
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
    case "open_app": {
      const path = normalizeApplicationPath(action.path);
      if (!path) return t(language, "behavior.applicationPathInvalid");
      return validLaunchTargetLength(path) ? null : t(language, "behavior.openTargetTooLong");
    }
    case "open_website": {
      const url = normalizeWebsiteUrl(action.url);
      if (!url) return t(language, "behavior.websiteUrlInvalid");
      return validLaunchTargetLength(url) ? null : t(language, "behavior.openTargetTooLong");
    }
  }
}

export function ActionDialog({ open, language, mode, initial, onSave, onDelete, onCancel }: ActionDialogProps) {
  const [draft, setDraft] = useState<ActionDraft>(initial ?? { trigger: "press", action: { type: "hotkey", keys: [] } });
  const [error, setError] = useState<string | null>(null);
  const [recording, setRecording] = useState(false);
  const [picking, setPicking] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const dialogRef = useRef<HTMLElement>(null);
  const pickerGeneration = useRef(0);
  const nativeRuntime = "__TAURI_INTERNALS__" in window;
  const macOS = navigator.platform.includes("Mac");

  useEffect(() => {
    if (!open) return;
    setDraft(initial ?? { trigger: "press", action: { type: "hotkey", keys: [] } });
    setError(null);
  }, [initial, open]);

  useEffect(() => {
    pickerGeneration.current += 1;
    setPicking(false);
    setDragOver(false);
    return () => { pickerGeneration.current += 1; };
  }, [open, initial, draft.action.type]);

  useEffect(() => {
    if (!open || !nativeRuntime || draft.action.type !== "open_app") return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebviewWindow().onDragDropEvent(({ payload }) => {
      if (disposed) return;
      if (payload.type === "leave") {
        setDragOver(false);
        return;
      }
      const rect = dialogRef.current?.getBoundingClientRect();
      const scale = window.devicePixelRatio || 1;
      const x = payload.position.x / scale;
      const y = payload.position.y / scale;
      const inside = Boolean(rect && x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom);
      setDragOver(payload.type !== "drop" && inside);
      if (payload.type !== "drop" || !inside) return;
      if (payload.paths.length !== 1) {
        setError(t(language, "behavior.applicationDropInvalid"));
        return;
      }
      const path = normalizeApplicationPath(payload.paths[0]);
      if (!path) {
        setError(t(language, "behavior.applicationPathInvalid"));
        return;
      }
      setDraft((current) => ({ ...current, action: { type: "open_app", path } }));
      setError(null);
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(() => {
      // Manual entry and the picker remain available if native drops are unavailable.
    });
    return () => { disposed = true; unlisten?.(); };
  }, [open, nativeRuntime, draft.action.type, language]);

  useEffect(() => {
    if (!open) return;
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (recording || picking) return;
      event.preventDefault();
      onCancel();
    };
    window.addEventListener("keydown", handleEscape, true);
    return () => window.removeEventListener("keydown", handleEscape, true);
  }, [onCancel, open, recording, picking]);

  if (!open) return null;

  const chooseApplication = async () => {
    const generation = pickerGeneration.current;
    setPicking(true);
    setError(null);
    try {
      const path = await chooseFile({
        title: t(language, "behavior.chooseApplication"),
        multiple: false,
        defaultPath: macOS ? "/Applications" : undefined,
        filters: [{ name: t(language, "behavior.openApp"), extensions: macOS ? ["app"] : ["exe", "com", "lnk"] }],
      });
      if (generation !== pickerGeneration.current) return;
      if (typeof path === "string") {
        setDraft((current) => ({ ...current, action: { type: "open_app", path } }));
      }
    } catch {
      if (generation === pickerGeneration.current) setError(t(language, "behavior.applicationPickFailed"));
    } finally {
      if (generation === pickerGeneration.current) setPicking(false);
    }
  };

  const save = () => {
    const validation = validateAction(draft.action, language);
    if (validation) {
      setError(validation);
      return;
    }
    setError(null);
    const action = draft.action.type === "open_app"
      ? { ...draft.action, path: normalizeApplicationPath(draft.action.path)! }
      : draft.action.type === "open_website"
        ? { ...draft.action, url: normalizeWebsiteUrl(draft.action.url)! }
        : draft.action;
    onSave({ ...draft, action });
  };

  const changeType = (type: ButtonAction["type"]) => {
    setDraft((current) => ({ ...current, action: defaultAction(type) }));
    setError(null);
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section ref={dialogRef} className={`action-dialog${dragOver ? " is-drop-target" : ""}`} role="dialog" aria-modal="true" aria-labelledby="action-dialog-title">
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
              <option value="open_app">{t(language, "behavior.openApp")}</option>
              <option value="open_website">{t(language, "behavior.openWebsite")}</option>
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
          {draft.action.type === "open_app" && (
            <div className="field-stack">
              <label htmlFor="application-path">{t(language, "behavior.applicationPath")}</label>
              <div className="launch-target-row">
                <input id="application-path" value={draft.action.path} placeholder={macOS ? "/Applications/Safari.app" : "C:\\Program Files\\App\\App.exe"} spellCheck={false} onChange={(event) => { setDraft((current) => ({ ...current, action: { type: "open_app", path: event.target.value } })); setError(null); }} />
                <button className="icon-button" type="button" aria-label={t(language, "behavior.chooseApplication")} title={t(language, nativeRuntime ? "behavior.chooseApplication" : "behavior.desktopPickerOnly")} disabled={!nativeRuntime || picking} onClick={() => void chooseApplication()}><FolderOpen size={18} /></button>
              </div>
              <small>{t(language, "behavior.applicationHint")}</small>
            </div>
          )}
          {draft.action.type === "open_website" && (
            <label className="field-stack">
              <span>{t(language, "behavior.websiteUrl")}</span>
              <input aria-label={t(language, "behavior.websiteUrl")} value={draft.action.url} placeholder="https://example.com" spellCheck={false} onChange={(event) => { setDraft((current) => ({ ...current, action: { type: "open_website", url: event.target.value } })); setError(null); }} />
              <small>{t(language, "behavior.websiteHint")}</small>
            </label>
          )}
          {error && draft.action.type !== "hotkey" && <small className="field-error">{error}</small>}
        </div>
        <div className="dialog-actions">
          {mode === "edit" && <button type="button" className="danger-button" aria-label={t(language, "behavior.deleteAction")} onClick={onDelete}>{t(language, "behavior.deleteAction")}</button>}
          <span className="dialog-actions-spacer" />
          <button type="button" className="secondary-button" onClick={onCancel}>{t(language, "behavior.cancel")}</button>
          <button type="button" className="primary-button" disabled={picking} onClick={save}>{t(language, "behavior.save")}</button>
        </div>
      </section>
    </div>
  );
}
