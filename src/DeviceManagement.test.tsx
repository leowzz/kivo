import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { DeviceManagement } from "./DeviceManagement";
import type { BoardProfileSummary, CandidateStatus, DeviceProfile, DeviceStatus, HomeMetricsSnapshot } from "./types";

const boards: BoardProfileSummary[] = [
  { id: "rp2040-pad", controllerFamilyId: "rp2040", displayName: "RP2040 Pad", runtimeUsb: "2e8a:000a", bootloaderUsb: null, safePins: [1, 2] },
  { id: "esp32-pad", controllerFamilyId: "esp32s3", displayName: "ESP32-S3 Pad", runtimeUsb: "303a:4002", bootloaderUsb: null, safePins: [4, 5] },
];

function device(overrides: Partial<DeviceStatus> = {}): DeviceStatus {
  return {
    deviceId: "rp-a",
    name: "RP2040 A",
    connection: "online",
    mode: "runtime",
    identity: "valid",
    assignment: "valid",
    runtime: "ready",
    hardwareSerial: "RP-A-001",
    port: "/dev/cu.rp-a",
    controllerFamilyId: "rp2040",
    boardProfileId: "rp2040-pad",
    firmwareBuildId: "rp-build",
    capabilities: [1, 2],
    runtimeAssignment: { device_profile_id: "profile-a", hardware_profile_id: "hardware-a" },
    latestError: null,
    learning: null,
    ...overrides,
  };
}

const candidates: CandidateStatus[] = [{
  key: "candidate:esp32-pad:bad-serial:/dev/cu.bad",
  deviceId: null,
  mode: "bootloader",
  identity: "invalid_identity",
  rawSerial: "BAD-001",
  port: "/dev/cu.bad",
  controllerFamilyId: "esp32s3",
  boardProfileId: "esp32-pad",
  latestError: "identity rejected",
}];

const metrics: HomeMetricsSnapshot = {
  totalPresses: 8,
  todayPresses: 2,
  activeButtonCount: 1,
  topButton: { buttonId: "A", presses: 8 },
  heatmap: [],
  logs: [{ timestampMs: 1, kind: "button", message: "A pressed", deviceId: "rp-a", deviceName: "RP2040 A", deviceProfileId: "profile-a", hardwareProfileId: "hardware-a", buttonId: "A" }],
};

const profiles: DeviceProfile[] = [{ schema_version: 2, profile: { id: "profile-a", name: "Counter Profile", groups: [] }, hardware_profiles: [{ id: "hardware-a", name: "Counter Hardware", board_profile_id: "rp2040-pad", debounce_ms: 20, inputs: [] }], actions: {} }];

function renderManagement(overrides: Partial<React.ComponentProps<typeof DeviceManagement>> = {}) {
  const props: React.ComponentProps<typeof DeviceManagement> = {
    language: "zh-CN",
    devices: [
      device(),
      device({ deviceId: "rp-b", name: "RP2040 B", hardwareSerial: "RP-B-002", port: "/dev/cu.rp-b" }),
      device({ deviceId: "esp-a", name: "ESP32 A", hardwareSerial: "ESP-A-003", port: "/dev/cu.esp-a", controllerFamilyId: "esp32s3", boardProfileId: "esp32-pad" }),
      device({ deviceId: "esp-offline", name: "ESP32 Offline", connection: "offline", mode: null, runtime: "inactive", hardwareSerial: "ESP-OFF-004", port: null, controllerFamilyId: "esp32s3", boardProfileId: "esp32-pad", runtimeAssignment: null }),
    ],
    candidates,
    boardProfiles: boards,
    deviceProfiles: profiles,
    metrics: { deviceId: "rp-a", snapshot: metrics },
    onRename: vi.fn(),
    onForget: vi.fn(),
    onMetricsChange: vi.fn(),
    ...overrides,
  };
  return { ...render(<DeviceManagement {...props} />), props };
}

test("keeps same-board devices as separate selectable rows across filters and search", async () => {
  const user = userEvent.setup();
  renderManagement({ devices: [
    device(),
    device({ deviceId: "rp-b", name: "RP2040 B", hardwareSerial: "RP-B-002", port: "/dev/cu.rp-b" }),
    device({ deviceId: "esp-a", name: "ESP32 A", hardwareSerial: "ESP-A-003", port: "/dev/cu.esp-a", controllerFamilyId: "esp32s3", boardProfileId: "esp32-pad", assignment: "unassigned" }),
    device({ deviceId: "esp-offline", name: "ESP32 Offline", connection: "offline", mode: null, runtime: "inactive", hardwareSerial: "ESP-OFF-004", port: null, controllerFamilyId: "esp32s3", boardProfileId: "esp32-pad", runtimeAssignment: null }),
  ] });

  expect(screen.getAllByRole("button", { name: /RP2040/ })).toHaveLength(2);
  await user.click(screen.getByRole("button", { name: "需处理" }));
  expect(screen.getByRole("button", { name: /ESP32 A/ })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /RP2040 A/ })).toBeNull();
  await user.click(screen.getByRole("button", { name: "就绪" }));
  expect(screen.getAllByRole("button", { name: /RP2040/ })).toHaveLength(2);
  await user.click(screen.getByRole("button", { name: "离线" }));
  expect(screen.getByRole("button", { name: /ESP32 Offline/ })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "全部" }));
  await user.type(screen.getByRole("searchbox", { name: "搜索设备" }), "ESP-A-003");
  expect(screen.getByRole("button", { name: /ESP32 A/ })).toBeInTheDocument();
  await user.clear(screen.getByRole("searchbox", { name: "搜索设备" }));
  await user.type(screen.getByRole("searchbox", { name: "搜索设备" }), "RP2040 Pad");
  expect(screen.getAllByRole("button", { name: /RP2040/ })).toHaveLength(2);
});

