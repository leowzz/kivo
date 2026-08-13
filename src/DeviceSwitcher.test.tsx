import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { DeviceSwitcher } from "./DeviceSwitcher";
import type { DeviceStatus } from "./types";

function device(overrides: Partial<DeviceStatus> = {}): DeviceStatus {
  return {
    deviceId: "ready",
    name: "前台键盘",
    connection: "online",
    mode: "runtime",
    identity: "valid",
    assignment: "valid",
    runtime: "ready",
    hardwareSerial: "SERIAL",
    port: "/dev/cu.test",
    controllerFamilyId: "esp32s3",
    boardProfileId: "board",
    firmwareBuildId: null,
    capabilities: [],
    runtimeAssignment: null,
    latestError: null,
    learning: null,
    ...overrides,
  };
}

test("switches between visible physical keyboards", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  const ready = device();
  const offline = device({ deviceId: "offline", name: "备用键盘", connection: "offline", runtime: "inactive" });

  render(<DeviceSwitcher devices={[ready, offline]} selectedDeviceId={ready.deviceId} language="zh-CN" onChange={onChange} />);

  const select = screen.getByRole("combobox", { name: "当前键盘" });
  expect(select).toHaveValue(ready.deviceId);
  await user.selectOptions(select, offline.deviceId);
  expect(onChange).toHaveBeenCalledWith(offline.deviceId);
  expect(screen.getByRole("option", { name: "备用键盘 · 离线" })).toBeInTheDocument();
});

test("shows a non-interactive connection prompt with no devices", () => {
  render(<DeviceSwitcher devices={[]} selectedDeviceId={null} language="zh-CN" onChange={vi.fn()} />);

  expect(screen.getByText("连接键盘")).toBeInTheDocument();
  expect(screen.queryByRole("combobox")).toBeNull();
});
