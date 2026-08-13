import { FileInput, Plus, Trash2, Upload } from "lucide-react";
import { t } from "./i18n";
import type { DeviceProfile, DeviceStatus, Language } from "./types";

export interface ProfileManagerProps {
  language: Language;
  profiles: DeviceProfile[];
  editorProfileId: string | null;
  devices: DeviceStatus[];
  onCreate(sourceProfileId?: string): void;
  onSelect(profileId: string): void;
  onImport(): void;
  onExport(profile: DeviceProfile): void;
  onDelete(profile: DeviceProfile): void;
}

export function ProfileManager({ language, profiles, editorProfileId, devices, onCreate, onSelect, onImport, onExport, onDelete }: ProfileManagerProps) {
  return <section className="profile-manager" aria-label={t(language, "data.profileList")}>
    <header className="profile-manager-header"><h2>{t(language, "advanced.profiles")}</h2><button className="primary-button" type="button" onClick={() => onCreate()}><Plus size={16} />{t(language, "profile.create")}</button></header>
    <div className="profile-manager-transfer"><button type="button" onClick={onImport}><FileInput size={16} />{t(language, "profile.import")}</button></div>
    <div className="profile-list">
      {profiles.length === 0 && <p className="empty-workspace-copy">{t(language, "model.empty")}</p>}
      {profiles.map((profile) => {
        const usage = devices.filter((device) => device.runtimeAssignment?.device_profile_id === profile.profile.id).length;
        return <article className="profile-row" key={profile.profile.id}>
          <div className="profile-row-main"><div className="profile-row-title"><h3>{profile.profile.name}</h3>{profile.profile.id === editorProfileId && <span className="profile-badge">{t(language, "advanced.editorBadge")}</span>}</div><p>{t(language, "advanced.usedBy", { count: usage })}</p><code>{profile.profile.id}</code></div>
          <div className="profile-row-actions">
            <button type="button" aria-label={`${t(language, "advanced.select")} ${profile.profile.name}`} onClick={() => onSelect(profile.profile.id)}>{t(language, "advanced.select")}</button>
            <button type="button" aria-label={`${t(language, "data.exportProfile")} ${profile.profile.name}`} onClick={() => onExport(profile)}><Upload size={15} />{t(language, "data.exportProfile")}</button>
            <button type="button" aria-label={`${t(language, "data.duplicateProfile")} ${profile.profile.name}`} onClick={() => onCreate(profile.profile.id)}><Plus size={15} />{t(language, "data.duplicateProfile")}</button>
            <button className="is-danger" type="button" aria-label={`${t(language, "data.deleteProfile")} ${profile.profile.name}`} onClick={() => onDelete(profile)}><Trash2 size={15} />{t(language, "data.deleteProfile")}</button>
          </div>
        </article>;
      })}
    </div>
  </section>;
}
