import { Download, RefreshCw } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import { t } from "../app/i18n";
import type { Language } from "../app/types";
import appPackage from "../../package.json";
import type { useReleaseUpdate } from "./useReleaseUpdate";
import { releasesUrl, updateIsBusy } from "./releaseUpdate";
import "./styles/update.css";

export function UpdateSettings({ update, language, blocked = false, productName = "Kivo", titleId = "update-settings-title" }: {
  update: ReturnType<typeof useReleaseUpdate>;
  language: Language;
  blocked?: boolean;
  productName?: string;
  titleId?: string;
}) {
  const [openFailed, setOpenFailed] = useState(false);
  const busy = updateIsBusy(update.phase);
  const progress = update.total && update.total > 0
    ? `${Math.min(100, Math.floor(update.downloaded / update.total * 100))}%`
    : `${(update.downloaded / 1024 / 1024).toFixed(1)} MB`;
  const status = !update.native ? t(language, "update.preview")
    : update.phase === "checking" ? t(language, "update.checking")
    : update.phase === "preparing" ? t(language, "update.preparing")
    : update.phase === "downloading" ? t(language, "update.downloading", { progress })
    : update.phase === "installing" ? t(language, "update.installing")
    : update.phase === "restarting" ? t(language, "update.restarting")
    : update.phase === "ready" ? t(language, "update.ready")
    : update.hasUpdate ? t(language, "update.available", { version: update.latestVersion! })
    : update.error ? t(language, "update.failed")
    : update.checkedAt !== null ? t(language, "update.current")
    : t(language, "update.waiting");

  return (
    <section className="data-card update-settings" aria-labelledby={titleId}>
      <div className="update-settings-heading">
        <h3 id={titleId}>{t(language, "update.title")}</h3>
        <span>{productName} {appPackage.version}</span>
      </div>
      <div className="update-settings-row">
        <div className="update-settings-status" role="status" aria-live="polite">
          <strong>{status}</strong>
          {update.error && <p className="update-error">{t(language, `update.error.${update.error}`)}</p>}
          {openFailed && <p className="update-error">{t(language, "update.error.open")}</p>}
          {!update.error && update.hasUpdate && !busy && <p>{t(language, blocked ? "update.blocked" : "update.hint")}</p>}
          {update.phase === "downloading" && <progress aria-label={t(language, "update.progress")}
            max={update.total && update.total > 0 ? update.total : undefined}
            value={update.total && update.total > 0 ? Math.min(update.downloaded, update.total) : undefined} />}
        </div>
        <div className="update-settings-actions">
          <button type="button" disabled={!update.native || update.phase !== "idle"} onClick={() => update.check(true)}>
            <RefreshCw size={16} aria-hidden="true" />{t(language, update.phase === "checking" ? "update.checkingButton" : "update.check")}
          </button>
          {!busy && (update.hasUpdate || update.error) && <a href={releasesUrl} target="_blank" rel="noopener noreferrer"
            onClick={(event) => {
              if (!update.native) return;
              event.preventDefault();
              setOpenFailed(false);
              void openUrl(releasesUrl).catch(() => setOpenFailed(true));
            }}>{t(language, "update.download")}</a>}
          {update.native && update.hasUpdate && <button type="button" className="primary"
            disabled={busy || update.phase === "checking" || blocked} onClick={update.install}>
            <Download size={16} aria-hidden="true" />{t(language, update.phase === "ready" ? "update.restart" : busy ? "update.updating" : "update.install")}
          </button>}
        </div>
      </div>
    </section>
  );
}

export function UpdateProgressDialog({ update, language, productName }: {
  update: ReturnType<typeof useReleaseUpdate>;
  language: Language;
  productName?: string;
}) {
  if (!updateIsBusy(update.phase)) return null;
  return <div className="update-progress-backdrop">
    <div role="dialog" aria-modal="true" aria-labelledby="update-progress-title">
      <UpdateSettings update={update} language={language} productName={productName} titleId="update-progress-title" />
    </div>
  </div>;
}
