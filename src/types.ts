export type Language = "zh-CN" | "en-US";
export type EventLevel = "info" | "warning" | "error";

export type ButtonAction =
  | { type: "paste"; text: string }
  | { type: "hotkey"; keys: string[] };

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

export type InputSource = DirectInputSource | ContactInputSource;

export interface HardwareProfile {
  id: string;
  name: string;
  board_profile_id: string;
  debounce_ms: number;
  inputs: InputSource[];
}

export interface DeviceProfile {
  schema_version: 2;
  profile: ModelLayout;
  hardware_profiles: HardwareProfile[];
  actions: Record<string, ButtonAction[]>;
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
}

export interface SettingsDocument {
  schema_version: 2;
  editor_profile: string | null;
  language: Language;
  devices: Record<string, DeviceRecord>;
}

export interface EditorSettingsPatch {
  schema_version: 2;
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
  profileCount: number;
  buttonCount: number;
  hardwareBindingCount: number;
  actionCount: number;
  deviceCount: number;
  assignmentCount: number;
  metricRowCount: number;
  activityCount: number;
}
