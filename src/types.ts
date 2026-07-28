export type ConnectionState = "searching" | "connected";
export type EventLevel = "info" | "warning" | "error";

export interface ConnectionStatus {
  state: ConnectionState;
  port: string | null;
}

export type ConfigMode = "io" | "behavior";

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

export interface AppSnapshot {
  models: ModelLayout[];
  activeModel: string;
  ioMaps: Record<string, Record<number, string>>;
  actions: Record<string, ButtonAction>;
  supportedGpios: number[];
  configPath: string;
  connection: ConnectionStatus;
  configError: string | null;
}

export interface RuntimeEvent {
  timestampMs: number;
  level: EventLevel;
  message: string;
  connection: ConnectionStatus;
  gpio: number | null;
  pressed: boolean | null;
}
