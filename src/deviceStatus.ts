import { t } from "./i18n";
import type {
  CandidateStatus,
  DeviceStatus,
  HardwareProfile,
  Language,
} from "./types";

export type DeviceFilter = "all" | "attention" | "ready" | "offline";

export interface DeviceSummary {
  ready: number;
  attention: number;
  offline: number;
  progress: number;
}

type DerivedState = Exclude<DeviceFilter, "all"> | "progress" | "inactive";

const availabilityRank: Record<DerivedState, number> = {
  ready: 0,
  progress: 1,
  attention: 2,
  inactive: 3,
  offline: 4,
};

function hasIdentityProblem(device: DeviceStatus) {
  return device.identity === "invalid_identity" || device.identity === "duplicate_identity";
}

function derivedState(device: DeviceStatus): DerivedState {
  if (
    hasIdentityProblem(device) ||
    device.assignment === "invalid_assignment" ||
    device.runtime === "runtime_error" ||
    device.mode === "bootloader" ||
    (device.connection === "online" && device.assignment === "unassigned")
  ) {
    return "attention";
  }
  if (device.connection === "offline") {
    return "offline";
  }
  if (
    device.identity === "validating" ||
    device.runtime === "configuring" ||
    device.runtime === "learning"
  ) {
    return "progress";
  }
  if (device.runtime === "ready") {
    return "ready";
  }
  return "inactive";
}

export function compareDeviceAvailability(
  left: DeviceStatus,
  right: DeviceStatus,
): number {
  return availabilityRank[derivedState(left)] - availabilityRank[derivedState(right)];
}

export function primaryDeviceLabel(device: DeviceStatus): string {
  if (device.identity === "duplicate_identity") return "设备身份冲突";
  if (device.identity === "invalid_identity") return "设备身份无效";
  if (device.assignment === "invalid_assignment") return "分配需要修复";
  if (device.runtime === "runtime_error") return "运行错误";
  if (device.mode === "bootloader") return "引导加载模式";
  if (device.connection === "online" && device.assignment === "unassigned") return "未分配";
  if (device.connection === "offline") return "离线";
  if (device.identity === "validating") return "正在验证";
  if (device.runtime === "configuring") return "正在配置";
  if (device.runtime === "learning") return "正在学习";
  if (device.runtime === "ready") return "就绪";
  return "未运行";
}

export function deviceSummary(devices: readonly DeviceStatus[]): DeviceSummary {
  const summary: DeviceSummary = { ready: 0, attention: 0, offline: 0, progress: 0 };
  for (const device of devices) {
    const state = derivedState(device);
    if (state !== "inactive") summary[state] += 1;
  }
  return summary;
}

export function matchesDeviceFilter(
  device: DeviceStatus,
  filter: DeviceFilter,
  query: string,
): boolean {
  if (filter !== "all" && derivedState(device) !== filter) return false;
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return true;
  return [device.name, device.hardwareSerial, device.boardProfileId, device.port ?? ""].some((value) =>
    value.toLocaleLowerCase().includes(normalized),
  );
}

export function compatibleHardwareProfiles(
  profiles: readonly HardwareProfile[],
  boardProfileId: string,
): HardwareProfile[] {
  return profiles.filter(({ board_profile_id }) => board_profile_id === boardProfileId);
}

export function editablePins(boardSafePins: readonly number[], capabilities: readonly number[] | null): number[] {
  if (capabilities === null) return [...boardSafePins];
  const reported = new Set(capabilities);
  return boardSafePins.filter((pin) => reported.has(pin));
}

export function serialSuffix(serial: string): string {
  return serial.slice(-6);
}

export function candidateDisplayLabel(
  candidate: Pick<CandidateStatus, "rawSerial">,
  ordinal: number,
  language: Language,
): string {
  return candidate.rawSerial
    ? serialSuffix(candidate.rawSerial)
    : t(language, "devices.pendingCandidate", { number: ordinal });
}
