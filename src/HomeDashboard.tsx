import { Activity, Hash, MousePointer2, Trophy } from "lucide-react";
import { t } from "./i18n";
import type { ConnectionStatus, HomeMetricsSnapshot, Language, ModelConfig } from "./types";

interface Props {
  connection: ConnectionStatus;
  language: Language;
  metrics: HomeMetricsSnapshot | null;
  model: ModelConfig | undefined;
}

function buttonLabel(model: ModelConfig | undefined, buttonId: string | undefined) {
  if (!buttonId) return "-";
  return model?.model.groups.flatMap((group) => group.buttons).find((button) => button.id === buttonId)?.label ?? buttonId;
}

export function HomeDashboard({ connection, language, metrics, model }: Props) {
  const maxHeat = Math.max(1, ...((metrics?.heatmap ?? []).map((entry) => entry.presses)));
  return (
    <div className="home-dashboard">
      <section className="home-main" aria-labelledby="home-title">
        <header className="content-heading home-heading">
          <div><span>{model?.model.name ?? "Kivo"}</span><h2 id="home-title">{t(language, "home.title")}</h2></div>
          <div className={connection.state === "connected" ? "home-device is-connected" : "home-device"}>
            <Activity size={15} /><span>{t(language, connection.state === "connected" ? "connection.connected" : "connection.searching")}</span>
            {connection.port && <code>{connection.port}</code>}
          </div>
        </header>
        {!metrics ? <p className="home-unavailable">{t(language, "home.unavailable")}</p> : <>
          <div className="metric-grid">
            <div className="metric-card"><MousePointer2 size={18} /><span>{t(language, "home.todayPresses")}</span><strong>{metrics.todayPresses}</strong></div>
            <div className="metric-card"><Hash size={18} /><span>{t(language, "home.activeButtons")}</span><strong>{metrics.activeButtonCount}</strong></div>
            <div className="metric-card"><Trophy size={18} /><span>{t(language, "home.topButton")}</span><strong>{buttonLabel(model, metrics.topButton?.buttonId)}</strong></div>
          </div>
          <section className="heatmap-section" aria-labelledby="heatmap-title">
            <div className="panel-title"><div><span>{t(language, "home.totalPresses")}: {metrics.totalPresses}</span><h2 id="heatmap-title">{t(language, "home.heatmap")}</h2></div></div>
            <div className="heatmap" aria-label={t(language, "home.heatmap")}>
              {metrics.heatmap.map((entry) => <div className="heat-cell" key={`${entry.buttonId}-${entry.day}`} style={{ backgroundColor: `rgba(23, 116, 87, ${.12 + (entry.presses / maxHeat) * .88})` }} title={`${buttonLabel(model, entry.buttonId)}: ${entry.presses}`}><span>{buttonLabel(model, entry.buttonId)}</span><strong>{entry.presses}</strong><small>{entry.day.slice(5)}</small></div>)}
            </div>
          </section>
        </>}
      </section>
      <aside className="activity-log" aria-label={t(language, "home.logs")}>
        <div className="panel-title"><div><span>{metrics?.logs.length ?? 0}</span><h2>{t(language, "home.logs")}</h2></div></div>
        <div className="activity-log-list">
          {metrics?.logs.length ? metrics.logs.map((log, index) => <div className="activity-log-item" key={`${log.timestampMs}-${index}`}><time>{new Date(log.timestampMs).toLocaleTimeString()}</time><span>{log.message}</span></div>) : <p className="panel-empty">{t(language, "activity.empty")}</p>}
        </div>
      </aside>
    </div>
  );
}
