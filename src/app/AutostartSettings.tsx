import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { t } from "./i18n";
import type { Language } from "./types";

export function AutostartSettings({ language, preview = false }: {
  language: Language;
  preview?: boolean;
}) {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (preview) return;
    let cancelled = false;
    void invoke<boolean>("plugin:autostart|is_enabled").then(
      (value) => { if (!cancelled) setEnabled(value); },
      (reason) => { if (!cancelled) setError(String(reason)); },
    );
    return () => { cancelled = true; };
  }, [preview]);

  async function toggle() {
    if (busy || enabled === null || preview) return;
    setBusy(true);
    setError(null);
    try {
      await invoke(enabled ? "plugin:autostart|disable" : "plugin:autostart|enable");
      setEnabled(await invoke<boolean>("plugin:autostart|is_enabled"));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="data-card">
      <h3>{t(language, "settings.general")}</h3>
      <div className="autostart-setting">
        <div>
          <label id="autostart-label" htmlFor="autostart-switch">
            {t(language, "settings.autostart")}
          </label>
          <p id="autostart-hint" className="content-subtitle">
            {t(language, preview ? "settings.autostartPreview" : "settings.autostartHint")}
          </p>
        </div>
        <button
          id="autostart-switch"
          className="settings-switch"
          type="button"
          role="switch"
          aria-labelledby="autostart-label"
          aria-describedby="autostart-hint"
          aria-checked={enabled === true}
          aria-busy={busy}
          disabled={preview || enabled === null || busy}
          onClick={() => void toggle()}
        ><span /></button>
      </div>
      {error && <p className="error-banner" role="alert">
        {t(language, "settings.autostartError")}: {error}
      </p>}
    </section>
  );
}
