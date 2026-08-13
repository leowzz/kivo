import { useEffect, useMemo, useState } from "react";
import { ConfigurationSettingsDialog } from "./ConfigurationSettingsDialog";
import { HardwareMapping } from "./HardwareMapping";
import { LayoutEditor } from "./LayoutEditor";
import { ProfileManager } from "./ProfileManager";
import { t } from "./i18n";
import type { BoardProfileSummary, DeviceProfile, DeviceStatus, Language } from "./types";

export type AdvancedSection = "profiles" | "layout" | "io" | "technical";
type ProfileMutation = (profile: DeviceProfile) => DeviceProfile;

export interface AdvancedSettingsProps {
  initialSection?: AdvancedSection;
  initialHardwareProfileId?: string;
  language: Language;
  profiles: DeviceProfile[];
  editorProfileId: string | null;
  devices: DeviceStatus[];
  selectedDevice: DeviceStatus | null;
  boardProfiles: BoardProfileSummary[];
  onCreate(sourceProfileId?: string): void;
  onSelectProfile(profileId: string): void;
  onImport(): void;
  onExport(profile: DeviceProfile): void;
  onDelete(profile: DeviceProfile): void;
  onRequestProfileMutation(mutation: ProfileMutation): void;
  onDuplicateForDevice(profile: DeviceProfile, name: string): Promise<void> | void;
  onHardwareSelectionChange(hardwareProfileId: string | null, deviceId: string | null): void;
  onSelectButton?(buttonId: string): void;
  onBeginLearning(hardwareProfileId: string, deviceId: string, pins: number[]): void;
  onEndLearning(deviceId: string): void;
}

const sections: AdvancedSection[] = ["profiles", "layout", "io", "technical"];

export function AdvancedSettings(props: AdvancedSettingsProps) {
  const [section, setSection] = useState<AdvancedSection>(props.initialSection ?? "profiles");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [selectedButtonId, setSelectedButtonId] = useState<string | null>(null);
  const target = useMemo(() => {
    const assignedId = props.selectedDevice?.runtimeAssignment?.device_profile_id;
    return props.profiles.find((profile) => profile.profile.id === assignedId) ?? props.profiles.find((profile) => profile.profile.id === props.editorProfileId) ?? null;
  }, [props.editorProfileId, props.profiles, props.selectedDevice?.runtimeAssignment?.device_profile_id]);
  const hardwareId = props.selectedDevice?.runtimeAssignment?.hardware_profile_id ?? props.initialHardwareProfileId;
  const sharedCount = target ? props.devices.filter((device) => device.runtimeAssignment?.device_profile_id === target.profile.id).length : 0;
  const offline = !props.selectedDevice?.runtimeAssignment;
  const canDuplicateForDevice = Boolean(target && props.selectedDevice &&
    props.selectedDevice.connection === "online" &&
    props.selectedDevice.mode === "runtime" &&
    props.selectedDevice.identity === "valid" &&
    props.selectedDevice.runtimeAssignment?.device_profile_id === target.profile.id);
  const labels: Record<AdvancedSection, string> = { profiles: t(props.language, "advanced.profiles"), layout: t(props.language, "advanced.layout"), io: t(props.language, "advanced.io"), technical: t(props.language, "advanced.technical") };

  useEffect(() => {
    if (props.initialSection) setSection(props.initialSection);
  }, [props.initialSection]);

  return <div className={canDuplicateForDevice ? "advanced-settings" : "advanced-settings without-device-duplicate"}>
    <header className="content-heading"><div><h2>{t(props.language, "settings.advanced")}</h2></div></header>
    <div className="advanced-tabs" role="tablist">{sections.map((item) => <button key={item} type="button" role="tab" aria-selected={section === item} onClick={() => setSection(item)}>{labels[item]}</button>)}</div>
    {section === "profiles" && <div role="tabpanel" aria-label={labels.profiles}><ProfileManager language={props.language} profiles={props.profiles} editorProfileId={props.editorProfileId} devices={props.devices} onCreate={props.onCreate} onSelect={props.onSelectProfile} onImport={props.onImport} onExport={props.onExport} onDelete={props.onDelete} /></div>}
    {(section === "layout" || section === "io") && !target && <p className="advanced-empty">{t(props.language, "advanced.selectProfile")}</p>}
    {section === "layout" && target && <section className="advanced-editor" role="tabpanel" aria-label={labels.layout}><div className="advanced-editor-toolbar"><p>{offline && t(props.language, "advanced.offline")}</p><button type="button" onClick={() => setSettingsOpen(true)}>{t(props.language, "devices.configurationSettings")}</button></div><LayoutEditor layout={target.profile} language={props.language} onChange={(nextLayout) => props.onRequestProfileMutation((current) => ({ ...current, profile: nextLayout }))} /></section>}
    {section === "io" && target && <section className="advanced-editor" role="tabpanel" aria-label={labels.io}>{offline && <p className="advanced-offline-note">{t(props.language, "advanced.offline")}</p>}<HardwareMapping language={props.language} layout={target.profile} hardwareProfiles={target.hardware_profiles} boardProfiles={props.boardProfiles} devices={offline ? [] : props.devices} learning={props.selectedDevice?.learning ?? null} initialHardwareProfileId={hardwareId} initialDeviceId={props.selectedDevice?.deviceId ?? null} selectedButtonId={selectedButtonId} onSelectButton={(buttonId) => { setSelectedButtonId(buttonId); props.onSelectButton?.(buttonId); }} onChange={(nextHardwareProfiles) => props.onRequestProfileMutation((current) => ({ ...current, hardware_profiles: nextHardwareProfiles }))} onSelectionChange={props.onHardwareSelectionChange} onBeginLearning={props.onBeginLearning} onEndLearning={props.onEndLearning} /></section>}
    {section === "technical" && <section className="advanced-technical" role="tabpanel" aria-label={labels.technical}>{props.selectedDevice?.runtimeAssignment ? <dl><dt>{t(props.language, "devices.id")}</dt><dd>{props.selectedDevice.deviceId}</dd><dt>{t(props.language, "devices.serial")}</dt><dd>{props.selectedDevice.hardwareSerial}</dd><dt>{t(props.language, "devices.board")}</dt><dd>{props.boardProfiles.find((item) => item.id === props.selectedDevice?.boardProfileId)?.displayName ?? props.selectedDevice.boardProfileId}</dd><dt>{t(props.language, "devices.firmware")}</dt><dd>{props.selectedDevice.firmwareBuildId ?? "-"}</dd><dt>{t(props.language, "advanced.protocol")}</dt><dd>{props.selectedDevice.firmwareProtocol ?? "-"}</dd><dt>{t(props.language, "devices.port")}</dt><dd>{props.selectedDevice.port ?? "-"}</dd></dl> : <p className="advanced-empty">{t(props.language, "advanced.noDevice")}</p>}</section>}
    {target && <ConfigurationSettingsDialog open={settingsOpen} profile={target} sharedDeviceCount={sharedCount} language={props.language} onSave={(settings) => { props.onRequestProfileMutation((current) => ({ ...current, trigger_settings: settings })); setSettingsOpen(false); }} onDuplicate={(name) => canDuplicateForDevice ? props.onDuplicateForDevice(target, name) : undefined} onCancel={() => setSettingsOpen(false)} />}
  </div>;
}
