import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { ProfileManager } from "./ProfileManager";
import type { DeviceProfile, DeviceStatus } from "./types";

const profile = (id: string, name: string): DeviceProfile => ({
  schema_version: 3, profile: { id, name, groups: [] }, trigger_settings: { long_press_ms: 500, double_press_ms: 300 }, hardware_profiles: [], actions: {},
});
const device: DeviceStatus = {
  deviceId: "keyboard-1", name: "Desk", connection: "online", mode: "runtime", identity: "valid", assignment: "valid", runtime: "ready", hardwareSerial: "serial", port: "/dev/tty", controllerFamilyId: "rp2040", boardProfileId: "rp", firmwareBuildId: "build", capabilities: [], runtimeAssignment: { device_profile_id: "profile-a", hardware_profile_id: "hardware-a" }, latestError: null, learning: null,
};

test("owns profile lifecycle controls and reports usage and editor selection", async () => {
  const user = userEvent.setup();
  const onCreate = vi.fn();
  const onSelect = vi.fn();
  const onImport = vi.fn();
  const onExport = vi.fn();
  const onDelete = vi.fn();
  const profileA = profile("profile-a", "Primary");
  const profileB = profile("profile-b", "Copy");
  render(<ProfileManager language="zh-CN" profiles={[profileA, profileB]} editorProfileId="profile-a" devices={[device]} onCreate={onCreate} onSelect={onSelect} onImport={onImport} onExport={onExport} onDelete={onDelete} />);

  expect(screen.getByText("当前编辑配置")).toBeInTheDocument();
  expect(screen.getByText("已被 1 台设备使用")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "选择 Copy" }));
  await user.click(screen.getByRole("button", { name: "导入配置" }));
  await user.click(screen.getByRole("button", { name: "导出 Primary" }));
  await user.click(screen.getByRole("button", { name: "复制 Primary" }));
  await user.click(screen.getByRole("button", { name: "删除 Primary" }));
  expect(onSelect).toHaveBeenCalledWith("profile-b");
  expect(onImport).toHaveBeenCalledTimes(1);
  expect(onExport).toHaveBeenCalledWith(profileA);
  expect(onCreate).toHaveBeenCalledWith("profile-a");
  expect(onDelete).toHaveBeenCalledWith(profileA);
});
