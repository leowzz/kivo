import { primaryDeviceLabel } from "./deviceStatus";
import { t } from "./i18n";
import type { DeviceStatus, Language } from "./types";

export interface DeviceSwitcherProps {
  devices: readonly DeviceStatus[];
  selectedDeviceId: string | null;
  language: Language;
  onChange(deviceId: string): void;
}

export function DeviceSwitcher({ devices, selectedDeviceId, language, onChange }: DeviceSwitcherProps) {
  if (devices.length === 0) return <span className="device-switcher-empty">{t(language, "device.connect")}</span>;

  return (
    <label className="device-switcher">
      <span>{t(language, "device.current")}</span>
      <select
        aria-label={t(language, "device.current")}
        value={selectedDeviceId ?? ""}
        onChange={(event) => onChange(event.target.value)}
      >
        {devices.map((device) => (
          <option key={device.deviceId} value={device.deviceId}>
            {device.name} {" · "}{t(language, `devices.status.${statusKey(device)}` as Parameters<typeof t>[1])}
          </option>
        ))}
      </select>
    </label>
  );
}

function statusKey(device: DeviceStatus) {
  const labels: Record<string, string> = {
    设备身份冲突: "identityConflict",
    设备身份无效: "identityInvalid",
    分配需要修复: "assignmentInvalid",
    运行错误: "runtimeError",
    引导加载模式: "bootloader",
    未分配: "unassigned",
    离线: "offline",
    正在验证: "validating",
    正在配置: "configuring",
    正在学习: "learning",
    就绪: "ready",
    未运行: "inactive",
  };
  return labels[primaryDeviceLabel(device)];
}
