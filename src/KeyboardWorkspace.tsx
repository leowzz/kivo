import { Cable, Settings2, Usb } from "lucide-react";
import { ActionEditor } from "./ActionEditor";
import { Keypad } from "./Keypad";
import { t } from "./i18n";
import type { DeviceProfile, DeviceStatus, Language, TriggerActions } from "./types";

export interface KeyboardWorkspaceProps {
  language: Language;
  device: DeviceStatus | null;
  profile: DeviceProfile | undefined;
  hasCandidates: boolean;
  selectedButtonId: string | null;
  pressedButtonIds: Set<string>;
  onSelectButton(buttonId: string): void;
  onChangeActions(buttonId: string, actions: TriggerActions): void;
  onRenameButton(buttonId: string, label: string): void;
  onOpenSetup(deviceId: string | null): void;
}

function emptyActions(): TriggerActions {
  return { press: [], release: [], long_press: [], double_press: [] };
}

export function KeyboardWorkspace({
  language,
  device,
  profile,
  hasCandidates,
  selectedButtonId,
  pressedButtonIds,
  onSelectButton,
  onChangeActions,
  onRenameButton,
  onOpenSetup,
}: KeyboardWorkspaceProps) {
  if (!device) {
    return <section className="keyboard-empty-state" aria-labelledby="keyboard-empty-title">
      <Usb size={28} aria-hidden="true" />
      <h2 id="keyboard-empty-title">{t(language, "workspace.connectTitle")}</h2>
      <p>{t(language, hasCandidates ? "workspace.connectCandidate" : "workspace.connectBody")}</p>
      <button className="primary-button" type="button" onClick={() => onOpenSetup(null)}>{t(language, "device.connect")}</button>
    </section>;
  }

  if (device.assignment !== "valid" || !profile) {
    const repair = device.assignment === "invalid_assignment";
    return <section className="keyboard-empty-state" aria-labelledby="keyboard-setup-title">
      <Settings2 size={28} aria-hidden="true" />
      <h2 id="keyboard-setup-title">{t(language, repair ? "workspace.repairTitle" : "workspace.setupTitle")}</h2>
      <p>{t(language, repair ? "workspace.repairBody" : "workspace.setupBody")}</p>
      <button className="primary-button" type="button" onClick={() => onOpenSetup(device.deviceId)}>{t(language, repair ? "workspace.repair" : "setup.continue")}</button>
    </section>;
  }

  const buttons = profile.profile.groups.flatMap((group) => group.buttons);
  const selectedButton = buttons.find((button) => button.id === selectedButtonId) ?? null;
  const selectedActions = selectedButton ? profile.actions[selectedButton.id] ?? emptyActions() : emptyActions();

  return <div className="keyboard-workspace">
    <header className="keyboard-workspace-heading">
      <div><span>{device.name}</span><h2>{profile.profile.name}</h2></div>
      {device.connection === "offline" && <span className="keyboard-connection is-offline"><Cable size={15} />{t(language, "workspace.disconnected")}</span>}
    </header>
    <div className="keypad-stage">
      <Keypad
        layout={profile.profile}
        actions={profile.actions}
        selectedButtonId={selectedButtonId}
        pressedButtonIds={pressedButtonIds}
        actionCountLabel={(count) => t(language, "model.actionCount", { count })}
        unconfiguredLabel={t(language, "workspace.unconfigured")}
        onSelect={onSelectButton}
      />
    </div>
    <ActionEditor
      language={language}
      button={selectedButton}
      actions={selectedActions}
      onChange={(actions) => selectedButton && onChangeActions(selectedButton.id, actions)}
      onRename={onRenameButton}
    />
  </div>;
}
