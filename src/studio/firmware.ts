import { Channel, invoke } from "@tauri-apps/api/core";

export interface FirmwareDevice {
  id: string;
  name: string;
  serial: string;
  boardProfileId: string;
}

export interface FirmwareResult {
  path: string;
  bytes: number;
  sha256: string;
  productVersionId?: string;
  buildId?: string;
  warning?: string;
  backupPath?: string;
}

export interface FirmwareProgress {
  phase: string;
  backupPath?: string;
}

export const firmwarePhases: Record<string, string> = {
  checking: "正在核对设备产品",
  connecting: "正在进入刷写模式",
  reading: "正在读取原有固件",
  rebooting: "正在重启设备",
  writing: "正在刷入固件，请勿断开设备",
  verifying: "正在验证设备固件",
  building: "正在构建 I/O 测试固件",
  backup_saved: "原固件备份已保存",
};

export type FirmwareOperation =
  "inspect" | "backup" | "flash" | "install_test" | "status";

export function invokeFirmware<T = FirmwareResult>(
  deviceId: string,
  operation: FirmwareOperation,
  onProgress: (event: FirmwareProgress) => void,
  path?: string,
  sha256?: string,
) {
  const progress = new Channel<FirmwareProgress>();
  progress.onmessage = onProgress;
  return invoke<T>("studio_firmware_operation", {
    operation,
    deviceId,
    path: path ?? null,
    sha256: sha256 ?? null,
    progress,
  });
}

export function firmwareFailureText(reason: unknown): string {
  if (typeof reason === "object" && reason && "code" in reason) {
    if (reason.code === "studio_repository_not_configured")
      return "请先在产品定义中选择 Kivo 仓库。";
    if (reason.code === "studio_firmware_busy") return "已有固件操作正在进行。";
    if ("detail" in reason && reason.detail) return String(reason.detail);
    return String(reason.code);
  }
  return String(reason);
}
