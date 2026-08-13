import { ArchiveRestore, DatabaseBackup, Settings2 } from "lucide-react";
import { t } from "./i18n";
import type { Language } from "./types";

export interface SettingsWorkspaceProps {
  language: Language;
  onLanguageChange(language: Language): void;
  onBackup(): void;
  onRestore(): void;
  onOpenAdvanced(): void;
}

export function SettingsWorkspace({ language, onLanguageChange, onBackup, onRestore, onOpenAdvanced }: SettingsWorkspaceProps) {
  return <div className="settings-workspace">
    <header className="content-heading"><div><h2>{t(language, "settings.workspaceTitle")}</h2></div></header>
    <section className="settings-section">
      <label><span>{t(language, "common.language")}</span><select aria-label={t(language, "common.language")} value={language} onChange={(event) => onLanguageChange(event.target.value as Language)}><option value="zh-CN">简体中文</option><option value="en-US">English</option></select></label>
    </section>
    <section className="settings-section settings-commands">
      <button type="button" onClick={onBackup}><DatabaseBackup size={16} />{t(language, "settings.backup")}</button>
      <button type="button" onClick={onRestore}><ArchiveRestore size={16} />{t(language, "settings.restore")}</button>
    </section>
    <section className="settings-section settings-advanced-entry"><button type="button" onClick={onOpenAdvanced}><Settings2 size={16} />{t(language, "settings.advanced")}</button></section>
  </div>;
}
