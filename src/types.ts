export type ConnectionState = "searching" | "connected";
export type EventLevel = "info" | "warning" | "error";

export interface ConnectionStatus {
  state: ConnectionState;
  port: string | null;
}

export interface AppSnapshot {
  buttons: Record<number, string>;
  configPath: string;
  connection: ConnectionStatus;
  configError: string | null;
}

export interface RuntimeEvent {
  timestampMs: number;
  level: EventLevel;
  message: string;
  connection: ConnectionStatus;
}
