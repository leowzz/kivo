import type {
  HardwareProfile,
  ModelLayout,
  ProductDefinition,
} from "../shared/types";
export type * from "../shared/types";

export type Language = "zh-CN" | "en-US";
export type EventLevel = "info" | "warning" | "error";

export type ButtonAction =
  | { type: "paste"; text: string }
  | { type: "hotkey"; keys: string[] }
  | { type: "delay"; duration_ms: number }
  | { type: "media"; command: MediaCommand }
  | { type: "open"; target: string };

export type MediaCommand =
  | "play_pause"
  | "previous_track"
  | "next_track"
  | "stop"
  | "volume_up"
  | "volume_down"
  | "mute";

export type ActionTrigger = "press" | "release" | "long_press" | "double_press";

export const DEFAULT_LONG_PRESS_MS = 500;
export const DEFAULT_DOUBLE_PRESS_MS = 300;

export interface TriggerSettings {
  long_press_ms: number;
  double_press_ms: number;
}

export type TriggerActions = Record<ActionTrigger, ButtonAction[]> & {
  note?: string;
};

export interface DeviceProfile {
  schema_version: 3;
  profile: ModelLayout;
  snapshot_metadata?: SnapshotMetadata | null;
  trigger_settings: TriggerSettings;
  hardware_profiles: HardwareProfile[];
  actions: Record<string, TriggerActions>;
}

export interface RuntimeAssignment {
  device_profile_id: string;
  hardware_profile_id: string;
}

export interface DeviceRecord {
  device_id: string;
  name: string;
  board_profile_id: string;
  runtime_assignment: RuntimeAssignment | null;
  product_configuration_id?: string | null;
}

export interface ProductConfigurationProfile {
  id: string;
  name: string;
  product_version_id: string;
  snapshot_metadata?: SnapshotMetadata | null;
  trigger_settings: TriggerSettings;
  actions: Record<string, TriggerActions>;
}

export interface SnapshotMetadata {
  created_at: number;
  source_device_id?: string | null;
  source_device_name?: string | null;
}

export interface SettingsDocument {
  schema_version: 4;
  editor_profile: string | null;
  language: Language;
  devices: Record<string, DeviceRecord>;
  product_configurations: Record<string, ProductConfigurationProfile>;
}

export interface EditorSettingsPatch {
  schema_version: 4;
  editor_profile: string | null;
  language: Language;
}

export type PhysicalInput =
  | { type: "direct"; gpio: number }
  | { type: "contact"; source: number; pin_a: number; pin_b: number };

export interface RuntimeActivity {
  code: string;
  params: Record<string, string>;
  detail: string | null;
  input: PhysicalInput | null;
  pressed: boolean | null;
}

export interface RuntimeEvent extends RuntimeActivity {
  timestampMs: number;
  level: EventLevel;
  deviceId: string;
  rawSerial: string;
  controllerFamilyId: string;
  boardProfileId: string;
  port: string | null;
  deviceProfileId: string | null;
  hardwareProfileId: string | null;
  homeUpdate: HomeMetricsSnapshot | null;
}

export interface BoardProfileSummary {
  id: string;
  controllerFamilyId: string;
  displayName: string;
  runtimeUsb: string;
  bootloaderUsb: string | null;
  safePins: number[];
  supportsOled?: boolean;
}

export type ConnectionDimension = "online" | "offline";
export type DeviceMode = "runtime" | "bootloader";
export type IdentityDimension =
  "validating" | "valid" | "invalid_identity" | "duplicate_identity";
export type AssignmentDimension = "unassigned" | "valid" | "invalid_assignment";
export type RuntimeDimension =
  "inactive" | "configuring" | "ready" | "runtime_error";

export interface DeviceStatus {
  deviceId: string;
  name: string;
  connection: ConnectionDimension;
  mode: DeviceMode | null;
  identity: IdentityDimension;
  assignment: AssignmentDimension;
  runtime: RuntimeDimension;
  hardwareSerial: string;
  port: string | null;
  controllerFamilyId: string;
  boardProfileId: string;
  firmwareBuildId: string | null;
  productVersionId?: string | null;
  productDefinition?: ProductDefinition | null;
  productConfigurationId?: string | null;
  productConfig?: ProductConfigurationProfile | null;
  firmwareProtocol?: number | null;
  capabilities: number[];
  runtimeAssignment: RuntimeAssignment | null;
  latestError: RuntimeActivity | null;
}

export interface CandidateStatus {
  key: string;
  deviceId: string | null;
  mode: DeviceMode;
  identity: IdentityDimension;
  issue: CandidateIssue;
  rawSerial: string | null;
  port: string | null;
  controllerFamilyId: string;
  boardProfileId: string;
  latestError: string | null;
}

export type CandidateIssue =
  | "validating"
  | "firmware_not_responding"
  | "firmware_incompatible"
  | "bootloader"
  | "port_unavailable"
  | "invalid_identity"
  | "duplicate_identity"
  | "unknown";

export type CreateDeviceProfileRequest =
  | { kind: "clone"; name: string; source_profile_id: string }
  | { kind: "blank"; name: string; board_profile_id: string };

export interface ButtonMetric {
  buttonId: string;
  presses: number;
}

export interface ButtonDayMetric {
  buttonId: string;
  day: string;
  presses: number;
}

export interface ActivityLog {
  timestampMs: number;
  kind: string;
  message: string;
  deviceId: string;
  deviceName: string;
  deviceProfileId: string;
  hardwareProfileId: string;
  buttonId: string | null;
  actionKind?: string | null;
  detail?: string | null;
}

export interface HomeMetricsSnapshot {
  totalPresses: number;
  todayPresses: number;
  activeButtonCount: number;
  topButton: ButtonMetric | null;
  heatmap: ButtonDayMetric[];
  logs: ActivityLog[];
}

export interface AppSnapshot {
  deviceProfiles: DeviceProfile[];
  productConfigurations: ProductConfigurationProfile[];
  editorProfile: string | null;
  boardProfiles: BoardProfileSummary[];
  devices: DeviceStatus[];
  candidates: CandidateStatus[];
  language: Language;
  homeMetrics: HomeMetricsSnapshot | null;
}


export interface StartupFailure {
  code: string;
  detail: string;
}

export interface ImportPreview {
  profileId: string;
  profileName: string;
  buttonCount: number;
  hardwareBindingCount: number;
  actionCount: number;
  replacesExisting: boolean;
}

export interface BackupPreview {
  kind?: "product_devices" | "full";
  profileCount: number;
  buttonCount: number;
  hardwareBindingCount: number;
  actionCount: number;
  deviceCount: number;
  assignmentCount: number;
  metricRowCount: number;
  activityCount: number;
}
