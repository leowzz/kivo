import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { beforeEach, expect, test, vi } from "vitest";
import App from "./App";
import type { AppSnapshot, DeviceProfile, DeviceStatus } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

const deviceProfile: DeviceProfile = {
  schema_version: 2,
  profile: {
    id: "tel-carbon-v1",
    name: "碳膜电话键盘",
    groups: [{
      id: "digits",
      columns: 2,
      buttons: [
        { id: "DIGIT_2", label: "2" },
        { id: "ENTER", label: "确认" },
      ],
    }],
  },
  hardware_profiles: [{
    id: "front-desk",
    name: "前台硬件配置",
    board_profile_id: "luatos-esp32s3-aio",
    debounce_ms: 30,
    inputs: [
      { type: "direct", id: "side", keys: { ENTER: 6 } },
      { type: "contact_matrix", id: "carbon", pins: [1, 2, 12, 13], keys: { DIGIT_2: [1, 12] } },
    ],
  }],
  actions: {},
};

function device(overrides: Partial<DeviceStatus> = {}): DeviceStatus {
  return {
    deviceId: "device-front-desk",
    name: "前台键盘",
    connection: "online",
    mode: "runtime",
    identity: "valid",
    assignment: "valid",
    runtime: "ready",
    hardwareSerial: "ABC123",
    port: "/dev/cu.test",
    controllerFamilyId: "esp32s3",
    boardProfileId: "luatos-esp32s3-aio",
    firmwareBuildId: "test-build",
    capabilities: [1, 2, 6, 12, 13],
    runtimeAssignment: {
      device_profile_id: deviceProfile.profile.id,
      hardware_profile_id: "front-desk",
    },
    latestError: null,
    learning: null,
    ...overrides,
  };
}

const baseSnapshot: AppSnapshot = {
  deviceProfiles: [deviceProfile],
  editorProfile: deviceProfile.profile.id,
  boardProfiles: [{
    id: "luatos-esp32s3-aio",
    controllerFamilyId: "esp32s3",
    displayName: "LuatOS ESP32-S3 AIO",
    runtimeUsb: "303a:4002",
    bootloaderUsb: null,
    safePins: [1, 2, 6, 12, 13],
  }],
  devices: [device()],
  candidates: [],
  language: "zh-CN",
  homeMetrics: {
    totalPresses: 12,
    todayPresses: 3,
    activeButtonCount: 2,
    topButton: { buttonId: "DIGIT_2", presses: 8 },
    heatmap: [{ buttonId: "DIGIT_2", day: "2026-07-30", presses: 3 }],
    logs: [{
      timestampMs: 1785396000000,
      kind: "button",
      message: "DIGIT_2 pressed",
      deviceId: "device-front-desk",
      deviceName: "前台键盘",
      deviceProfileId: deviceProfile.profile.id,
      hardwareProfileId: "front-desk",
      buttonId: "DIGIT_2",
    }],
  },
};

let currentSnapshot: AppSnapshot;

beforeEach(() => {
  vi.clearAllMocks();
  currentSnapshot = structuredClone(baseSnapshot);
  HTMLDialogElement.prototype.showModal = function showModal() { this.setAttribute("open", ""); };
  HTMLDialogElement.prototype.close = function close() { this.removeAttribute("open"); };
  vi.mocked(listen).mockResolvedValue(vi.fn());
  vi.mocked(open).mockResolvedValue(null);
  vi.mocked(save).mockResolvedValue(null);
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "save_device_profile") {
      const saved = (args as { profile: DeviceProfile }).profile;
      currentSnapshot.deviceProfiles = currentSnapshot.deviceProfiles.map((item) =>
        item.profile.id === saved.profile.id ? saved : item
      );
    }
    if (command === "save_settings") {
      const settings = (args as { settings: { editor_profile: string | null; language: AppSnapshot["language"] } }).settings;
      currentSnapshot.editorProfile = settings.editor_profile;
      currentSnapshot.language = settings.language;
    }
    if (command === "delete_device_profile") {
      currentSnapshot = { ...currentSnapshot, deviceProfiles: [], editorProfile: null };
    }
    return structuredClone(currentSnapshot);
  });
});

test("does not override the WebView viewport height", async () => {
  render(<App />);

  await screen.findByRole("heading", { name: "按键概览" });
  expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");
});

test("summarizes an empty device registry", async () => {
  currentSnapshot.devices = [];
  render(<App />);

  expect(await screen.findByLabelText("设备状态汇总")).toHaveTextContent("0 就绪 · 0 需处理 · 0 离线");
});

test("summarizes mixed devices and adds candidates only to attention", async () => {
  currentSnapshot.devices = [
    device(),
    device({ deviceId: "device-needs-attention", assignment: "invalid_assignment", runtime: "inactive" }),
    device({ deviceId: "device-offline", connection: "offline", mode: null, runtime: "inactive" }),
  ];
  currentSnapshot.candidates = [{
    key: "candidate-bootloader",
    deviceId: null,
    mode: "bootloader",
    identity: "invalid_identity",
    rawSerial: null,
    port: null,
    controllerFamilyId: "rp2040",
    boardProfileId: "vccgnd-yd-rp2040",
    latestError: null,
  }];
  render(<App />);

  expect(await screen.findByLabelText("设备状态汇总")).toHaveTextContent("1 就绪 · 2 需处理 · 1 离线");
});

