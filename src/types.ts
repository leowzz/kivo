export type Language = "zh-CN" | "en-US";
export type ConnectionState = "searching" | "connected";
export type EventLevel = "info" | "warning" | "error";

export interface ConnectionStatus {
  state: ConnectionState;
  port: string | null;
}

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

export interface HardwareConfig {
  controller: string;
  debounce_ms: number;
  inputs: InputSource[];
}

export interface ModelConfig {
  schema_version: number;
  model: ModelLayout;
  hardware: HardwareConfig;
  actions: Record<string, ButtonAction[]>;
  legacy?: { unresolved_gpio_text: Record<number, string> };
}

export interface SettingsDocument {
  schema_version: number;
  active_model: string | null;
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
  connection: ConnectionStatus;
}

export interface LearningSession {
  revision: number;
  pins: number[];
}

export interface AppSnapshot {
  models: ModelConfig[];
  activeModel: string | null;
  language: Language;
  supportedGpios: number[];
  connection: ConnectionStatus;
  runtimeError: RuntimeActivity | null;
  learning: LearningSession | null;
}

export interface ImportPreview {
  modelId: string;
  modelName: string;
  buttonCount: number;
  hardwareBindingCount: number;
  actionCount: number;
  replacesExisting: boolean;
}

export interface BackupPreview {
  modelCount: number;
  buttonCount: number;
  hardwareBindingCount: number;
  actionCount: number;
}