test("never renders metrics owned by another Device", () => {
  renderManagement({ metrics: { deviceId: "rp-b", snapshot: metrics } });
  expect(screen.queryByText("A pressed")).toBeNull();
  expect(screen.queryByText("2 / 8")).toBeNull();
});

test("composes visible Board Profile search with non-All filters", async () => {
  const user = userEvent.setup();
  renderManagement();
  await user.click(screen.getByRole("button", { name: "就绪" }));
  await user.type(screen.getByRole("searchbox", { name: "搜索设备" }), "RP2040 Pad");
  expect(screen.getAllByRole("button", { name: /RP2040/ })).toHaveLength(2);
  await user.clear(screen.getByRole("searchbox", { name: "搜索设备" }));
  await user.click(screen.getByRole("button", { name: "需处理" }));
  await user.type(screen.getByRole("searchbox", { name: "搜索设备" }), "ESP32-S3 Pad");
  expect(screen.getByRole("button", { name: /BAD-001/ })).toBeInTheDocument();
});

test("uses assignment display names, retains missing IDs, and shows selected activity", async () => {
  const user = userEvent.setup();
  renderManagement();
  expect(screen.getAllByRole("button", { name: /Counter Profile \/ Counter Hardware/ })).toHaveLength(3);
  expect(screen.getByText("A pressed")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /ESP32 A/ }));
  expect(screen.getAllByText("Counter Profile / Counter Hardware")).toHaveLength(4);
  renderManagement({ devices: [device({ runtimeAssignment: { device_profile_id: "gone", hardware_profile_id: "missing" } })] });
  expect(screen.getAllByText("gone / missing")).toHaveLength(2);
});

test("preserves selected device identity across live replacements", () => {
  const { rerender, props } = renderManagement();
  screen.getByRole("button", { name: /ESP32 A/ }).click();
  rerender(<DeviceManagement {...props} devices={props.devices.map((item) =>
    item.deviceId === "esp-a" ? { ...item, runtime: "runtime_error" } : item
  )} />);
  expect(screen.getByRole("button", { name: /ESP32 A/ })).toHaveAttribute("aria-pressed", "true");
});

test("moves candidate selection to the nearest remaining row when its observation disappears", () => {
  const { rerender, props } = renderManagement();
  screen.getByRole("button", { name: /BAD-001/ }).click();
  rerender(<DeviceManagement {...props} candidates={[]} />);
  expect(screen.getByRole("button", { name: /ESP32 Offline/ })).toHaveAttribute("aria-pressed", "true");
});

test("shows candidate diagnostics without mutable device actions", async () => {
  const user = userEvent.setup();
  renderManagement();
  await user.click(screen.getByRole("button", { name: /BAD-001/ }));
  expect(screen.getByText("identity rejected")).toBeInTheDocument();
  expect(screen.queryByRole("textbox", { name: "设备名称" })).toBeNull();
  expect(screen.queryByRole("button", { name: "忘记设备" })).toBeNull();
});

test("renames exactly the selected device", async () => {
  const user = userEvent.setup();
  const onRename = vi.fn();
  renderManagement({ onRename });
  await user.click(screen.getByRole("button", { name: /RP2040 B/ }));
  await user.click(screen.getByRole("button", { name: "重命名设备" }));
  await user.clear(screen.getByRole("textbox", { name: "设备名称" }));
  await user.type(screen.getByRole("textbox", { name: "设备名称" }), "Counter B");
  await user.click(screen.getByRole("button", { name: "确认重命名" }));
  expect(onRename).toHaveBeenCalledWith("rp-b", "Counter B");
});

test("permits forget only after offline confirmation naming that device", async () => {
  const user = userEvent.setup();
  const onForget = vi.fn();
  renderManagement({ onForget });
  expect(screen.getByRole("button", { name: "忘记设备" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: /ESP32 Offline/ }));
  await user.click(screen.getByRole("button", { name: "忘记设备" }));
  const dialog = screen.getByRole("dialog", { name: "忘记设备" });
  expect(within(dialog).getByText(/ESP32 Offline/)).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "确认" }));
  expect(onForget).toHaveBeenCalledWith("esp-offline");
  expect(screen.queryByRole("checkbox")).toBeNull();
});

test("closes stale forget confirmation when the device reconnects", async () => {
  const user = userEvent.setup();
  const onForget = vi.fn();
  const { rerender, props } = renderManagement({ onForget });
  await user.click(screen.getByRole("button", { name: /ESP32 Offline/ }));
  await user.click(screen.getByRole("button", { name: "忘记设备" }));
  rerender(<DeviceManagement {...props} devices={props.devices.map((item) => item.deviceId === "esp-offline" ? { ...item, connection: "online", mode: "runtime" } : item)} />);
  expect(screen.queryByRole("dialog", { name: "忘记设备" })).toBeNull();
  expect(onForget).not.toHaveBeenCalled();
});