test("keeps device management and configuration-file actions as separate work destinations", async () => {
  render(<App />);

  expect(await screen.findByRole("heading", { name: "按键概览" })).toBeInTheDocument();
  const navigation = screen.getByRole("navigation", { name: "配置" });
  expect(navigation).not.toContainElement(screen.getByRole("button", { name: "首页" }));
  expect(screen.getByRole("button", { name: "首页" })).toHaveClass("is-active");
  const devicesButton = screen.getByRole("button", { name: "设备管理" });
  expect(devicesButton.querySelector(".lucide-usb")).not.toBeNull();
  await userEvent.setup().click(devicesButton);
  expect(screen.getByRole("heading", { name: "设备管理" })).toBeInTheDocument();
  expect(screen.queryByLabelText("当前编辑配置")).toBeNull();
  await userEvent.setup().click(screen.getByRole("button", { name: "配置文件" }));
  expect(screen.getByLabelText("当前编辑配置")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "删除设备配置" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /^保存$/ })).not.toBeInTheDocument();
  expect(document.body).not.toHaveTextContent("型号");
});

test("selecting the Editor Profile saves only the version-2 editor settings patch", async () => {
  const secondProfile: DeviceProfile = {
    ...structuredClone(deviceProfile),
    profile: { ...deviceProfile.profile, id: "operator-console", name: "接线员控制台" },
  };
  currentSnapshot.deviceProfiles.push(secondProfile);
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "配置文件" }));

  await user.selectOptions(screen.getByLabelText("当前编辑配置"), secondProfile.profile.id);

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_settings", {
    settings: {
      schema_version: 2,
      editor_profile: secondProfile.profile.id,
      language: "zh-CN",
    },
  }));
  expect(vi.mocked(invoke).mock.calls.some(([command]) =>
    command === "save_runtime_assignment" || command === "clear_runtime_assignment"
  )).toBe(false);
  expect(currentSnapshot.devices[0].runtimeAssignment).toEqual(baseSnapshot.devices[0].runtimeAssignment);
});

test("keeps the interface in Simplified Chinese without a language selector", async () => {
  currentSnapshot.language = "en-US";
  render(<App />);

  expect(await screen.findByText("配置文件")).toBeInTheDocument();
  expect(screen.queryByLabelText("语言")).toBeNull();
});

test("renders seven-day metrics in model order with Chinese logs", async () => {
  currentSnapshot.deviceProfiles[0].profile.groups = [
    { id: "digits", columns: 2, buttons: [{ id: "DIGIT_2", label: "2" }, { id: "DIGIT_5", label: "5" }] },
    { id: "actions", columns: 1, buttons: [{ id: "ENTER", label: "确认" }] },
  ];
  currentSnapshot.homeMetrics = {
    ...baseSnapshot.homeMetrics!,
    heatmap: [{ buttonId: "DIGIT_5", day: "2026-07-30", presses: 3 }],
    logs: [{
      ...baseSnapshot.homeMetrics!.logs[0],
      message: "DIGIT_5 pressed",
      buttonId: "DIGIT_5",
    }],
  };
  render(<App />);

  await screen.findByRole("heading", { name: "按键概览" });
  expect([...document.querySelectorAll(".heat-cell")].map((item) => item.textContent)).toEqual([
    expect.stringContaining("2"), expect.stringContaining("5"), expect.stringContaining("确认"),
  ]);
  expect(screen.getByText("按下 DIGIT_5")).toBeInTheDocument();
  expect(screen.queryByLabelText("当前编辑配置")).toBeNull();
  expect(screen.queryByLabelText("语言")).toBeNull();
});

test("builds an ordered action list and autosaves it", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "按键行为" }));
  await screen.findByRole("button", { name: "2，0 项行为" });
  await screen.findByRole("complementary", { name: "2" });

  await user.click(screen.getByRole("button", { name: "粘贴文本" }));
  await user.type(screen.getByRole("textbox", { name: "文本" }), "你好");
  await user.click(screen.getByRole("button", { name: "按下按键" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
    profile: expect.objectContaining({
      actions: {
        DIGIT_2: [
          { type: "paste", text: "你好" },
          { type: "hotkey", keys: ["enter"] },
        ],
      },
    }),
  }), { timeout: 1600 });
  expect(screen.getByRole("button", { name: "2，2 项行为" })).toBeInTheDocument();
});

test("records a shortcut from the application window", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "按键行为" }));
  const editor = await screen.findByRole("complementary", { name: "2" });

  await user.click(screen.getByRole("button", { name: "按下按键" }));
  await user.click(within(editor).getByRole("button", { name: "录入按键" }));
  fireEvent.keyDown(window, { code: "KeyK", key: "k", metaKey: true, shiftKey: true });

  expect(within(editor).getByText("Command + Shift + K")).toBeInTheDocument();
});

