import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { AdvancedSettings } from "./AdvancedSettings";
import type { BoardProfileSummary, DeviceProfile, DeviceStatus } from "./types";

const profile: DeviceProfile = {
  schema_version: 3, profile: { id: "profile-a", name: "Primary", groups: [{ id: "main", columns: 1, buttons: [{ id: "key", label: "Key" }] }] }, trigger_settings: { long_press_ms: 500, double_press_ms: 300 }, hardware_profiles: [{ id: "hardware-a", name: "Hardware", board_profile_id: "rp", debounce_ms: 30, inputs: [{ type: "direct", id: "direct", keys: {} }] }], actions: {},
};
const board: BoardProfileSummary = { id: "rp", controllerFamilyId: "rp2040", displayName: "RP2040", runtimeUsb: "2e8a:102e", bootloaderUsb: "2e8a:0003", safePins: [0, 1] };
const device: DeviceStatus = { deviceId: "device-a", name: "Desk", connection: "online", mode: "runtime", identity: "valid", assignment: "valid", runtime: "ready", hardwareSerial: "serial-123", port: "/dev/cu.usb", controllerFamilyId: "rp2040", boardProfileId: "rp", firmwareBuildId: "fw-1", capabilities: [0, 1], runtimeAssignment: { device_profile_id: "profile-a", hardware_profile_id: "hardware-a" }, latestError: null, learning: null };

test("keeps advanced sections in a fixed order and shows selected device details", async () => {
  const user = userEvent.setup();
  render(<AdvancedSettings language="zh-CN" profiles={[profile]} editorProfileId="profile-a" devices={[device]} selectedDevice={device} boardProfiles={[board]} onCreate={vi.fn()} onSelectProfile={vi.fn()} onImport={vi.fn()} onExport={vi.fn()} onDelete={vi.fn()} onRequestProfileMutation={vi.fn()} onDuplicateForDevice={vi.fn()} onBeginLearning={vi.fn()} onEndLearning={vi.fn()} />);

  expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["配置文件", "按键布局", "I/O 映射", "技术信息"]);
  await user.click(screen.getByRole("tab", { name: "技术信息" }));
  expect(screen.getByText("device-a")).toBeInTheDocument();
  expect(screen.getByText("serial-123")).toBeInTheDocument();
  expect(screen.getByText("RP2040")).toBeInTheDocument();
  expect(screen.getByText("fw-1")).toBeInTheDocument();
  expect(screen.getByText("/dev/cu.usb")).toBeInTheDocument();
});

test("uses the gated mutation callback for layout edits", async () => {
  const user = userEvent.setup();
  const onRequestProfileMutation = vi.fn();
  render(<AdvancedSettings language="zh-CN" profiles={[profile]} editorProfileId="profile-a" devices={[device]} selectedDevice={device} boardProfiles={[board]} onCreate={vi.fn()} onSelectProfile={vi.fn()} onImport={vi.fn()} onExport={vi.fn()} onDelete={vi.fn()} onRequestProfileMutation={onRequestProfileMutation} onDuplicateForDevice={vi.fn()} onBeginLearning={vi.fn()} onEndLearning={vi.fn()} />);

  await user.click(screen.getByRole("tab", { name: "按键布局" }));
  await user.click(screen.getByRole("button", { name: "添加按键" }));
  expect(onRequestProfileMutation).toHaveBeenCalledTimes(1);
});

test("keeps device technical details unavailable without a runtime assignment", async () => {
  const user = userEvent.setup();
  render(<AdvancedSettings language="zh-CN" profiles={[profile]} editorProfileId="profile-a" devices={[device]} selectedDevice={{ ...device, runtimeAssignment: null }} boardProfiles={[board]} onCreate={vi.fn()} onSelectProfile={vi.fn()} onImport={vi.fn()} onExport={vi.fn()} onDelete={vi.fn()} onRequestProfileMutation={vi.fn()} onDuplicateForDevice={vi.fn()} onBeginLearning={vi.fn()} onEndLearning={vi.fn()} />);

  await user.click(screen.getByRole("tab", { name: "技术信息" }));
  expect(screen.getByText("请选择一台已分配设备以查看技术信息。")).toBeInTheDocument();
  expect(screen.queryByText("serial-123")).not.toBeInTheDocument();
});
