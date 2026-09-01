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

export type TriggerActions = Record<ActionTrigger, ButtonAction[]>;

export interface ModelButton {
  id: string;
  label: string;
}

export interface ButtonGroup {
  id: string;
  columns: number;
  buttons: ModelButton[];
}

export interface ModelLayout {
  id: string;
  name: string;
  groups: ButtonGroup[];
}

export interface DirectInputSource {
  type: "direct";
  id: string;
  keys: Record<string, number>;
}

export interface ContactInputSource {
  type: "contact_matrix";
  id: string;
  pins: number[];
  keys: Record<string, [number, number]>;
}

export interface FeatureSwitchInputSource {
  type: "feature_switch";
  id: string;
  name: string;
  gpio: number;
  buttons: string[];
}

export type InputSource = DirectInputSource | ContactInputSource | FeatureSwitchInputSource;

export interface HardwareProfile {
  id: string;
  name: string;
  board_profile_id: string;
  debounce_ms: number;
  ssd1306?: {
    sda: number;
    scl: number;
    control_panel?: {
      type: "ec11_confirm_back";
      confirm: number;
      encoder_press: number;
      encoder_a: number;
      encoder_b: number;
      back: number;
    };
  };
  sh1106?: {
    sda: number;
    scl: number;
    control_panel?: {
      type: "ec11_confirm_back";
      confirm: number;
      encoder_press: number;
      encoder_a: number;
      encoder_b: number;
      back: number;
    };
  };
  inputs: InputSource[];
}

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
  product_config?: ProductDeviceConfig | null;
}

export interface ProductDeviceConfig {
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
  schema_version: 3;
  editor_profile: string | null;
  language: Language;
  devices: Record<string, DeviceRecord>;
}

export interface EditorSettingsPatch {
  schema_version: 3;
  editor_profile: string | null;
  language: Language;
}

export type PhysicalInput =
  | { type: "direct"; gpio: number }
  | { type: "contact"; source: number; pin_a: number; pin_b: number };

export interface LearningTarget {
  deviceId: string;
  deviceProfileId: string;
  hardwareProfileId: string;
  editingRevision: number;
  firmwareRevision: number;
  pins: number[];
}

export interface RuntimeActivity {
  code: string;
  params: Record<string, string>;
  detail: string | null;
  input: PhysicalInput | null;
  pressed: boolean | null;
  learningTarget: LearningTarget | null;
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
  | "validating"
  | "valid"
  | "invalid_identity"
  | "duplicate_identity";
export type AssignmentDimension = "unassigned" | "valid" | "invalid_assignment";
export type RuntimeDimension =
  | "inactive"
  | "configuring"
  | "learning"
  | "ready"
  | "runtime_error";

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
  productDefinition?: {
    schema_version: 1;
    product: {
      display_name: string;
      family_id: string;
      variant_id: string;
      hardware_revision: number;
      product_version_id: string;
      capabilities: string[];
    };
    layout: ModelLayout;
    hardware_profile: HardwareProfile;
  } | null;
  productConfig?: ProductDeviceConfig | null;
  firmwareProtocol?: number | null;
  capabilities: number[];
  runtimeAssignment: RuntimeAssignment | null;
  latestError: RuntimeActivity | null;
  learning: LearningTarget | null;
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
  editorProfile: string | null;
  boardProfiles: BoardProfileSummary[];
  devices: DeviceStatus[];
  candidates: CandidateStatus[];
  language: Language;
  homeMetrics: HomeMetricsSnapshot | null;
  usage?: UsageView | null;
}

export type UsageState =
  | "disabled"
  | "connecting"
  | "ready"
  | "stale"
  | "auth_error"
  | "network_error"
  | "parse_error"
  | "api_error";

export interface UsageSnapshot {
  state: UsageState;
  hasData: boolean;
  costMicros: number;
  todayTokens: number;
  tpm: number;
  updatedAtMs: number | null;
}

export interface UsageSettingsSummary {
  enabled: boolean;
  baseUrl: string;
  email: string;
  intervalSeconds: number;
  passwordRequired: boolean;
}

export interface UsageView {
  settings: UsageSettingsSummary;
  snapshot: UsageSnapshot;
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
