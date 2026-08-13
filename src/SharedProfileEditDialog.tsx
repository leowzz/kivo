import { Info, X } from "lucide-react";
import { t } from "./i18n";
import type { Language } from "./types";

interface SharedProfileEditDialogProps {
  language: Language;
  deviceName: string;
  profileName: string;
  affectedDeviceCount: number;
  allowDeviceScope: boolean;
  onChoose(scope: "device" | "shared"): void;
  onCancel(): void;
}

export function SharedProfileEditDialog({
  language,
  deviceName,
  profileName,
  affectedDeviceCount,
  allowDeviceScope,
  onChoose,
  onCancel,
}: SharedProfileEditDialogProps) {
  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => {
    if (event.target === event.currentTarget) onCancel();
  }}>
    <section className="shared-profile-edit-dialog" role="dialog" aria-modal="true" aria-labelledby="shared-profile-edit-title">
      <header className="shared-profile-edit-header">
        <div>
          <Info size={18} aria-hidden="true" />
          <h2 id="shared-profile-edit-title">{t(language, "sharedEdit.title")}</h2>
        </div>
        <button className="icon-button" type="button" aria-label={t(language, "common.close")} title={t(language, "common.close")} onClick={onCancel}>
          <X size={17} />
        </button>
      </header>
      <div className="shared-profile-edit-body">
        <p>{t(language, "sharedEdit.body", { device: deviceName, profile: profileName })}</p>
        <p className="shared-profile-edit-warning">{t(language, "sharedEdit.warning")}</p>
      </div>
      <footer className="shared-profile-edit-actions">
        <button type="button" onClick={onCancel}>{t(language, "common.cancel")}</button>
        {allowDeviceScope && <button className="primary-button" type="button" onClick={() => onChoose("device")}>
          {t(language, "sharedEdit.device")}
        </button>}
        <button type="button" onClick={() => onChoose("shared")}>
          {t(language, "sharedEdit.shared", { count: affectedDeviceCount })}
        </button>
      </footer>
    </section>
  </div>;
}
