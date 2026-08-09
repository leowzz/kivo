import { useEffect, useMemo, useState } from "react";
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
  onCancel(): void;
}

function parseInteger(value: string) {
  return /^\d+$/.test(value.trim()) ? Number(value) : NaN;
}

export function ConfigurationSettingsDialog({
  open,
  profile,
  sharedDeviceCount,
  language = "zh-CN",
  onSave,
  onDuplicate,
  onDraftChange,
  onCancel,
}: ConfigurationSettingsDialogProps) {
  const [settings, setSettings] = useState<TriggerSettings>(profile.trigger_settings);
  const [copyName, setCopyName] = useState(profile.profile.name);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setSettings(profile.trigger_settings);
    setCopyName(profile.profile.name);
    setBusy(false);
  }, [open, profile]);

  const validation = useMemo(() => {
    const long = settings.long_press_ms;
    const double = settings.double_press_ms;
    if (!Number.isInteger(long) || !Number.isInteger(double)) return t(language, "settings.integerError");
    if (long < 100 || long > 5000 || double < 100 || double > 1000) return t(language, "settings.rangeError");
    return null;
  }, [language, settings]);

  if (!open) return null;
  const saveLabel = sharedDeviceCount > 1 ? t(language, "devices.saveShared") : t(language, "common.save");

  return (
    <div className="modal-backdrop" role="presentation">
      <section className="device-setup-dialog configuration-settings-dialog" role="dialog" aria-modal="true" aria-labelledby="configuration-settings-title">
        <header className="device-setup-header">
          <h2 id="configuration-settings-title">{t(language, "settings.title")}</h2>
          <button className="icon-button" type="button" aria-label={t(language, "common.close")} title={t(language, "common.close")} onClick={onCancel}>×</button>
        </header>
        <div className="device-setup-body">
          <div className="settings-fields">
            <label>
              <span>{t(language, "settings.longPress")}</span>
              <input aria-label={t(language, "settings.longPress")} type="number" min={100} max={5000} step={1} value={Number.isNaN(settings.long_press_ms) ? "" : settings.long_press_ms} onChange={(event) => setSettings((current) => { const next = { ...current, long_press_ms: parseInteger(event.target.value) }; onDraftChange?.(next); return next; })} />
            </label>
            <label>
              <span>{t(language, "settings.doublePress")}</span>
              <input aria-label={t(language, "settings.doublePress")} type="number" min={100} max={1000} step={1} value={Number.isNaN(settings.double_press_ms) ? "" : settings.double_press_ms} onChange={(event) => setSettings((current) => { const next = { ...current, double_press_ms: parseInteger(event.target.value) }; onDraftChange?.(next); return next; })} />
            </label>
          </div>
          {validation && <p className="field-error" role="alert">{validation}</p>}
          <div className="settings-duplicate">
            <label>
              <span>{t(language, "settings.duplicateName")}</span>
              <input aria-label={t(language, "settings.duplicateName")} value={copyName} onChange={(event) => setCopyName(event.target.value)} />
            </label>
            <p className="form-hint">{t(language, "settings.duplicateHint")}</p>
          </div>
        </div>
        <footer className="device-setup-footer">
          <button type="button" onClick={onCancel}>{t(language, "common.cancel")}</button>
          <button type="button" disabled={Boolean(validation) || busy} onClick={() => onSave(settings)}>{saveLabel}</button>
          <button className="primary-button" type="button" disabled={!copyName.trim() || Boolean(validation) || busy} onClick={async () => {
            setBusy(true);
            try { await onDuplicate(copyName.trim()); } finally { setBusy(false); }
          }}>{t(language, "devices.duplicateForDevice")}</button>
        </footer>
      </section>
    </div>
  );
}
