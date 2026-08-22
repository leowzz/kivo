import { useEffect, useMemo, useState } from "react";
import { X } from "lucide-react";
import { t } from "./i18n";
import type { DeviceProfile, Language, TriggerSettings } from "./types";

export interface ConfigurationSettingsDialogProps {
  open: boolean;
  profile: DeviceProfile;
  sharedDeviceCount: number;
  language?: Language;
  onSave(settings: TriggerSettings): void;
  onDuplicate(name: string): Promise<void> | void;
  onDraftChange?(settings: TriggerSettings): void;
  allowDuplicate?: boolean;
  onCancel(): void;
}

function parseSeconds(value: string) {
  const trimmed = value.trim();
  if (!trimmed || !/^(?:\d+(?:\.\d*)?|\.\d+)$/.test(trimmed)) return NaN;
  const seconds = Number(trimmed);
  return Number.isFinite(seconds) ? Math.round(seconds * 1000) : NaN;
}

function formatSeconds(milliseconds: number) {
  return Number.isFinite(milliseconds) ? String(milliseconds / 1000) : "";
}

export function ConfigurationSettingsDialog({
  open,
  profile,
  sharedDeviceCount,
  language = "zh-CN",
  onSave,
  onDuplicate,
  onDraftChange,
  allowDuplicate = true,
  onCancel,
}: ConfigurationSettingsDialogProps) {
  const [settings, setSettings] = useState<TriggerSettings>(profile.trigger_settings);
  const [longPressSeconds, setLongPressSeconds] = useState(() => formatSeconds(profile.trigger_settings.long_press_ms));
  const [doublePressSeconds, setDoublePressSeconds] = useState(() => formatSeconds(profile.trigger_settings.double_press_ms));
  const [copyName, setCopyName] = useState(profile.profile.name);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setSettings(profile.trigger_settings);
    setLongPressSeconds(formatSeconds(profile.trigger_settings.long_press_ms));
    setDoublePressSeconds(formatSeconds(profile.trigger_settings.double_press_ms));
    setCopyName(profile.profile.name);
    setBusy(false);
  }, [open, profile.profile.id]);

  const validation = useMemo(() => {
    const long = settings.long_press_ms;
    const double = settings.double_press_ms;
    if (!Number.isInteger(long) || !Number.isInteger(double)) return t(language, "settings.invalidSecondsError");
    if (long < 100 || long > 5000 || double < 100 || double > 1000) return t(language, "settings.secondsRangeError");
    return null;
  }, [language, settings]);

  if (!open) return null;
  const saveLabel = sharedDeviceCount > 1 ? t(language, "devices.saveShared") : t(language, "common.save");

  return (
    <div className="modal-backdrop" role="presentation">
      <section className="device-setup-dialog configuration-settings-dialog" role="dialog" aria-modal="true" aria-labelledby="configuration-settings-title">
        <header className="device-setup-header">
          <h2 id="configuration-settings-title">{t(language, "settings.title")}</h2>
          <button className="icon-button" type="button" aria-label={t(language, "common.close")} title={t(language, "common.close")} onClick={onCancel}><X size={17} aria-hidden="true" /></button>
        </header>
        <div className="device-setup-body">
          <div className="settings-fields">
            <label>
              <span>{t(language, "settings.longPressSeconds")}</span>
              <input aria-label={t(language, "settings.longPressSeconds")} type="number" min={0.1} max={5} step={0.001} value={longPressSeconds} onChange={(event) => { const value = event.target.value; setLongPressSeconds(value); setSettings((current) => { const next = { ...current, long_press_ms: parseSeconds(value) }; onDraftChange?.(next); return next; }); }} />
            </label>
            <label>
              <span>{t(language, "settings.doublePressSeconds")}</span>
              <input aria-label={t(language, "settings.doublePressSeconds")} type="number" min={0.1} max={1} step={0.001} value={doublePressSeconds} onChange={(event) => { const value = event.target.value; setDoublePressSeconds(value); setSettings((current) => { const next = { ...current, double_press_ms: parseSeconds(value) }; onDraftChange?.(next); return next; }); }} />
            </label>
          </div>
          {validation && <p className="field-error" role="alert">{validation}</p>}
          {allowDuplicate && <div className="settings-duplicate">
            <label>
              <span>{t(language, "settings.duplicateName")}</span>
              <input aria-label={t(language, "settings.duplicateName")} value={copyName} onChange={(event) => setCopyName(event.target.value)} />
            </label>
            <p className="form-hint">{t(language, "settings.duplicateHint")}</p>
          </div>}
        </div>
        <footer className="device-setup-footer">
          <button className="secondary-button" type="button" onClick={onCancel}>{t(language, "common.cancel")}</button>
          <button className="primary-button" type="button" disabled={Boolean(validation) || busy} onClick={() => onSave(settings)}>{saveLabel}</button>
          {allowDuplicate && <button className="secondary-button" type="button" disabled={!copyName.trim() || Boolean(validation) || busy} onClick={async () => {
            setBusy(true);
            try { await onDuplicate(copyName.trim()); } finally { setBusy(false); }
          }}>{t(language, "devices.duplicateForDevice")}</button>}
        </footer>
      </section>
    </div>
  );
}