test("manually selects a multi-modifier shortcut", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "按键行为" }));
  const editor = await screen.findByRole("complementary", { name: "2" });

  await user.click(screen.getByRole("button", { name: "按下按键" }));
  await user.click(within(editor).getByRole("checkbox", { name: "Cmd" }));
  await user.click(within(editor).getByRole("checkbox", { name: "Ctrl" }));
  await user.click(within(editor).getByRole("checkbox", { name: "Shift" }));
  await user.selectOptions(within(editor).getByRole("combobox", { name: "按键" }), "k");

  expect(within(editor).getByText("Command + Control + Shift + K")).toBeInTheDocument();
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
    profile: expect.objectContaining({
      actions: { DIGIT_2: [{ type: "hotkey", keys: ["cmd", "ctrl", "shift", "k"] }] },
    }),
  }), { timeout: 1600 });
});

test("reorders actions from the right editor", async () => {
  const user = userEvent.setup();
  currentSnapshot.deviceProfiles[0].actions.DIGIT_2 = [
    { type: "paste", text: "先粘贴" },
    { type: "hotkey", keys: ["enter"] },
  ];
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "按键行为" }));
  const editor = await screen.findByRole("complementary", { name: "2" });

  await user.click(within(editor).getAllByRole("button", { name: "上移" })[1]);

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
    profile: expect.objectContaining({
      actions: { DIGIT_2: [{ type: "hotkey", keys: ["enter"] }, { type: "paste", text: "先粘贴" }] },
    }),
  }), { timeout: 1600 });
});

test("keeps a failed autosave and exposes retry", async () => {
  const user = userEvent.setup();
  let saveAttempts = 0;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "save_device_profile" && saveAttempts++ === 0) throw new Error("disk full");
    return structuredClone(currentSnapshot);
  });
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "按键行为" }));
  const key = await screen.findByRole("button", { name: "2，0 项行为" });

  await user.click(key);
  await user.click(screen.getByRole("button", { name: "按下按键" }));
  expect(await screen.findByText("保存失败", {}, { timeout: 1600 })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "重试" }));

  await waitFor(() => expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "save_device_profile")).toHaveLength(2));
});

test("previews a device profile before importing it", async () => {
  const user = userEvent.setup();
  vi.mocked(open).mockResolvedValue("/tmp/device-profile.yaml");
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "preview_device_profile_import") return {
      profileId: "tel-carbon-v1",
      profileName: "碳膜电话键盘",
      buttonCount: 22,
      hardwareBindingCount: 22,
      actionCount: 8,
      replacesExisting: true,
    };
    return structuredClone(currentSnapshot);
  });
  render(<App />);
  await screen.findByText("配置文件");

  await user.click(screen.getByRole("button", { name: "配置文件" }));
  await user.click(screen.getByRole("button", { name: "导入设备配置" }));
  const dialog = await screen.findByRole("dialog", { name: "替换现有设备配置" });
  expect(within(dialog).getByText("22 个按键，22 项硬件配置，8 项行为")).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("import_device_profile", { path: "/tmp/device-profile.yaml" }));
});

test("previews a full backup before restoring it", async () => {
  const user = userEvent.setup();
  vi.mocked(open).mockResolvedValue("/tmp/backup.yaml");
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "preview_backup") return {
      profileCount: 3,
      buttonCount: 44,
      hardwareBindingCount: 40,
      actionCount: 19,
    };
    return structuredClone(currentSnapshot);
  });
  render(<App />);
  await screen.findByText("配置文件");

  await user.click(screen.getByRole("button", { name: "配置文件" }));
  await user.click(screen.getByRole("button", { name: "恢复备份" }));
  const dialog = await screen.findByRole("dialog", { name: "恢复全量备份" });
  expect(within(dialog).getByText("3 个设备配置，44 个按键，40 项硬件配置，19 项行为")).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("restore_backup", { path: "/tmp/backup.yaml" }));
});

test("deletes the last device profile and keeps configuration-file actions available", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("配置文件");

  await user.click(screen.getByRole("button", { name: "配置文件" }));
  await user.click(screen.getByRole("button", { name: "删除设备配置" }));
  const dialog = await screen.findByRole("dialog", { name: "删除设备配置" });
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  expect(await screen.findByRole("option", { name: "还没有设备配置" })).toBeInTheDocument();
  expect(screen.getAllByRole("button", { name: "导入设备配置" }).length).toBeGreaterThan(0);
  expect(screen.getAllByRole("button", { name: "恢复备份" }).length).toBeGreaterThan(0);
});

test("keeps key learning secondary and collapsed by default", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("配置文件");

  await user.click(screen.getByRole("button", { name: "硬件配置" }));

  expect(screen.getByText("直连 GPIO")).toBeInTheDocument();
  expect(screen.getByText("接触矩阵")).toBeInTheDocument();
  expect(screen.getByText("适配新设备").closest("details")).not.toHaveAttribute("open");
});
