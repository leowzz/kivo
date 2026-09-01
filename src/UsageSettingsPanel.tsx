import { useEffect, useMemo, useState } from "react";
import { RefreshCw, Save } from "lucide-react";
import { t } from "./i18n";
import type { Language, UsageState, UsageView } from "./types";

export interface UsageSettingsPatch {
  enabled: boolean;
  baseUrl: string;
  email: string;
  password: string;
  intervalSeconds: number;
}

export interface UsageSettingsPanelProps {
  language: Language;
  usage: UsageView | null;
  onSave(settings: UsageSettingsPatch): Promise<void>;
}

function statusKey(state: UsageState) {
  return `usage.status.${state}` as const;
}

function initialPatch(usage: UsageView | null): UsageSettingsPatch {
  return {
    enabled: usage?.settings.enabled ?? false,
    baseUrl: usage?.settings.baseUrl ?? "",
    email: usage?.settings.email ?? "",
    password: "",
    intervalSeconds: usage?.settings.intervalSeconds ?? 60,
  };
}

export function UsageSettingsPanel({ language, usage, onSave }: UsageSettingsPanelProps) {
  const [draft, setDraft] = useState(() => initialPatch(usage));
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setDraft(initialPatch(usage));
  }, [
    usage?.settings.enabled,
    usage?.settings.baseUrl,
    usage?.settings.email,
    usage?.settings.intervalSeconds,
  ]);

  const validation = useMemo(() => {
    if (!Number.isInteger(draft.intervalSeconds) || draft.intervalSeconds < 2 || draft.intervalSeconds > 3600) {
      return t(language, "usage.intervalError");
    }
    if (!draft.enabled) return null;
    try {
      const url = new URL(draft.baseUrl);
      if (url.protocol !== "https:" || url.username || url.password || url.pathname !== "/" || url.search || url.hash) {
        return t(language, "usage.urlError");
      }
    } catch {
      return t(language, "usage.urlError");
    }
    if (!draft.email.trim()) return t(language, "usage.emailError");
    const identityChanged = draft.baseUrl.trim().replace(/\/$/, "") !== usage?.settings.baseUrl
      || draft.email.trim() !== usage?.settings.email;
    if ((identityChanged || usage?.settings.passwordRequired) && !draft.password) {
      return t(language, "usage.passwordError");
    }
    return null;
  }, [draft, language, usage]);

  const snapshot = usage?.snapshot;
  const cost = snapshot?.hasData ? `$${(snapshot.costMicros / 1_000_000).toFixed(2)}` : "-";

  return (
    <section className="data-card usage-settings" aria-labelledby="usage-settings-title">
      <div className="usage-settings-heading">
        <div>
          <h3 id="usage-settings-title">{t(language, "usage.title")}</h3>
          <p className={`usage-status is-${snapshot?.state ?? "disabled"}`}>
            <i />{t(language, statusKey(snapshot?.state ?? "disabled"))}
          </p>
        </div>
        <label className="usage-toggle">
          <input
            type="checkbox"
            checked={draft.enabled}
            onChange={(event) => setDraft((current) => ({ ...current, enabled: event.target.checked }))}
          />
          <span>{t(language, "usage.enabled")}</span>
        </label>
      </div>
      <div className="usage-metrics" aria-label={t(language, "usage.latest")}>
        <span><small>{t(language, "usage.cost")}</small><strong>{cost}</strong></span>
        <span><small>{t(language, "usage.tokens")}</small><strong>{snapshot?.hasData ? snapshot.todayTokens.toLocaleString() : "-"}</strong></span>
        <span><small>{t(language, "usage.tpm")}</small><strong>{snapshot?.hasData ? snapshot.tpm.toLocaleString() : "-"}</strong></span>
      </div>
      <div className="usage-fields">
        <label>
          <span>{t(language, "usage.baseUrl")}</span>
          <input value={draft.baseUrl} disabled={!draft.enabled} placeholder="https://sub2api.example.com" onChange={(event) => setDraft((current) => ({ ...current, baseUrl: event.target.value }))} />
        </label>
        <label>
          <span>{t(language, "usage.email")}</span>
          <input type="email" autoComplete="username" value={draft.email} disabled={!draft.enabled} onChange={(event) => setDraft((current) => ({ ...current, email: event.target.value }))} />
        </label>
        <label>
          <span>{t(language, "usage.password")}</span>
          <input type="password" autoComplete="current-password" value={draft.password} disabled={!draft.enabled} onChange={(event) => setDraft((current) => ({ ...current, password: event.target.value }))} />
        </label>
        <label>
          <span>{t(language, "usage.interval")}</span>
          <input type="number" min={2} max={3600} step={1} value={draft.intervalSeconds} onChange={(event) => setDraft((current) => ({ ...current, intervalSeconds: Number(event.target.value) }))} />
        </label>
      </div>
      {validation && <p className="field-error" role="alert">{validation}</p>}
      <div className="usage-actions">
        <button className="primary-button" type="button" aria-label={t(language, "usage.save")} disabled={Boolean(validation) || busy} onClick={async () => {
          setBusy(true);
          try {
            await onSave({
              ...draft,
              baseUrl: draft.baseUrl.trim().replace(/\/$/, ""),
              email: draft.email.trim(),
            });
            setDraft((current) => ({ ...current, password: "" }));
          } finally {
            setBusy(false);
          }
        }}>
          {busy ? <RefreshCw className="is-spinning" size={15} aria-hidden="true" /> : <Save size={15} aria-hidden="true" />}
          {t(language, "common.save")}
        </button>
      </div>
    </section>
  );
}
