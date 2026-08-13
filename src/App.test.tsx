import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
// Vitest runs this source assertion in Node, while the production tsconfig excludes Node globals.
// @ts-expect-error Test-only Node module.
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "./App";
import type {
  AppSnapshot,
  CreateDeviceProfileRequest,
  DeviceProfile,
  DeviceStatus,
  RuntimeAssignment,
  RuntimeEvent,
} from "./types";

const appCss = readFileSync("src/styles/app.css", "utf8");
const baseCss = readFileSync("src/styles/base.css", "utf8");

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

const deviceProfile: DeviceProfile = {
  schema_version: 3,
  profile: {
    id: "tel-carbon-v1",
    name: "碳膜电话键盘",
    groups: [
      {
        id: "digits",
        columns: 2,
        buttons: [
          { id: "DIGIT_2", label: "2" },
          { id: "ENTER", label: "确认" },
        ],
      },
    ],
  },
  trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
  hardware_profiles: [
    {
      id: "front-desk",
      name: "前台硬件配置",
      board_profile_id: "luatos-esp32s3-aio",
      debounce_ms: 30,
      inputs: [
        { type: "direct", id: "side", keys: { ENTER: 6 } },
        {
          type: "contact_matrix",
          id: "carbon",
          pins: [1, 2, 12, 13],
          keys: { DIGIT_2: [1, 12] },
        },
      ],
    },
  ],
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

const rpBoard: AppSnapshot["boardProfiles"][number] = {
  id: "rp",
  controllerFamilyId: "rp2040",
  displayName: "RP2040 Pad",
  runtimeUsb: "2e8a:102e",
  bootloaderUsb: "2e8a:0003",
  safePins: [0, 1],
};

const rpProfile: DeviceProfile = {
  schema_version: 3,
  profile: { id: "rp-profile", name: "RP Profile", groups: [{ id: "main", columns: 1, buttons: [{ id: "rp-key", label: "RP Key" }] }] },
  trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
  hardware_profiles: [
    {
      id: "rp-other",
      name: "RP Other",
      board_profile_id: "rp",
      debounce_ms: 30,
      inputs: [{ type: "direct", id: "buttons", keys: { "rp-key": 0 } }],
    },
    {
      id: "rp-hardware",
      name: "RP Hardware",
      board_profile_id: "rp",
      debounce_ms: 30,
      inputs: [{ type: "direct", id: "buttons", keys: { "rp-key": 0 } }],
    },
  ],
  actions: {},
};

function rpCandidate(
  overrides: Partial<AppSnapshot["candidates"][number]> = {},
): AppSnapshot["candidates"][number] {
  return {
    key: "runtime:/dev/cu.usbmodem1101",
    deviceId: "stable-rp",
    mode: "runtime",
    identity: "validating",
    issue: "validating",
    rawSerial: "50031519384E811C",
    port: "/dev/cu.usbmodem1101",
    controllerFamilyId: "rp2040",
    boardProfileId: "rp",
    latestError: null,
    ...overrides,
  };
}

function rpUnassignedDevice(
  overrides: Partial<DeviceStatus> = {},
): DeviceStatus {
  return device({
    deviceId: "stable-rp",
    name: "RP2040 Pad · 4E811C",
    assignment: "unassigned",
    runtime: "inactive",
    hardwareSerial: "50031519384E811C",
    port: "/dev/cu.usbmodem1101",
    controllerFamilyId: "rp2040",
    boardProfileId: "rp",
    firmwareBuildId: "hello-v3",
    capabilities: [0, 1],
    runtimeAssignment: null,
    ...overrides,
  });
}

const baseSnapshot: AppSnapshot = {
  deviceProfiles: [deviceProfile],
  editorProfile: deviceProfile.profile.id,
  boardProfiles: [
    {
      id: "luatos-esp32s3-aio",
      controllerFamilyId: "esp32s3",
      displayName: "LuatOS ESP32-S3 AIO",
      runtimeUsb: "303a:4002",
      bootloaderUsb: null,
      safePins: [1, 2, 6, 12, 13],
    },
  ],
  devices: [device()],
  candidates: [],
  language: "zh-CN",
  homeMetrics: {
    totalPresses: 12,
    todayPresses: 3,
    activeButtonCount: 2,
    topButton: { buttonId: "DIGIT_2", presses: 8 },
    heatmap: [{ buttonId: "DIGIT_2", day: "2026-07-30", presses: 3 }],
    logs: [
      {
        timestampMs: 1785396000000,
        kind: "button",
        message: "DIGIT_2 pressed",
        deviceId: "device-front-desk",
        deviceName: "前台键盘",
        deviceProfileId: deviceProfile.profile.id,
        hardwareProfileId: "front-desk",
        buttonId: "DIGIT_2",
      },
    ],
  },
};

let currentSnapshot: AppSnapshot;
let emitRuntimeEvent: (event: RuntimeEvent) => void;

function deferred<Value>() {
  let resolve!: (value: Value) => void;
  const promise = new Promise<Value>((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}

async function openActionDialog(user: ReturnType<typeof userEvent.setup>, type: "paste" | "hotkey" = "hotkey") {
  await user.click(screen.getByRole("button", { name: "添加其他行为" }));
  await user.selectOptions(screen.getByLabelText("行为类型"), type);
}

async function addPasteAction(user: ReturnType<typeof userEvent.setup>, text: string) {
  await openActionDialog(user, "paste");
  await user.type(screen.getByRole("textbox", { name: "文本" }), text);
  await user.click(screen.getByRole("button", { name: "保存" }));
}

async function addHotkeyAction(user: ReturnType<typeof userEvent.setup>) {
  await openActionDialog(user, "hotkey");
  await user.click(screen.getByRole("checkbox", { name: "回车" }));
  await user.click(screen.getByRole("button", { name: "保存" }));
}

async function openDeviceIo(user: ReturnType<typeof userEvent.setup>) {
  const settingsButton = screen.getByRole("button", { name: "设置" });
  if (!settingsButton.classList.contains("is-active")) await user.click(settingsButton);
  const advancedButton = screen.queryByRole("button", { name: "高级设置" });
  if (advancedButton) await user.click(advancedButton);
  await user.click(await screen.findByRole("tab", { name: "I/O 映射" }));
}

function runtimeEvent(overrides: Partial<RuntimeEvent> = {}): RuntimeEvent {
  return {
    timestampMs: 1785396000000,
    level: "info",
    deviceId: "device-front-desk",
    rawSerial: "ABC123",
    controllerFamilyId: "esp32s3",
    boardProfileId: "luatos-esp32s3-aio",
    port: "/dev/cu.test",
    deviceProfileId: deviceProfile.profile.id,
    hardwareProfileId: "front-desk",
    homeUpdate: null,
    code: "input_state",
    params: {},
    detail: null,
    input: { type: "direct", gpio: 6 },
    pressed: true,
    learningTarget: null,
    ...overrides,
  };
}

beforeEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
  localStorage.removeItem("kivo:selected-device-id");
  currentSnapshot = structuredClone(baseSnapshot);
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close() {
    this.removeAttribute("open");
  };
  vi.mocked(listen).mockImplementation(async (_event, handler) => {
    emitRuntimeEvent = (event) => handler({ payload: event } as never);
    return () => undefined;
  });
  vi.mocked(open).mockResolvedValue(null);
  vi.mocked(save).mockResolvedValue(null);
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "save_device_profile") {
      const saved = (args as { profile: DeviceProfile }).profile;
      currentSnapshot.deviceProfiles = currentSnapshot.deviceProfiles.map(
        (item) => (item.profile.id === saved.profile.id ? saved : item),
      );
    }
    if (command === "save_settings") {
      const settings = (
        args as {
          settings: {
            editor_profile: string | null;
            language: AppSnapshot["language"];
          };
        }
      ).settings;
      currentSnapshot.editorProfile = settings.editor_profile;
      currentSnapshot.language = settings.language;
    }
    if (command === "retry_candidate") {
      const deviceId = (args as { deviceId: string }).deviceId;
      currentSnapshot.candidates = currentSnapshot.candidates.map((candidate) =>
        candidate.deviceId === deviceId
          ? { ...candidate, issue: "validating", latestError: null }
          : candidate,
      );
    }
    if (command === "create_device_profile") {
      const request = (args as { request: CreateDeviceProfileRequest }).request;
      const id = request.name === "Offline RP" ? "offline-rp" : "created-profile";
      const created =
        request.kind === "clone"
          ? {
              ...structuredClone(
                currentSnapshot.deviceProfiles.find(
                  (profile) => profile.profile.id === request.source_profile_id,
                )!,
              ),
              profile: {
                ...structuredClone(
                  currentSnapshot.deviceProfiles.find(
                    (profile) => profile.profile.id === request.source_profile_id,
                  )!.profile,
                ),
                id,
                name: request.name,
              },
            }
          : {
              schema_version: 3 as const,
              profile: { id, name: request.name, groups: [] },
              trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
              hardware_profiles: [
                {
                  id: "hardware",
                  name: "Default hardware",
                  board_profile_id: request.board_profile_id,
                  debounce_ms: 30,
                  inputs: [],
                },
              ],
              actions: {},
            };
      currentSnapshot.deviceProfiles.push(created);
      currentSnapshot.editorProfile = id;
    }
    if (command === "complete_device_setup") {
      const { deviceId, name, assignment } = args as {
        deviceId: string;
        name: string;
        assignment: RuntimeAssignment;
      };
      currentSnapshot.devices = currentSnapshot.devices.map((item) =>
        item.deviceId === deviceId
          ? {
              ...item,
              name,
              assignment: "valid",
              runtime: "configuring",
              runtimeAssignment: assignment,
            }
          : item,
      );
    }
    if (command === "delete_device_profile") {
      currentSnapshot = {
        ...currentSnapshot,
        deviceProfiles: [],
        editorProfile: null,
      };
    }
    if (command === "begin_learning") {
      const target = args as {
        deviceId: string;
        deviceProfileId: string;
        hardwareProfileId: string;
        editingRevision: number;
        pins: number[];
      };
      const selected = currentSnapshot.devices.find(({ deviceId }) => deviceId === target.deviceId);
      if (selected) selected.learning = { ...target, firmwareRevision: 0 };
    }
    if (command === "end_learning") {
      const selected = currentSnapshot.devices.find(
        ({ deviceId }) => deviceId === (args as { deviceId: string }).deviceId,
      );
      if (selected) selected.learning = null;
    }
    return structuredClone(currentSnapshot);
  });
});

afterEach(() => {
  vi.useRealTimers();
});

test("keeps ordinary navigation to keyboard, devices, and settings, with Advanced Settings returning through Settings", async () => {
  const user = userEvent.setup();
  render(<App />);

  await screen.findByRole("heading", { name: "碳膜电话键盘" });
  const sidebarButtons = Array.from(document.querySelectorAll(".sidebar > button"))
    .map((button) => button.textContent?.trim());
  expect(sidebarButtons).toEqual(["我的键盘", "设备", "设置"]);
  expect(screen.queryByRole("button", { name: "首页" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "按键行为" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "配置文件" })).not.toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "高级设置" }));
  expect(screen.getByRole("button", { name: "返回" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "返回" }));
  expect(screen.getByRole("heading", { name: "设置" })).toBeInTheDocument();
});

test("uses a three-column text navigation row without page-level horizontal overflow on narrow screens", () => {
  expect(appCss).toMatch(
    /\.product-workspace\s*\{[^}]*grid-template-columns:\s*200px\s+minmax\(0,\s*1fr\)[^}]*\}/,
  );
  expect(appCss).toMatch(
    /@media \(max-width: 980px\)[\s\S]*?\.product-workspace\s*\{[^}]*grid-template-columns:\s*180px\s+minmax\(0,\s*1fr\)[^}]*\}/,
  );
  expect(appCss).toMatch(
    /@media \(max-width: 680px\)[\s\S]*?\.sidebar\s*\{[^}]*grid-template-columns:\s*repeat\(3,\s*minmax\(0,\s*1fr\)\)[^}]*\}/,
  );
  expect(baseCss).toMatch(/html, body, #root\s*\{[^}]*overflow:\s*hidden/);
});

test("shows an incompatible-workspace startup screen without loading runtime state", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_startup_failure") {
      return {
        code: "unsupported_profile_schema",
        detail: "unsupported_profile_schema",
      };
    }
    throw new Error(`unexpected command: ${command}`);
  });

  render(<App />);

  expect(await screen.findByRole("heading", { name: "Kivo 无法启动" })).toBeInTheDocument();
  expect(screen.getByText("当前配置由较新版本创建。请更新 Kivo 后重试。")).toBeInTheDocument();
  expect(screen.getByText("现有配置未被修改。")).toBeInTheDocument();
  expect(screen.getByText("unsupported_profile_schema")).toBeInTheDocument();
  expect(invoke).not.toHaveBeenCalledWith("get_snapshot");
  expect(listen).not.toHaveBeenCalled();
});

test("does not override the WebView viewport height", async () => {
  render(<App />);

  await screen.findByRole("heading", { name: "碳膜电话键盘" });
  expect(document.documentElement.style.getPropertyValue("--app-height")).toBe(
    "",
  );
});

test("fills default trigger settings when a stale profile omits them", async () => {
  const staleProfile = structuredClone(currentSnapshot.deviceProfiles[0]) as Omit<DeviceProfile, "trigger_settings"> & {
    trigger_settings?: DeviceProfile["trigger_settings"];
  };
  delete staleProfile.trigger_settings;
  currentSnapshot.deviceProfiles = [staleProfile as DeviceProfile];

  const user = userEvent.setup();
  render(<App />);

  expect(await screen.findByRole("heading", { name: "碳膜电话键盘" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "高级设置" }));
  await user.click(screen.getByRole("tab", { name: "按键布局" }));
  await user.click(screen.getByRole("button", { name: "添加按键" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", expect.anything()));
  const saved = vi.mocked(invoke).mock.calls.find(([command]) => command === "save_device_profile")?.[1] as {
    profile: DeviceProfile;
  } | undefined;
  expect(saved?.profile.trigger_settings).toEqual({ long_press_ms: 500, double_press_ms: 300 });
});

test("keeps the editor configuration independent from the device assignment", async () => {
  const secondProfile: DeviceProfile = {
    ...structuredClone(deviceProfile),
    profile: {
      ...deviceProfile.profile,
      id: "profile-b",
      name: "备用配置",
    },
  };
  currentSnapshot.deviceProfiles.push(secondProfile);
  const user = userEvent.setup();
  render(<App />);

  const keyboard = await screen.findByRole("combobox", { name: "当前键盘" });
  expect(keyboard).toHaveValue("device-front-desk");
  await user.click(screen.getByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "高级设置" }));
  await user.click(screen.getByRole("button", { name: "选择 备用配置" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_settings", {
    settings: expect.objectContaining({ editor_profile: "profile-b" }),
  }));
  expect(screen.getByText("当前编辑配置")).toBeInTheDocument();
  expect(keyboard).toHaveValue("device-front-desk");
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_runtime_assignment")).toBe(false);
});

test("autosaves advanced offline editor layout changes without a selected device", async () => {
  currentSnapshot.devices = [];
  const user = userEvent.setup();
  render(<App />);

  await user.click(await screen.findByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "高级设置" }));
  await user.click(screen.getByRole("tab", { name: "按键布局" }));
  await user.click(screen.getByRole("button", { name: "添加按键" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
    profile: expect.objectContaining({
      profile: expect.objectContaining({
        id: deviceProfile.profile.id,
        groups: expect.arrayContaining([expect.objectContaining({
          buttons: expect.arrayContaining([expect.objectContaining({ id: "KEY_1" })]),
        })]),
      }),
    }),
  }));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "duplicate_profile_for_device")).toBe(false);
});

test("does not duplicate an unrelated editor profile for an unassigned selected device", async () => {
  currentSnapshot.devices[0] = device({ assignment: "unassigned", runtimeAssignment: null });
  const user = userEvent.setup();
  render(<App />);

  await user.click(await screen.findByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "高级设置" }));
  await user.click(screen.getByRole("tab", { name: "按键布局" }));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "duplicate_profile_for_device")).toBe(false);
});

test("saves a uniquely used editor profile directly when the selected device is unassigned", async () => {
  currentSnapshot.devices[0] = device({ assignment: "unassigned", runtimeAssignment: null });
  const user = userEvent.setup();
  render(<App />);

  await user.click(await screen.findByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "高级设置" }));
  await user.click(screen.getByRole("tab", { name: "按键布局" }));
  await user.click(screen.getByRole("button", { name: "添加按键" }));

  expect(screen.queryByRole("heading", { name: "选择修改范围" })).toBeNull();
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
    profile: expect.objectContaining({ profile: expect.objectContaining({ id: deviceProfile.profile.id }) }),
  }));
});

test("offers only the shared edit scope for an offline editor profile shared by two devices", async () => {
  currentSnapshot.devices = [
    device({ assignment: "unassigned", runtimeAssignment: null }),
    device({ deviceId: "device-second", name: "后台键盘" }),
    device({ deviceId: "device-third", name: "第三键盘" }),
  ];
  const user = userEvent.setup();
  render(<App />);

  await user.click(await screen.findByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "高级设置" }));
  await user.click(screen.getByRole("tab", { name: "按键布局" }));
  await user.click(screen.getByRole("button", { name: "添加按键" }));

  expect(await screen.findByRole("heading", { name: "选择修改范围" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "同步修改 2 台键盘" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "仅修改这台键盘" })).toBeNull();
});

test("waits for an autosave before changing pages", async () => {
  const pendingSave = deferred<AppSnapshot>();
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "save_device_profile") {
      const saved = (args as { profile: DeviceProfile }).profile;
      currentSnapshot.deviceProfiles = currentSnapshot.deviceProfiles.map((profile) =>
        profile.profile.id === saved.profile.id ? saved : profile,
      );
      return pendingSave.promise;
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  await addPasteAction(user, "未保存");

  await user.click(screen.getByRole("button", { name: "设备" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", expect.anything()));
  expect(screen.getByRole("heading", { name: "碳膜电话键盘" })).toBeInTheDocument();

  pendingSave.resolve(structuredClone(currentSnapshot));
  expect(await screen.findByRole("heading", { name: "设备" })).toBeInTheDocument();
});

test("default workspace names the selected keyboard without exposing its system port", async () => {
  render(<App />);
  await screen.findByRole("heading", { name: "碳膜电话键盘" });
  expect(screen.getByText("前台键盘")).toBeInTheDocument();
  expect(screen.queryByText("/dev/cu.test")).toBeNull();
});

test("does not mutate or autosave a shared profile until the edit scope is chosen", async () => {
  currentSnapshot.devices.push(device({
    deviceId: "device-second",
    name: "后台键盘",
  }));
  const user = userEvent.setup();
  render(<App />);

  await user.click(await screen.findByRole("button", { name: "确认，0 项行为" }));
  await user.click(screen.getByRole("button", { name: "重命名按键 确认" }));
  await user.clear(screen.getByRole("textbox", { name: "按键名称" }));
  await user.type(screen.getByRole("textbox", { name: "按键名称" }), "新的确认");
  await user.click(screen.getByRole("button", { name: "确认重命名" }));

  expect(await screen.findByRole("heading", { name: "选择修改范围" })).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "确认" })).toBeInTheDocument();
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);
});

test("cancelling a shared profile edit leaves the profile unchanged", async () => {
  currentSnapshot.devices.push(device({ deviceId: "device-second", name: "后台键盘" }));
  const user = userEvent.setup();
  render(<App />);

  await user.click(await screen.findByRole("button", { name: "确认，0 项行为" }));
  await user.click(screen.getByRole("button", { name: "重命名按键 确认" }));
  await user.clear(screen.getByRole("textbox", { name: "按键名称" }));
  await user.type(screen.getByRole("textbox", { name: "按键名称" }), "取消的修改");
  await user.click(screen.getByRole("button", { name: "确认重命名" }));
  await user.click(await screen.findByRole("button", { name: "取消" }));

  expect(screen.queryByRole("heading", { name: "选择修改范围" })).toBeNull();
  expect(screen.getByRole("button", { name: "确认，0 项行为" })).toBeInTheDocument();
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);
});

test("saves the original shared profile after choosing the shared edit scope", async () => {
  currentSnapshot.devices.push(device({ deviceId: "device-second", name: "后台键盘" }));
  const user = userEvent.setup();
  render(<App />);

  await user.click(await screen.findByRole("button", { name: "确认，0 项行为" }));
  await user.click(screen.getByRole("button", { name: "重命名按键 确认" }));
  await user.clear(screen.getByRole("textbox", { name: "按键名称" }));
  await user.type(screen.getByRole("textbox", { name: "按键名称" }), "新的确认");
  await user.click(screen.getByRole("button", { name: "确认重命名" }));
  await user.click(await screen.findByRole("button", { name: "同步修改 2 台键盘" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
    profile: expect.objectContaining({
      profile: expect.objectContaining({
        id: deviceProfile.profile.id,
        groups: expect.arrayContaining([expect.objectContaining({
          buttons: expect.arrayContaining([expect.objectContaining({ id: "ENTER", label: "新的确认" })]),
        })]),
      }),
    }),
  }));
});

test("duplicates the edited shared profile for the current keyboard", async () => {
  currentSnapshot.devices.push(device({ deviceId: "device-second", name: "后台键盘" }));
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "duplicate_profile_for_device") {
      const request = (args as { request: { device_id: string; source_profile: DeviceProfile; name: string } }).request;
      const cloned: DeviceProfile = {
        ...structuredClone(request.source_profile),
        profile: { ...request.source_profile.profile, id: "front-desk-copy", name: request.name },
      };
      currentSnapshot = {
        ...currentSnapshot,
        deviceProfiles: [currentSnapshot.deviceProfiles[0], cloned],
        devices: currentSnapshot.devices.map((item) => item.deviceId === request.device_id
          ? { ...item, runtimeAssignment: { ...item.runtimeAssignment!, device_profile_id: cloned.profile.id } }
          : item),
      };
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);

  await user.click(await screen.findByRole("button", { name: "确认，0 项行为" }));
  await user.click(screen.getByRole("button", { name: "重命名按键 确认" }));
  await user.clear(screen.getByRole("textbox", { name: "按键名称" }));
  await user.type(screen.getByRole("textbox", { name: "按键名称" }), "设备专用确认");
  await user.click(screen.getByRole("button", { name: "确认重命名" }));
  await user.click(await screen.findByRole("button", { name: "仅修改这台键盘" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("duplicate_profile_for_device", {
    request: expect.objectContaining({
      device_id: "device-front-desk",
      name: "碳膜电话键盘 (前台键盘)",
      source_profile: expect.objectContaining({
        profile: expect.objectContaining({
          id: deviceProfile.profile.id,
          groups: expect.arrayContaining([expect.objectContaining({
            buttons: expect.arrayContaining([expect.objectContaining({ id: "ENTER", label: "设备专用确认" })]),
          })]),
        }),
      }),
    }),
  }));
  expect(currentSnapshot.deviceProfiles[0].profile.groups[0].buttons.find((button) => button.id === "ENTER")?.label).toBe("确认");
});

test("keeps the original assignment and allows retrying a failed device-only copy", async () => {
  currentSnapshot.devices.push(device({ deviceId: "device-second", name: "后台键盘" }));
  let copyFailures = 1;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "duplicate_profile_for_device" && copyFailures-- > 0) throw new Error("copy failed");
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);

  await user.click(await screen.findByRole("button", { name: "确认，0 项行为" }));
  await user.click(screen.getByRole("button", { name: "重命名按键 确认" }));
  await user.clear(screen.getByRole("textbox", { name: "按键名称" }));
  await user.type(screen.getByRole("textbox", { name: "按键名称" }), "不会保存");
  await user.click(screen.getByRole("button", { name: "确认重命名" }));
  await user.click(await screen.findByRole("button", { name: "仅修改这台键盘" }));

  expect(await screen.findByText(/保存失败: copy failed/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "仅修改这台键盘" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "仅修改这台键盘" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "确认，0 项行为" })).toBeInTheDocument();
  expect(currentSnapshot.devices[0].runtimeAssignment?.device_profile_id).toBe(deviceProfile.profile.id);
  await user.click(screen.getByRole("button", { name: "仅修改这台键盘" }));
  await waitFor(() => expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "duplicate_profile_for_device")).toHaveLength(2));
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("submits a device-only shared-profile copy once and enables retry after failure", async () => {
  currentSnapshot.devices.push(device({ deviceId: "device-second", name: "后台键盘" }));
  const pendingCopy = deferred<AppSnapshot>();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "duplicate_profile_for_device") {
      return pendingCopy.promise;
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);

  await user.click(await screen.findByRole("button", { name: "确认，0 项行为" }));
  await user.click(screen.getByRole("button", { name: "重命名按键 确认" }));
  await user.clear(screen.getByRole("textbox", { name: "按键名称" }));
  await user.type(screen.getByRole("textbox", { name: "按键名称" }), "单次复制");
  await user.click(screen.getByRole("button", { name: "确认重命名" }));
  const deviceOnly = await screen.findByRole("button", { name: "仅修改这台键盘" });
  await user.dblClick(deviceOnly);

  await waitFor(() => expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "duplicate_profile_for_device")).toHaveLength(1));
  expect(screen.getByRole("dialog")).toHaveAttribute("aria-busy", "true");
  expect(deviceOnly).toBeDisabled();
  pendingCopy.resolve(structuredClone(currentSnapshot));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

test("prompts to connect a keyboard when the device registry is empty", async () => {
  currentSnapshot.devices = [];
  render(<App />);

  expect(await screen.findByText("连接你的键盘")).toBeInTheDocument();
  expect(screen.queryByRole("combobox", { name: "当前键盘" })).toBeNull();
});

test("shows all physical keyboards in the top switcher", async () => {
  currentSnapshot.devices = [
    device(),
    device({
      deviceId: "device-needs-attention",
      assignment: "invalid_assignment",
      runtime: "inactive",
    }),
    device({
      deviceId: "device-offline",
      connection: "offline",
      mode: null,
      runtime: "inactive",
    }),
  ];
  currentSnapshot.candidates = [
    {
      key: "candidate-bootloader",
      deviceId: null,
      mode: "bootloader",
      identity: "invalid_identity",
      issue: "invalid_identity",
      rawSerial: null,
      port: null,
      controllerFamilyId: "rp2040",
      boardProfileId: "vccgnd-yd-rp2040",
      latestError: null,
    },
  ];
  render(<App />);

  expect(await screen.findByRole("combobox", { name: "当前键盘" })).toHaveValue("device-front-desk");
  expect(screen.getByRole("option", { name: "前台键盘 · 分配需要修复" })).toBeInTheDocument();
  expect(screen.getByRole("option", { name: "前台键盘 · 离线" })).toBeInTheDocument();
});

test("switches physical keyboard context without persisting settings or assignments", async () => {
  const alternateProfile: DeviceProfile = {
    ...structuredClone(deviceProfile),
    profile: {
      ...deviceProfile.profile,
      id: "alternate-profile",
      name: "备用配置",
      groups: [{
        ...deviceProfile.profile.groups[0],
        buttons: [{ id: "ALT_ENTER", label: "备用确认" }],
      }],
    },
  };
  currentSnapshot.deviceProfiles.push(alternateProfile);
  currentSnapshot.devices.push(device({
    deviceId: "device-second",
    name: "备用键盘",
    hardwareSerial: "SECOND",
    runtimeAssignment: {
      device_profile_id: alternateProfile.profile.id,
      hardware_profile_id: "front-desk",
    },
  }));
  const user = userEvent.setup();
  render(<App />);

  const keyboard = await screen.findByRole("combobox", { name: "当前键盘" });
  expect(keyboard).toHaveValue("device-front-desk");
  expect(localStorage.getItem("kivo:selected-device-id")).toBeNull();
  await user.selectOptions(keyboard, "device-second");
  expect(await screen.findByRole("button", { name: "备用确认，0 项行为" })).toBeInTheDocument();
  expect(keyboard).toHaveValue("device-second");
  expect(localStorage.getItem("kivo:selected-device-id")).toBe("device-second");
  const invokedCommands = () => vi.mocked(invoke).mock.calls.map(([command]) => command);
  expect(invokedCommands()).not.toContain("save_settings");
  expect(invokedCommands()).not.toContain("save_runtime_assignment");
  expect(invokedCommands()).not.toContain("clear_runtime_assignment");
});

test("switching Home keyboards selects the assigned profile's first non-overlapping key", async () => {
  const alternateProfile: DeviceProfile = {
    ...structuredClone(deviceProfile),
    profile: {
      ...deviceProfile.profile,
      id: "alternate-profile",
      name: "备用配置",
      groups: [{ id: "alternate", columns: 1, buttons: [{ id: "ALT_ONLY", label: "备用确认" }] }],
    },
    hardware_profiles: [{
      ...structuredClone(deviceProfile.hardware_profiles[0]),
      id: "alternate-hardware",
      inputs: [{ type: "direct", id: "alternate-direct", keys: { ALT_ONLY: 6 } }],
    }],
    actions: {},
  };
  currentSnapshot.deviceProfiles.push(alternateProfile);
  currentSnapshot.devices.push(device({
    deviceId: "device-second",
    name: "备用键盘",
    hardwareSerial: "SECOND",
    runtimeAssignment: {
      device_profile_id: alternateProfile.profile.id,
      hardware_profile_id: "alternate-hardware",
    },
  }));
  const user = userEvent.setup();
  render(<App />);

  await user.selectOptions(await screen.findByRole("combobox", { name: "当前键盘" }), "device-second");
  expect(await screen.findByRole("heading", { name: "备用确认" })).toBeInTheDocument();
});

test("default Home highlights input only from the selected keyboard", async () => {
  currentSnapshot.devices.push(device({ deviceId: "device-second", hardwareSerial: "SECOND" }));
  const user = userEvent.setup();
  render(<App />);
  await user.selectOptions(await screen.findByRole("combobox", { name: "当前键盘" }), "device-second");
  const enter = screen.getByRole("button", { name: "确认，0 项行为" });

  await act(async () => emitRuntimeEvent(runtimeEvent()));
  expect(enter).not.toHaveClass("is-pressed");

  await act(async () => emitRuntimeEvent(runtimeEvent({ deviceId: "device-second", rawSerial: "SECOND" })));
  expect(enter).toHaveClass("is-pressed");
});

test("keeps a Device Management candidate detail selected without changing physical keyboard context", async () => {
  currentSnapshot.candidates = [rpCandidate({
    issue: "firmware_not_responding",
    rawSerial: "CANDIDATE-001",
  })];
  currentSnapshot.boardProfiles.push(rpBoard);
  const user = userEvent.setup();
  render(<App />);

  const keyboard = await screen.findByRole("combobox", { name: "当前键盘" });
  expect(keyboard).toHaveValue("device-front-desk");
  expect(localStorage.getItem("kivo:selected-device-id")).toBeNull();
  await user.click(screen.getByRole("button", { name: "设备" }));
  await user.click(screen.getByRole("button", { name: /001/ }));

  expect(screen.getByRole("heading", { name: "诊断信息" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "我的键盘" }));
  await user.click(screen.getByRole("button", { name: "设备" }));
  expect(screen.getByRole("heading", { name: "诊断信息" })).toBeInTheDocument();
  expect(keyboard).toHaveValue("device-front-desk");
  expect(localStorage.getItem("kivo:selected-device-id")).toBeNull();
  const invokedCommands = () => vi.mocked(invoke).mock.calls.map(([command]) => command);
  expect(invokedCommands()).not.toContain("save_settings");
  expect(invokedCommands()).not.toContain("save_runtime_assignment");
  expect(invokedCommands()).not.toContain("clear_runtime_assignment");
});

test("uses the authoritative profile-save snapshot for the device status transition", async () => {
  currentSnapshot.devices[0].runtime = "configuring";
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "save_device_profile") {
      const saved = (args as { profile: DeviceProfile }).profile;
      currentSnapshot.deviceProfiles = [saved];
      currentSnapshot.devices[0].runtime = "ready";
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  expect(await screen.findByRole("combobox", { name: "当前键盘" })).toHaveValue("device-front-desk");
  await user.click(screen.getByRole("button", { name: "我的键盘" }));
  await addHotkeyAction(user);

  await waitFor(
    () =>
      expect(screen.getByRole("option", { name: "前台键盘 · 就绪" })).toBeInTheDocument(),
    { timeout: 1600 },
  );
});

test("refreshes authoritative registry state after a runtime event", async () => {
  render(<App />);
  expect(await screen.findByRole("option", { name: "前台键盘 · 就绪" })).toBeInTheDocument();
  currentSnapshot.devices[0] = device({
    connection: "offline",
    mode: null,
    runtime: "inactive",
    port: null,
  });

  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({ code: "runtime_timeout", input: null, pressed: null }),
    ),
  );

  await waitFor(() =>
    expect(screen.getByRole("option", { name: "前台键盘 · 离线" })).toBeInTheDocument(),
  );
});

test("periodically refreshes candidates that produce no runtime event", async () => {
  vi.useFakeTimers();
  render(<App />);
  await act(async () => undefined);
  expect(screen.getByRole("combobox", { name: "当前键盘" })).toHaveValue("device-front-desk");
  currentSnapshot.candidates = [
    {
      key: "candidate-runtime",
      deviceId: null,
      mode: "runtime",
      identity: "invalid_identity",
      issue: "invalid_identity",
      rawSerial: null,
      port: "/dev/cu.candidate",
      controllerFamilyId: "rp2040",
      boardProfileId: "vccgnd-yd-rp2040",
      latestError: null,
    },
  ];

  await act(async () => vi.advanceTimersByTimeAsync(2_000));

  expect(screen.getByRole("option", { name: "前台键盘 · 就绪" })).toBeInTheDocument();
});

test("auto-opens one new Candidate once and stays dismissed for the insertion", async () => {
  const user = userEvent.setup();
  currentSnapshot.devices = [];
  currentSnapshot.candidates = [rpCandidate()];
  currentSnapshot.boardProfiles = [rpBoard];
  render(<App />);

  expect(
    await screen.findByRole("dialog", { name: "添加键盘" }),
  ).toBeInTheDocument();
  await user.click(
    within(screen.getByRole("dialog", { name: "添加键盘" })).getByRole(
      "button",
      { name: "稍后处理" },
    ),
  );
  expect(screen.queryByRole("dialog", { name: "添加键盘" })).toBeNull();
  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({ code: "topology_active", input: null, pressed: null }),
    ),
  );
  expect(screen.queryByRole("dialog", { name: "添加键盘" })).toBeNull();
});

test("does not reopen when Candidate becomes the same unassigned Device", async () => {
  currentSnapshot.devices = [];
  currentSnapshot.candidates = [rpCandidate()];
  currentSnapshot.boardProfiles = [rpBoard];
  currentSnapshot.deviceProfiles = [rpProfile];
  render(<App />);
  const dialog = await screen.findByRole("dialog", { name: "添加键盘" });
  expect(dialog).toHaveTextContent("正在确认设备");

  currentSnapshot.candidates = [];
  currentSnapshot.devices = [rpUnassignedDevice()];
  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({
        deviceId: "stable-rp",
        code: "topology_active",
        input: null,
        pressed: null,
      }),
    ),
  );

  await waitFor(() =>
    expect(screen.getByRole("dialog", { name: "添加键盘" })).toHaveTextContent(
      "第 1 步，共 3 步",
    ),
  );
  expect(screen.getAllByRole("dialog", { name: "添加键盘" })).toHaveLength(1);
});

test("advanced settings creates a profile while no device is usable", async () => {
  const user = userEvent.setup();
  currentSnapshot.devices = [];
  currentSnapshot.candidates = [
    rpCandidate({ issue: "firmware_not_responding" }),
  ];
  currentSnapshot.boardProfiles = [rpBoard];
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "稍后处理" }));
  await user.click(screen.getByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "高级设置" }));
  await user.click(screen.getByRole("button", { name: "新建配置" }));
  const profileDialog = screen.getByRole("dialog", { name: "新建配置" });
  const profileClose = within(profileDialog).getByRole("button", { name: "关闭" });
  expect(profileClose).toHaveFocus();
  await user.tab({ shift: true });
  expect(within(profileDialog).getByRole("button", { name: "创建配置" })).toHaveFocus();
  await user.click(screen.getByRole("radio", { name: "空白配置" }));
  await user.type(screen.getByRole("textbox", { name: "配置名称" }), "Offline RP");
  await user.selectOptions(screen.getByRole("combobox", { name: "板型" }), "rp");
  await user.click(screen.getByRole("button", { name: "创建配置" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("create_device_profile", {
      request: {
        kind: "blank",
        name: "Offline RP",
        board_profile_id: "rp",
      },
    }),
  );
  expect(await screen.findByRole("heading", { name: "高级设置" })).toBeInTheDocument();
  expect(screen.getByText("Offline RP")).toBeInTheDocument();
  expect(screen.queryByLabelText("当前编辑配置")).not.toBeInTheDocument();
  expect(currentSnapshot.devices).toHaveLength(0);
});

test("completes one exact Device and navigates to its keyboard workspace", async () => {
  const user = userEvent.setup();
  currentSnapshot.devices = [
    rpUnassignedDevice(),
    rpUnassignedDevice({ deviceId: "other-rp", hardwareSerial: "OTHER" }),
  ];
  currentSnapshot.candidates = [];
  currentSnapshot.boardProfiles = [rpBoard];
  currentSnapshot.deviceProfiles = [rpProfile];
  currentSnapshot.editorProfile = rpProfile.profile.id;
  render(<App />);
  const dialog = await screen.findByRole("dialog", { name: "添加键盘" });
  await user.click(within(dialog).getByRole("button", { name: "继续设置" }));
  await user.click(within(dialog).getByRole("button", { name: "下一步" }));
  await user.click(
    within(dialog).getByRole("button", { name: "完成设置" }),
  );

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("complete_device_setup", {
      deviceId: "stable-rp",
      name: expect.any(String),
      assignment: {
        device_profile_id: "rp-profile",
        hardware_profile_id: "rp-other",
      },
    }),
  );
  expect(
    currentSnapshot.devices.find((item) => item.deviceId === "other-rp")
      ?.runtimeAssignment,
  ).toBeNull();
  expect(await screen.findByRole("heading", { name: "RP Profile" })).toBeInTheDocument();
  expect(screen.queryByRole("combobox", { name: "硬件配置" })).toBeNull();
});

test("opens the selected preset as an unassigned advanced I/O draft", async () => {
  const user = userEvent.setup();
  currentSnapshot.devices = [rpUnassignedDevice()];
  currentSnapshot.candidates = [];
  currentSnapshot.boardProfiles = [rpBoard];
  currentSnapshot.deviceProfiles = [rpProfile];
  currentSnapshot.editorProfile = rpProfile.profile.id;
  render(<App />);
  const dialog = await screen.findByRole("dialog", { name: "添加键盘" });
  await user.click(within(dialog).getByRole("button", { name: "继续设置" }));
  await user.click(within(dialog).getByRole("button", { name: "下一步" }));
  await user.click(within(dialog).getByRole("button", { name: "高级 I/O 设置" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_settings", {
    settings: { schema_version: 2, editor_profile: "rp-profile", language: "zh-CN" },
  }));
  expect(invoke).not.toHaveBeenCalledWith("complete_device_setup", expect.anything());
  expect(currentSnapshot.devices[0].runtimeAssignment).toBeNull();
  expect(await screen.findByRole("tab", { name: "I/O 映射" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("combobox", { name: "硬件配置" })).toHaveValue("rp-other");
  expect(screen.getByRole("button", { name: "开始学习" })).toBeDisabled();
});

test("forwards unassigned input_state only to the open setup target", async () => {
  const user = userEvent.setup();
  currentSnapshot.devices = [rpUnassignedDevice()];
  currentSnapshot.candidates = [];
  currentSnapshot.boardProfiles = [rpBoard];
  currentSnapshot.deviceProfiles = [rpProfile];
  render(<App />);
  const dialog = await screen.findByRole("dialog", { name: "添加键盘" });
  await user.click(within(dialog).getByRole("button", { name: "继续设置" }));
  await user.click(within(dialog).getByRole("button", { name: "下一步" }));

  await act(async () =>
    emitRuntimeEvent(runtimeEvent({
      deviceId: "stable-rp",
      deviceProfileId: null,
      hardwareProfileId: null,
      input: { type: "direct", gpio: 0 },
      pressed: true,
    })),
  );
  expect(within(dialog).getByRole("button", { name: /RP Key/ })).toHaveClass("is-pressed");

  await act(async () =>
    emitRuntimeEvent(runtimeEvent({
      deviceId: "other-rp",
      deviceProfileId: null,
      hardwareProfileId: null,
      input: { type: "direct", gpio: 0 },
      pressed: false,
    })),
  );
  expect(within(dialog).getByRole("button", { name: /RP Key/ })).toHaveClass("is-pressed");
});

test("clears a retained setup input when its target disconnects", async () => {
  const user = userEvent.setup();
  const connected = rpUnassignedDevice();
  currentSnapshot.devices = [connected];
  currentSnapshot.candidates = [];
  currentSnapshot.boardProfiles = [rpBoard];
  currentSnapshot.deviceProfiles = [rpProfile];
  render(<App />);
  const dialog = await screen.findByRole("dialog", { name: "添加键盘" });
  await user.click(within(dialog).getByRole("button", { name: "继续设置" }));
  await user.click(within(dialog).getByRole("button", { name: "下一步" }));

  await act(async () =>
    emitRuntimeEvent(runtimeEvent({
      deviceId: "stable-rp",
      deviceProfileId: null,
      hardwareProfileId: null,
      input: { type: "direct", gpio: 0 },
      pressed: true,
    })),
  );
  expect(within(dialog).getByRole("button", { name: /RP Key/ })).toHaveClass("is-pressed");

  currentSnapshot.devices = [];
  await act(async () =>
    emitRuntimeEvent(runtimeEvent({
      deviceId: "other-rp",
      deviceProfileId: null,
      hardwareProfileId: null,
      input: { type: "direct", gpio: 0 },
      pressed: false,
    })),
  );
  expect(await within(dialog).findByRole("heading", { name: /键盘已断开/ })).toBeInTheDocument();

  currentSnapshot.devices = [connected];
  await act(async () =>
    emitRuntimeEvent(runtimeEvent({
      deviceId: "other-rp",
      deviceProfileId: null,
      hardwareProfileId: null,
      input: { type: "direct", gpio: 0 },
      pressed: false,
    })),
  );
  expect(await within(dialog).findByText("第 3 步，共 3 步")).toBeInTheDocument();
  await waitFor(() =>
    expect(within(dialog).getByRole("button", { name: /RP Key/ })).not.toHaveClass("is-pressed"),
  );
});

test("keeps completed setup successful when saving the Editor Profile fails", async () => {
  const defaultInvoke = vi.mocked(invoke).getMockImplementation()!;
  currentSnapshot.devices = [rpUnassignedDevice()];
  currentSnapshot.candidates = [];
  currentSnapshot.boardProfiles.push(rpBoard);
  currentSnapshot.deviceProfiles.push(rpProfile);
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "save_settings") throw new Error("settings_write_failed");
    return defaultInvoke(command, args);
  });
  const user = userEvent.setup();
  render(<App />);
  const dialog = await screen.findByRole("dialog", { name: "添加键盘" });
  await user.click(within(dialog).getByRole("button", { name: "继续设置" }));
  await user.click(within(dialog).getByRole("button", { name: "下一步" }));
  await user.click(
    within(dialog).getByRole("button", { name: "完成设置" }),
  );

  expect(
    await screen.findByText("保存失败: settings_write_failed"),
  ).toHaveClass("error-banner");
  expect(screen.queryByRole("dialog", { name: "添加键盘" })).toBeNull();
  expect(screen.getByRole("heading", { name: "RP Profile" })).toBeInTheDocument();
  expect(screen.queryByRole("combobox", { name: "硬件配置" })).toBeNull();
});

test("serializes bootstrap behind a delayed listener and existing registry refresh", async () => {
  vi.useFakeTimers();
  const listener = deferred<() => void>();
  const firstSnapshot = deferred<AppSnapshot>();
  const secondSnapshot = deferred<AppSnapshot>();
  let snapshotRequests = 0;
  vi.mocked(listen).mockReturnValue(listener.promise);
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command !== "get_snapshot") return structuredClone(currentSnapshot);
    snapshotRequests += 1;
    return snapshotRequests === 1
      ? firstSnapshot.promise
      : secondSnapshot.promise;
  });

  const { unmount } = render(<App />);
  await act(async () => vi.advanceTimersByTimeAsync(2_000));
  expect(snapshotRequests).toBe(1);

  await act(async () => {
    listener.resolve(() => undefined);
    await Promise.resolve();
  });
  expect(snapshotRequests).toBe(1);

  await act(async () => {
    firstSnapshot.resolve(structuredClone(currentSnapshot));
    await Promise.resolve();
    await Promise.resolve();
  });
  expect(snapshotRequests).toBe(2);

  await act(async () => {
    secondSnapshot.resolve(structuredClone(currentSnapshot));
    await Promise.resolve();
  });
  unmount();
});

test("disposes a listener that resolves after App unmounts without loading a snapshot", async () => {
  const listener = deferred<() => void>();
  const unlisten = vi.fn();
  vi.mocked(listen).mockReturnValue(listener.promise);

  const { unmount } = render(<App />);
  await waitFor(() => expect(listen).toHaveBeenCalledOnce());
  unmount();
  await act(async () => {
    listener.resolve(unlisten);
    await Promise.resolve();
  });

  expect(unlisten).toHaveBeenCalledOnce();
  expect(
    vi
      .mocked(invoke)
      .mock.calls.some(([command]) => command === "get_snapshot"),
  ).toBe(false);
});

test("retries a failed bootstrap as a full snapshot and clears its load error", async () => {
  vi.useFakeTimers();
  let snapshotRequests = 0;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_snapshot" && snapshotRequests++ === 0) {
      throw new Error("temporary snapshot failure");
    }
    return structuredClone(currentSnapshot);
  });

  render(<App />);
  await act(async () => undefined);

  expect(screen.getByRole("alert")).toHaveTextContent(
    "载入失败: temporary snapshot failure",
  );
  expect(screen.getByText("连接你的键盘")).toBeInTheDocument();

  await act(async () => vi.advanceTimersByTimeAsync(2_000));

  expect(screen.getByRole("heading", { name: "碳膜电话键盘" })).toBeInTheDocument();
  expect(screen.getByText("前台键盘")).toBeInTheDocument();
  expect(screen.queryByRole("alert")).toBeNull();
});

test("preserves a dirty Device Profile draft across a registry refresh", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  await addHotkeyAction(user);
  expect(
    screen.getByRole("button", { name: "2，1 项行为" }),
  ).toBeInTheDocument();

  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({ code: "topology_active", input: null, pressed: null }),
    ),
  );

  expect(
    screen.getByRole("button", { name: "2，1 项行为" }),
  ).toBeInTheDocument();
});

test("fetches selected Device metrics with an exact ID and renders its activity", async () => {
  currentSnapshot.devices.push(
    device({
      deviceId: "device-second",
      name: "Second Device",
      hardwareSerial: "SECOND",
    }),
  );
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_device_metrics") {
      const deviceId = (args as { deviceId: string }).deviceId;
      return {
        ...baseSnapshot.homeMetrics!,
        logs: [
          {
            ...baseSnapshot.homeMetrics!.logs[0],
            deviceId,
            message: `${deviceId} pressed`,
          },
        ],
      };
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设备" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("get_device_metrics", {
      deviceId: "device-front-desk",
    }),
  );
  expect(
    await screen.findByText("device-front-desk pressed"),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Second Device/ }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("get_device_metrics", {
      deviceId: "device-second",
    }),
  );
  expect(await screen.findByText("device-second pressed")).toBeInTheDocument();
});

test("clears prior Device metrics while another selected Device request is pending", async () => {
  currentSnapshot.devices.push(device({ deviceId: "device-second", name: "Second Device", hardwareSerial: "SECOND" }));
  const secondMetrics = deferred<AppSnapshot["homeMetrics"]>();
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_device_metrics") {
      return (args as { deviceId: string }).deviceId === "device-second"
        ? secondMetrics.promise
        : { ...baseSnapshot.homeMetrics!, logs: [{ ...baseSnapshot.homeMetrics!.logs[0], message: "first activity" }] };
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设备" }));
  expect(await screen.findByText("first activity")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Second Device/ }));
  expect(screen.queryByText("first activity")).toBeNull();
  await act(async () => secondMetrics.resolve({ ...baseSnapshot.homeMetrics!, logs: [{ ...baseSnapshot.homeMetrics!.logs[0], message: "second activity" }] }));
  expect(await screen.findByText("second activity")).toBeInTheDocument();
});

test("refreshes metrics only for a runtime event from the selected Device", async () => {
  let metricRequests = 0;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_device_metrics") {
      metricRequests += 1;
      return baseSnapshot.homeMetrics!;
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设备" }));
  await waitFor(() => expect(metricRequests).toBe(1));
  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({ deviceId: "other-device", rawSerial: "OTHER" }),
    ),
  );
  expect(metricRequests).toBe(1);
  await act(async () => emitRuntimeEvent(runtimeEvent()));
  await waitFor(() => expect(metricRequests).toBe(2));
});

test("rejects an older same-Device metrics response", async () => {
  const first = deferred<AppSnapshot["homeMetrics"]>();
  const second = deferred<AppSnapshot["homeMetrics"]>();
  let metricRequests = 0;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_device_metrics")
      return metricRequests++ === 0 ? first.promise : second.promise;
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设备" }));
  await waitFor(() => expect(metricRequests).toBe(1));
  await act(async () => emitRuntimeEvent(runtimeEvent()));
  await waitFor(() => expect(metricRequests).toBe(2));
  await act(async () =>
    second.resolve({
      ...baseSnapshot.homeMetrics!,
      logs: [
        { ...baseSnapshot.homeMetrics!.logs[0], message: "newer activity" },
      ],
    }),
  );
  expect(await screen.findByText("newer activity")).toBeInTheDocument();
  await act(async () =>
    first.resolve({
      ...baseSnapshot.homeMetrics!,
      logs: [
        { ...baseSnapshot.homeMetrics!.logs[0], message: "older activity" },
      ],
    }),
  );
  expect(screen.queryByText("older activity")).toBeNull();
  expect(screen.getByText("newer activity")).toBeInTheDocument();
});

test("renames one Device through the authoritative snapshot and reports failure", async () => {
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_device_metrics") return baseSnapshot.homeMetrics!;
    if (command === "rename_device") {
      const { deviceId, name } = args as { deviceId: string; name: string };
      expect(deviceId).toBe("device-front-desk");
      currentSnapshot.devices[0] = device({ name });
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设备" }));
  await user.click(screen.getByRole("button", { name: "重命名设备" }));
  await user.clear(screen.getByRole("textbox", { name: "设备名称" }));
  await user.type(screen.getByRole("textbox", { name: "设备名称" }), "Renamed");
  await user.click(screen.getByRole("button", { name: "确认重命名" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("rename_device", {
      deviceId: "device-front-desk",
      name: "Renamed",
    }),
  );
  expect(screen.getByRole("heading", { name: "Renamed" })).toBeInTheDocument();
  vi.mocked(invoke).mockRejectedValueOnce(new Error("rename denied"));
  await user.click(screen.getByRole("button", { name: "重命名设备" }));
  await user.click(screen.getByRole("button", { name: "确认重命名" }));
  expect(await screen.findByText("保存失败: rename denied")).toHaveClass(
    "error-banner",
  );
});

test("does not expose a clear runtime assignment action", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设备" }));
  expect(screen.queryByRole("button", { name: "保存运行分配" })).toBeNull();
  expect(screen.queryByRole("dialog", { name: "保存运行分配" })).toBeNull();
  expect(screen.queryByRole("button", { name: "清除运行分配" })).toBeNull();
  expect(screen.queryByRole("dialog", { name: "清除运行分配" })).toBeNull();
});

test("forgets only the confirmed offline Device and keeps failure retryable", async () => {
  currentSnapshot.devices[0] = device({
    connection: "offline",
    mode: null,
    runtime: "inactive",
    port: null,
  });
  let rejectForget = false;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_device_metrics") return baseSnapshot.homeMetrics!;
    if (command === "forget_device") {
      expect(args).toEqual({ deviceId: "device-front-desk" });
      if (rejectForget) throw new Error("still connected");
      currentSnapshot.devices = [];
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设备" }));
  await user.click(screen.getByRole("button", { name: "忘记设备" }));
  await user.click(
    screen
      .getByRole("dialog", { name: "忘记设备" })
      .querySelector("button.danger-button")!,
  );
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("forget_device", {
      deviceId: "device-front-desk",
    }),
  );
  expect(screen.queryByRole("button", { name: /前台键盘/ })).toBeNull();
  currentSnapshot.devices = [
    device({
      connection: "offline",
      mode: null,
      runtime: "inactive",
      port: null,
    }),
  ];
  rejectForget = true;
  await act(async () =>
    emitRuntimeEvent(runtimeEvent({ deviceId: "device-front-desk" })),
  );
  await user.click(await screen.findByRole("button", { name: "忘记设备" }));
  await user.click(
    screen
      .getByRole("dialog", { name: "忘记设备" })
      .querySelector("button.danger-button")!,
  );
  expect(await screen.findByText("删除失败: still connected")).toHaveClass(
    "error-banner",
  );
  expect(screen.getByRole("dialog", { name: "忘记设备" })).toBeInTheDocument();
});

test("selecting the Editor Profile saves only the version-2 editor settings patch", async () => {
  const secondProfile: DeviceProfile = {
    ...structuredClone(deviceProfile),
    profile: {
      ...deviceProfile.profile,
      id: "operator-console",
      name: "接线员控制台",
    },
  };
  currentSnapshot.deviceProfiles.push(secondProfile);
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "高级设置" }));
  await user.click(screen.getByRole("button", { name: "选择 接线员控制台" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_settings", {
      settings: {
        schema_version: 2,
        editor_profile: secondProfile.profile.id,
        language: "zh-CN",
      },
    }),
  );
  expect(
    vi
      .mocked(invoke)
      .mock.calls.some(
        ([command]) =>
          command === "save_runtime_assignment" ||
          command === "clear_runtime_assignment",
      ),
  ).toBe(false);
  expect(currentSnapshot.devices[0].runtimeAssignment).toEqual(
    baseSnapshot.devices[0].runtimeAssignment,
  );
});

test("moves language and backup controls into the ordinary settings workspace", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设置" }));

  expect(screen.getByRole("heading", { name: "设置" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "高级设置" })).toBeInTheDocument();
  await user.selectOptions(screen.getByRole("combobox", { name: "语言" }), "en-US");
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_settings", {
    settings: { schema_version: 2, editor_profile: deviceProfile.profile.id, language: "en-US" },
  }));
  expect(currentSnapshot.devices[0].runtimeAssignment).toEqual(baseSnapshot.devices[0].runtimeAssignment);
});

test("renders the assigned keyboard layout in model order", async () => {
  currentSnapshot.deviceProfiles[0].profile.groups = [
    {
      id: "digits",
      columns: 2,
      buttons: [
        { id: "DIGIT_2", label: "2" },
        { id: "DIGIT_5", label: "5" },
      ],
    },
    { id: "actions", columns: 1, buttons: [{ id: "ENTER", label: "确认" }] },
  ];
  render(<App />);

  await screen.findByRole("heading", { name: "碳膜电话键盘" });
  expect(
    [...document.querySelectorAll(".key-button")].map(
      (item) => item.textContent,
    ),
  ).toEqual([
    expect.stringContaining("2"),
    expect.stringContaining("5"),
    expect.stringContaining("确认"),
  ]);
  expect(screen.queryByLabelText("当前编辑配置")).toBeNull();
  expect(screen.queryByLabelText("语言")).toBeNull();
});

test("shows the selected keyboard when only another profile's Device is online", async () => {
  const secondProfile: DeviceProfile = {
    ...structuredClone(deviceProfile),
    profile: {
      ...deviceProfile.profile,
      id: "operator-console",
      name: "接线员控制台",
    },
    hardware_profiles: [
      {
        ...structuredClone(deviceProfile.hardware_profiles[0]),
        id: "operator-hardware",
      },
    ],
  };
  currentSnapshot.deviceProfiles.push(secondProfile);
  currentSnapshot.devices = [
    device({
      deviceId: "device-operator-console",
      port: "/dev/cu.unrelated",
      runtimeAssignment: {
        device_profile_id: secondProfile.profile.id,
        hardware_profile_id: secondProfile.hardware_profiles[0].id,
      },
    }),
  ];

  render(<App />);

  expect(await screen.findByRole("heading", { name: "接线员控制台" })).toBeInTheDocument();
  expect(screen.queryByText("/dev/cu.unrelated")).toBeNull();
});

test("selects the ready keyboard over an offline keyboard", async () => {
  currentSnapshot.devices = [
    device({
      deviceId: "device-offline",
      connection: "offline",
      mode: null,
      runtime: "inactive",
      port: null,
    }),
    device({
      deviceId: "device-online",
      hardwareSerial: "ONLINE",
      port: "/dev/cu.online",
    }),
  ];

  render(<App />);

  expect(await screen.findByRole("heading", { name: "碳膜电话键盘" })).toBeInTheDocument();
  expect(screen.getByText("前台键盘")).toBeInTheDocument();
  expect(screen.queryByText("/dev/cu.online")).toBeNull();
});

test("projects pressed feedback to the selected physical Device", async () => {
  currentSnapshot.devices.push(
    device({ deviceId: "device-second", hardwareSerial: "SECOND" }),
  );
  const user = userEvent.setup();
  render(<App />);
  await user.selectOptions(await screen.findByRole("combobox", { name: "当前键盘" }), "device-second");
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  const enter = screen.getByRole("button", { name: "确认，0 项行为" });

  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({ deviceId: "device-front-desk", pressed: true }),
    ),
  );
  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({
        deviceId: "device-second",
        rawSerial: "SECOND",
        pressed: true,
      }),
    ),
  );
  expect(enter).toHaveClass("is-pressed");

  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({
        deviceId: "device-second",
        rawSerial: "SECOND",
        pressed: false,
      }),
    ),
  );
  expect(enter).not.toHaveClass("is-pressed");
});

test("clears pressed feedback only for the disconnected Device regardless of the managed row", async () => {
  currentSnapshot.devices.push(device({
    deviceId: "device-second",
    name: "Second Device",
    hardwareSerial: "SECOND",
  }));
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设备" }));
  await user.click(screen.getByRole("button", { name: /前台键盘/ }));
  await user.click(screen.getByRole("button", { name: "我的键盘" }));
  const enter = screen.getByRole("button", { name: "确认，0 项行为" });

  await act(async () => emitRuntimeEvent(runtimeEvent()));
  await act(async () => emitRuntimeEvent(runtimeEvent({
    deviceId: "device-second",
    rawSerial: "SECOND",
  })));
  currentSnapshot.devices[1] = device({
    deviceId: "device-second",
    name: "Second Device",
    hardwareSerial: "SECOND",
    connection: "offline",
    mode: null,
    runtime: "inactive",
    port: null,
  });
  await act(async () => emitRuntimeEvent(runtimeEvent({
    code: "topology_active",
    input: null,
    pressed: null,
  })));
  await waitFor(() => expect(enter).toHaveClass("is-pressed"));

  currentSnapshot.devices[0] = device({
    connection: "offline",
    mode: null,
    runtime: "inactive",
    port: null,
  });
  await act(async () => emitRuntimeEvent(runtimeEvent({
    code: "topology_active",
    input: null,
    pressed: null,
  })));
  await waitFor(() => expect(enter).not.toHaveClass("is-pressed"));
});

test("keeps Home scoped to the selected Device profile while retaining runtime attribution", async () => {
  const otherProfile: DeviceProfile = {
    ...structuredClone(deviceProfile),
    profile: { ...deviceProfile.profile, id: "other-profile", name: "其他键盘" },
  };
  currentSnapshot.deviceProfiles.push(otherProfile);
  currentSnapshot.devices.push(device({
    deviceId: "device-other-profile",
    name: "其他设备",
    hardwareSerial: "OTHER",
    runtimeAssignment: {
      device_profile_id: otherProfile.profile.id,
      hardware_profile_id: "front-desk",
    },
  }));
  const otherMetrics = {
    ...structuredClone(baseSnapshot.homeMetrics!),
    totalPresses: 99,
    logs: [{
      ...baseSnapshot.homeMetrics!.logs[0],
      deviceId: "device-other-profile",
      deviceName: "其他设备",
      deviceProfileId: otherProfile.profile.id,
      message: "other-profile activity",
    }],
  };
  let metricRequests = 0;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_device_metrics") {
      metricRequests += 1;
      return structuredClone(baseSnapshot.homeMetrics!);
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设备" }));
  await user.selectOptions(screen.getByRole("combobox", { name: "当前键盘" }), "device-other-profile");
  await waitFor(() => expect(metricRequests).toBe(2));
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  const enter = screen.getByRole("button", { name: "确认，0 项行为" });

  await act(async () => emitRuntimeEvent(runtimeEvent({
    deviceId: "device-other-profile",
    rawSerial: "OTHER",
    deviceProfileId: otherProfile.profile.id,
    homeUpdate: otherMetrics,
  })));
  expect(enter).toHaveClass("is-pressed");
  await waitFor(() => expect(metricRequests).toBe(3));

  await user.click(screen.getByRole("button", { name: "我的键盘" }));
  expect(screen.getByRole("heading", { name: "其他键盘" })).toBeInTheDocument();
  expect(screen.queryByText("other-profile activity")).toBeNull();

  await user.click(screen.getByRole("button", { name: "我的键盘" }));
  await user.selectOptions(screen.getByRole("combobox", { name: "当前键盘" }), "device-front-desk");
  await act(async () => emitRuntimeEvent(runtimeEvent()));
  expect(screen.getByRole("button", { name: "确认，0 项行为" })).toHaveClass("is-pressed");
});

test("does not turn learning captures into runtime keypad feedback", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  const enter = screen.getByRole("button", { name: "确认，0 项行为" });

  await act(async () => emitRuntimeEvent(runtimeEvent({
    code: "learning_input",
    learningTarget: {
      deviceId: "device-front-desk",
      deviceProfileId: deviceProfile.profile.id,
      hardwareProfileId: "front-desk",
      editingRevision: 1,
      firmwareRevision: 1,
      pins: [6],
    },
  })));

  expect(enter).not.toHaveClass("is-pressed");
});

test("resolves runtime input from the event Hardware Profile and rejects assignment mismatch", async () => {
  currentSnapshot.deviceProfiles[0].hardware_profiles.push({
    id: "alternate-hardware",
    name: "备用硬件配置",
    board_profile_id: "luatos-esp32s3-aio",
    debounce_ms: 30,
    inputs: [{ type: "direct", id: "alternate", keys: { ENTER: 7 } }],
  });
  currentSnapshot.devices[0].runtimeAssignment = {
    device_profile_id: deviceProfile.profile.id,
    hardware_profile_id: "alternate-hardware",
  };
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  const enter = screen.getByRole("button", { name: "确认，0 项行为" });

  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({
        hardwareProfileId: "front-desk",
        input: { type: "direct", gpio: 6 },
      }),
    ),
  );
  expect(enter).not.toHaveClass("is-pressed");

  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({
        hardwareProfileId: "alternate-hardware",
        input: { type: "direct", gpio: 7 },
      }),
    ),
  );
  expect(enter).toHaveClass("is-pressed");
});

test("learns input only for the exact selected Device Profile and Hardware Profile target", async () => {
  const activeLearningTarget = {
    deviceId: "device-front-desk",
    deviceProfileId: deviceProfile.profile.id,
    hardwareProfileId: "front-desk",
    editingRevision: 7,
    firmwareRevision: 11,
    pins: [2, 13],
  };
  currentSnapshot.devices[0].learning = activeLearningTarget;
  const user = userEvent.setup();
  render(<App />);
  await openDeviceIo(user);
  await user.click(screen.getByText("适配新设备"));
  await user.selectOptions(screen.getByLabelText("在线设备"), "device-front-desk");
  const digitA = screen.getByRole("combobox", { name: "2 A" });
  const digitB = screen.getByRole("combobox", { name: "2 B" });
  expect(digitA).toHaveValue("1");
  expect(digitB).toHaveValue("12");

  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({
        deviceId: "device-other",
        rawSerial: "OTHER",
        code: "learning_input",
        input: { type: "contact", source: 1, pin_a: 2, pin_b: 13 },
        learningTarget: {
          deviceId: "device-other",
          deviceProfileId: deviceProfile.profile.id,
          hardwareProfileId: "front-desk",
          editingRevision: 0,
          firmwareRevision: 0,
          pins: [2, 13],
        },
      }),
    ),
  );
  expect(digitA).toHaveValue("1");
  expect(digitB).toHaveValue("12");

  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({
        code: "learning_input",
        input: { type: "contact", source: 1, pin_a: 2, pin_b: 13 },
        learningTarget: { ...activeLearningTarget, editingRevision: 6 },
      }),
    ),
  );
  expect(digitA).toHaveValue("1");
  expect(digitB).toHaveValue("12");

  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({
        code: "learning_input",
        input: { type: "contact", source: 1, pin_a: 2, pin_b: 13 },
        learningTarget: activeLearningTarget,
      }),
    ),
  );
  expect(digitA).toHaveValue("2");
  expect(digitB).toHaveValue("13");

  const directSource = screen
    .getByRole("combobox", { name: "确认 GPIO" })
    .closest(".source-editor");
  expect(directSource).not.toBeNull();
  await user.click(
    within(directSource as HTMLElement).getByRole("button", {
      name: "确认",
    }),
  );
  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({
        code: "learning_input",
        input: { type: "direct", gpio: 13 },
        learningTarget: activeLearningTarget,
      }),
    ),
  );
  expect(screen.getByRole("combobox", { name: "确认 GPIO" })).toHaveValue("13");
});

test("builds an ordered action list and autosaves it", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  await screen.findByRole("button", { name: "2，0 项行为" });
  await screen.findByRole("complementary", { name: "2" });

  await addPasteAction(user, "你好");
  await addHotkeyAction(user);

  await waitFor(
    () =>
      expect(invoke).toHaveBeenCalledWith("save_device_profile", {
        profile: expect.objectContaining({
          actions: {
            DIGIT_2: {
              press: [
                { type: "paste", text: "你好" },
                { type: "hotkey", keys: ["enter"] },
              ],
              release: [], long_press: [], double_press: [],
            },
          },
        }),
      }),
    { timeout: 1600 },
  );
  expect(
    screen.getByRole("button", { name: "2，2 项行为" }),
  ).toBeInTheDocument();
});

test("autosaves Button Behavior after Device Management enables shared-profile confirmation", async () => {
  currentSnapshot.devices.push(device({
    deviceId: "device-second",
    name: "后台键盘",
    hardwareSerial: "SECOND",
    port: "/dev/cu.second",
  }));
  const user = userEvent.setup();
  render(<App />);

  await openDeviceIo(user);
  await user.click(screen.getByRole("button", { name: "我的键盘" }));
  await addPasteAction(user, "共享配置的新行为");
  await user.click(await screen.findByRole("button", { name: "同步修改 2 台键盘" }));

  await waitFor(
    () => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
      profile: expect.objectContaining({
        actions: expect.objectContaining({
          DIGIT_2: expect.objectContaining({
            press: [{ type: "paste", text: "共享配置的新行为" }],
          }),
        }),
      }),
    }),
    { timeout: 1600 },
  );
  expect(screen.getByText("已自动保存")).toBeInTheDocument();
});

test("autosaves a uniquely assigned managed profile that is not the Editor Profile", async () => {
  const managedProfile = structuredClone(deviceProfile);
  managedProfile.profile = {
    ...managedProfile.profile,
    id: "managed-only",
    name: "设备专用配置",
  };
  currentSnapshot.deviceProfiles.push(managedProfile);
  currentSnapshot.devices[0].runtimeAssignment = {
    device_profile_id: managedProfile.profile.id,
    hardware_profile_id: "front-desk",
  };
  const user = userEvent.setup();
  render(<App />);

  await openDeviceIo(user);
  await user.selectOptions(screen.getByRole("combobox", { name: "2 A" }), "2");

  await waitFor(
    () => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
      profile: expect.objectContaining({
        profile: expect.objectContaining({ id: "managed-only" }),
        hardware_profiles: [expect.objectContaining({
          inputs: expect.arrayContaining([
            expect.objectContaining({ keys: { DIGIT_2: [2, 12] } }),
          ]),
        })],
      }),
    }),
    { timeout: 1600 },
  );
});

test("keeps a multi-profile autosave failure visible and retryable", async () => {
  const secondProfile = structuredClone(deviceProfile);
  secondProfile.profile = {
    ...secondProfile.profile,
    id: "managed-second",
    name: "后台配置",
  };
  currentSnapshot.deviceProfiles.push(secondProfile);
  currentSnapshot.devices.push(device({
    deviceId: "device-second",
    name: "后台键盘",
    hardwareSerial: "SECOND",
    port: "/dev/cu.second",
    runtimeAssignment: {
      device_profile_id: secondProfile.profile.id,
      hardware_profile_id: "front-desk",
    },
  }));
  let secondProfileFailures = 1;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "save_device_profile") {
      const saved = (args as { profile: DeviceProfile }).profile;
      if (saved.profile.id === secondProfile.profile.id && secondProfileFailures-- > 0) {
        throw new Error("second profile failed");
      }
      currentSnapshot.deviceProfiles = currentSnapshot.deviceProfiles.map(
        (item) => item.profile.id === saved.profile.id ? saved : item,
      );
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);

  await openDeviceIo(user);
  vi.useFakeTimers();
  fireEvent.change(screen.getByRole("combobox", { name: "2 A" }), { target: { value: "2" } });
  fireEvent.change(screen.getByRole("combobox", { name: "当前键盘" }), { target: { value: "device-second" } });
  fireEvent.change(screen.getByRole("combobox", { name: "2 A" }), { target: { value: "6" } });
  await act(() => vi.advanceTimersByTimeAsync(400));

  expect(screen.getByText("保存失败")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "重试" }));
  await act(async () => Promise.resolve());

  expect(
    vi.mocked(invoke).mock.calls.filter(([command]) => command === "save_device_profile"),
  ).toEqual(expect.arrayContaining([
    ["save_device_profile", { profile: expect.objectContaining({ profile: expect.objectContaining({ id: deviceProfile.profile.id }) }) }],
    ["save_device_profile", { profile: expect.objectContaining({ profile: expect.objectContaining({ id: secondProfile.profile.id }) }) }],
  ]));
  const savedProfileIds = vi.mocked(invoke).mock.calls
    .filter(([command]) => command === "save_device_profile")
    .map(([, args]) => (args as { profile: DeviceProfile }).profile.profile.id);
  expect(savedProfileIds.filter((profileId) => profileId === deviceProfile.profile.id)).toHaveLength(1);
  expect(savedProfileIds.filter((profileId) => profileId === secondProfile.profile.id)).toHaveLength(2);
  expect(screen.getByText("已自动保存")).toBeInTheDocument();
});

test("records a shortcut from the application window", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  const editor = await screen.findByRole("complementary", { name: "2" });

  await openActionDialog(user, "hotkey");
  await user.click(screen.getByRole("button", { name: "录入快捷键" }));
  fireEvent.keyDown(window, {
    code: "KeyK",
    key: "k",
    metaKey: true,
    shiftKey: true,
  });

  fireEvent.keyUp(window, { code: "KeyK", key: "k", metaKey: true, shiftKey: true });
  expect(screen.getByRole("button", { name: "移除 K" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "保存" }));
});

test("manually selects a multi-modifier shortcut", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  const editor = await screen.findByRole("complementary", { name: "2" });

  await openActionDialog(user, "hotkey");
  await user.click(screen.getByRole("checkbox", { name: "cmd" }));
  await user.click(screen.getByRole("checkbox", { name: "ctrl" }));
  await user.click(screen.getByRole("checkbox", { name: "shift" }));
  await user.click(screen.getByRole("tab", { name: "字母" }));
  await user.click(screen.getByRole("checkbox", { name: "K" }));

  expect(
    screen.getByRole("button", { name: "移除 K" }),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(
    () =>
      expect(invoke).toHaveBeenCalledWith("save_device_profile", {
        profile: expect.objectContaining({
          actions: {
            DIGIT_2: {
              press: [{ type: "hotkey", keys: ["cmd", "ctrl", "shift", "k"] }],
              release: [], long_press: [], double_press: [],
            },
          },
        }),
      }),
    { timeout: 1600 },
  );
});

test("manually selects the backtick key", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  const editor = await screen.findByRole("complementary", { name: "2" });

  await openActionDialog(user, "hotkey");
  await user.click(screen.getByRole("tab", { name: "符号" }));
  await user.click(screen.getByRole("checkbox", { name: "`" }));

  expect(screen.getByRole("button", { name: "移除 `" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(
    () =>
      expect(invoke).toHaveBeenCalledWith("save_device_profile", {
        profile: expect.objectContaining({
          actions: {
            DIGIT_2: {
              press: [{ type: "hotkey", keys: ["backtick"] }],
              release: [], long_press: [], double_press: [],
            },
          },
        }),
      }),
    { timeout: 1600 },
  );
});

test("reorders actions from the right editor", async () => {
  const user = userEvent.setup();
  currentSnapshot.deviceProfiles[0].actions.DIGIT_2 = {
    press: [
      { type: "paste", text: "先粘贴" },
      { type: "hotkey", keys: ["enter"] },
    ],
    release: [], long_press: [], double_press: [],
  };
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  const editor = await screen.findByRole("complementary", { name: "2" });

  await user.click(within(editor).getAllByRole("button", { name: "上移" })[1]);

  await waitFor(
    () =>
      expect(invoke).toHaveBeenCalledWith("save_device_profile", {
        profile: expect.objectContaining({
          actions: {
            DIGIT_2: {
              press: [
                { type: "hotkey", keys: ["enter"] },
                { type: "paste", text: "先粘贴" },
              ],
              release: [], long_press: [], double_press: [],
            },
          },
        }),
      }),
    { timeout: 1600 },
  );
});

test("keeps a failed autosave and exposes retry", async () => {
  const user = userEvent.setup();
  let saveAttempts = 0;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "save_device_profile" && saveAttempts++ === 0)
      throw new Error("disk full");
    return structuredClone(currentSnapshot);
  });
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  const key = await screen.findByRole("button", { name: "2，0 项行为" });

  await user.click(key);
  await addHotkeyAction(user);
  expect(
    await screen.findByText("保存失败", {}, { timeout: 1600 }),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "重试" }));

  await waitFor(() =>
    expect(
      vi
        .mocked(invoke)
        .mock.calls.filter(([command]) => command === "save_device_profile"),
    ).toHaveLength(2),
  );
});

test("previews a device profile before importing it from advanced settings", async () => {
  const user = userEvent.setup();
  vi.mocked(open).mockResolvedValue("/tmp/device-profile.yaml");
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "preview_device_profile_import")
      return {
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
  await screen.findByRole("button", { name: "设置" });
  await user.click(screen.getByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "高级设置" }));
  await user.click(screen.getByRole("button", { name: "导入配置" }));
  const dialog = await screen.findByRole("dialog", {
    name: "替换现有设备配置",
  });
  expect(
    within(dialog).getByText("22 个按键，22 项硬件配置，8 项行为"),
  ).toBeInTheDocument();
  expect(dialog).not.toHaveTextContent("设备，");
  expect(dialog).not.toHaveTextContent("指标行");
  expect(dialog).not.toHaveTextContent("动态");
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("import_device_profile", {
      path: "/tmp/device-profile.yaml",
    }),
  );
});

test("previews a full backup before restoring it from settings", async () => {
  const user = userEvent.setup();
  vi.mocked(open).mockResolvedValue("/tmp/backup.yaml");
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "preview_backup")
      return {
        profileCount: 3,
        buttonCount: 44,
        hardwareBindingCount: 40,
        actionCount: 19,
        deviceCount: 4,
        assignmentCount: 3,
        metricRowCount: 12,
        activityCount: 9,
      };
    return structuredClone(currentSnapshot);
  });
  render(<App />);
  await screen.findByRole("button", { name: "设置" });
  await user.click(screen.getByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "恢复" }));
  const dialog = await screen.findByRole("dialog", { name: "恢复全量备份" });
  expect(
    within(dialog).getByText(
      "3 个设备配置，44 个按键，40 项硬件配置，19 项行为，4 台设备，3 项运行分配，12 行指标，9 条动态",
    ),
  ).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("restore_backup", {
      path: "/tmp/backup.yaml",
    }),
  );
});

test("deletes the last device profile and keeps advanced profile actions available", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByRole("button", { name: "设置" });
  await user.click(screen.getByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "高级设置" }));
  await user.click(screen.getByRole("button", { name: /删除.*碳膜电话键盘/ }));
  const dialog = await screen.findByRole("dialog", { name: "删除设备配置" });
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  expect(await screen.findByText("还没有设备配置")).toBeInTheDocument();
  expect(
    screen.getAllByRole("button", { name: "导入配置" }).length,
  ).toBeGreaterThan(0);
  expect(screen.queryByRole("button", { name: "恢复" })).toBeNull();
});

test("keeps key learning secondary and collapsed by default", async () => {
  const user = userEvent.setup();
  render(<App />);

  await openDeviceIo(user);

  expect(screen.getByText("直连 GPIO")).toBeInTheDocument();
  expect(screen.getByText("接触矩阵")).toBeInTheDocument();
  expect(screen.getByText("适配新设备").closest("details")).not.toHaveAttribute(
    "open",
  );
});

test("autosaves a newly added Hardware Profile with its compiled Board Profile", async () => {
  const user = userEvent.setup();
  render(<App />);
  await openDeviceIo(user);

  await user.click(screen.getByRole("button", { name: "添加硬件配置" }));

  await waitFor(
    () => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
      profile: expect.objectContaining({
        hardware_profiles: expect.arrayContaining([
          expect.objectContaining({
            id: "luatos-esp32s3-aio-hardware",
            name: "LuatOS ESP32-S3 AIO 硬件配置",
            board_profile_id: "luatos-esp32s3-aio",
          }),
        ]),
      }),
    }),
    { timeout: 1600 },
  );
});

test("blocks autosave while a Board Profile change leaves invalid mappings and permits revert", async () => {
  currentSnapshot.boardProfiles.push({
    id: "vccgnd-yd-rp2040",
    controllerFamilyId: "rp2040",
    displayName: "YD-RP2040",
    runtimeUsb: "2e8a:000a",
    bootloaderUsb: "2e8a:0003",
    safePins: [0, 1, 2],
  });
  const user = userEvent.setup();
  render(<App />);
  await openDeviceIo(user);
  vi.mocked(invoke).mockClear();

  await user.selectOptions(screen.getByLabelText("板型"), "vccgnd-yd-rp2040");
  expect(await screen.findByText("无效 GPIO 6")).toBeInTheDocument();
  await new Promise((resolve) => setTimeout(resolve, 550));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);

  await user.selectOptions(screen.getByLabelText("板型"), "luatos-esp32s3-aio");
  expect(screen.queryByText("无效 GPIO 6")).toBeNull();
  await new Promise((resolve) => setTimeout(resolve, 550));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);
});

test("deletes a Hardware Profile without repairing its Device assignment", async () => {
  currentSnapshot.deviceProfiles[0].hardware_profiles.push({
    id: "spare",
    name: "备用硬件配置",
    board_profile_id: "luatos-esp32s3-aio",
    debounce_ms: 30,
    inputs: [],
  });
  const assignment = structuredClone(currentSnapshot.devices[0].runtimeAssignment);
  const user = userEvent.setup();
  render(<App />);
  await openDeviceIo(user);

  await user.click(screen.getByRole("button", { name: "删除硬件配置" }));
  await user.click(within(screen.getByRole("dialog", { name: "删除硬件配置" })).getByRole("button", { name: "确认" }));

  await waitFor(
    () => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
      profile: expect.objectContaining({
        hardware_profiles: [expect.objectContaining({ id: "spare" })],
      }),
    }),
    { timeout: 1600 },
  );
  expect(currentSnapshot.devices[0].runtimeAssignment).toEqual(assignment);
});

test("preserves an unsaved Device Profile draft when learning begins", async () => {
  const user = userEvent.setup();
  render(<App />);
  await openDeviceIo(user);

  fireEvent.change(screen.getByLabelText("消抖"), { target: { value: "31" } });
  await user.click(screen.getByText("适配新设备"));
  await user.selectOptions(screen.getByLabelText("在线设备"), "device-front-desk");
  await user.click(screen.getByRole("checkbox", { name: "键盘已与原电话电路及外部电压完全隔离" }));
  await user.click(screen.getByRole("checkbox", { name: "GPIO 2" }));
  await user.click(screen.getByRole("button", { name: "开始学习" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("begin_learning", expect.objectContaining({
    deviceId: "device-front-desk",
    deviceProfileId: deviceProfile.profile.id,
    hardwareProfileId: "front-desk",
    pins: [2],
  })));
  const beginArgs = vi.mocked(invoke).mock.calls.find(([command]) => command === "begin_learning")?.[1] as {
    editingRevision: number;
  };
  expect(beginArgs.editingRevision).toBeGreaterThan(0);
  expect(screen.getByLabelText("消抖")).toHaveValue(31);
});

test("isolates learning lifecycle and defers the captured draft until a later ordinary save", async () => {
  currentSnapshot.devices.push(device({
    deviceId: "device-second",
    name: "后台键盘",
    hardwareSerial: "SECOND",
    port: "/dev/cu.second",
  }));
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "begin_learning") {
      const target = args as {
        deviceId: string;
        deviceProfileId: string;
        hardwareProfileId: string;
        editingRevision: number;
        pins: number[];
      };
      const selected = currentSnapshot.devices.find(({ deviceId }) => deviceId === target.deviceId)!;
      selected.learning = { ...target, firmwareRevision: 23 };
      selected.runtime = "learning";
    }
    if (command === "end_learning") {
      const selected = currentSnapshot.devices.find(
        ({ deviceId }) => deviceId === (args as { deviceId: string }).deviceId,
      )!;
      selected.learning = null;
      selected.runtime = "ready";
    }
    if (command === "save_device_profile") {
      const saved = (args as { profile: DeviceProfile }).profile;
      currentSnapshot.deviceProfiles = currentSnapshot.deviceProfiles.map((profile) =>
        profile.profile.id === saved.profile.id ? saved : profile
      );
      currentSnapshot.devices = currentSnapshot.devices.map((item) => ({
        ...item,
        runtime: "configuring",
      }));
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await openDeviceIo(user);
  await user.click(screen.getByText("适配新设备"));
  await user.selectOptions(screen.getByLabelText("在线设备"), "device-second");
  await user.click(screen.getByRole("checkbox", { name: "键盘已与原电话电路及外部电压完全隔离" }));
  await user.click(screen.getByRole("checkbox", { name: "GPIO 2" }));
  await user.click(screen.getByRole("checkbox", { name: "GPIO 13" }));
  await user.click(screen.getByRole("button", { name: "开始学习" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("begin_learning", expect.objectContaining({
    deviceId: "device-second",
    deviceProfileId: deviceProfile.profile.id,
    hardwareProfileId: "front-desk",
    pins: [2, 13],
  })));
  const target = currentSnapshot.devices[1].learning!;
  expect(target.editingRevision).toBeGreaterThan(0);

  await user.click(screen.getByRole("button", { name: "设备" }));
  expect(screen.getByRole("button", { name: /前台键盘.*就绪/ })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /后台键盘.*正在学习/ })).toBeInTheDocument();
  await openDeviceIo(user);
  await user.click(screen.getByText("适配新设备"));
  await user.selectOptions(screen.getByLabelText("在线设备"), "device-second");

  await act(async () => emitRuntimeEvent(runtimeEvent({
    deviceId: "device-second",
    rawSerial: "SECOND",
    code: "learning_input",
    input: { type: "contact", source: 1, pin_a: 2, pin_b: 13 },
    learningTarget: target,
  })));
  expect(screen.getByRole("combobox", { name: "2 A" })).toHaveValue("2");
  expect(screen.getByRole("combobox", { name: "2 B" })).toHaveValue("13");
  await new Promise((resolve) => setTimeout(resolve, 550));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);

  await user.click(screen.getByRole("button", { name: "结束学习" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("end_learning", { deviceId: "device-second" }));
  expect(vi.mocked(invoke).mock.calls.some(([command, args]) =>
    command === "end_learning" && (args as { deviceId: string }).deviceId === "device-front-desk"
  )).toBe(false);
  await new Promise((resolve) => setTimeout(resolve, 550));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);
  expect(screen.getByRole("combobox", { name: "2 B" })).toHaveValue("13");

  fireEvent.change(screen.getByLabelText("消抖"), { target: { value: "31" } });
  await user.click(await screen.findByRole("button", { name: "同步修改 2 台键盘" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
    profile: expect.objectContaining({
      hardware_profiles: [expect.objectContaining({
        debounce_ms: 31,
        inputs: expect.arrayContaining([
          expect.objectContaining({ keys: { DIGIT_2: [2, 13] } }),
        ]),
      })],
    }),
  }), { timeout: 1600 });
  await user.click(screen.getByRole("button", { name: "设备" }));
  expect(screen.getByRole("button", { name: /前台键盘.*正在配置/ })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /后台键盘.*正在配置/ })).toBeInTheDocument();
});

test("keeps a captured draft when only its learning Device disconnects", async () => {
  const target = {
    deviceId: "device-second",
    deviceProfileId: deviceProfile.profile.id,
    hardwareProfileId: "front-desk",
    editingRevision: 41,
    firmwareRevision: 7,
    pins: [2, 13],
  };
  currentSnapshot.devices.push(device({
    deviceId: "device-second",
    name: "后台键盘",
    hardwareSerial: "SECOND",
    port: "/dev/cu.second",
    runtime: "learning",
    learning: target,
  }));
  const user = userEvent.setup();
  render(<App />);
  await openDeviceIo(user);
  await user.click(screen.getByText("适配新设备"));
  await user.selectOptions(screen.getByLabelText("在线设备"), "device-second");
  await act(async () => emitRuntimeEvent(runtimeEvent({
    deviceId: "device-second",
    rawSerial: "SECOND",
    code: "learning_input",
    input: { type: "contact", source: 1, pin_a: 2, pin_b: 13 },
    learningTarget: target,
  })));

  currentSnapshot.devices[1] = device({
    deviceId: "device-second",
    name: "后台键盘",
    hardwareSerial: "SECOND",
    connection: "offline",
    mode: null,
    runtime: "inactive",
    port: null,
    learning: null,
  });
  await act(async () => emitRuntimeEvent(runtimeEvent({
    code: "topology_active",
    input: null,
    pressed: null,
  })));
  await waitFor(() => expect(screen.getByLabelText("在线设备")).toHaveValue(""));
  expect(screen.getByRole("combobox", { name: "2 A" })).toHaveValue("2");
  expect(screen.getByRole("combobox", { name: "2 B" })).toHaveValue("13");
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "end_learning")).toBe(false);
  await new Promise((resolve) => setTimeout(resolve, 550));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);
});

test("preserves a captured mapping when learning ends before autosave", async () => {
  const activeLearningTarget = {
    deviceId: "device-front-desk",
    deviceProfileId: deviceProfile.profile.id,
    hardwareProfileId: "front-desk",
    editingRevision: 3,
    firmwareRevision: 5,
    pins: [2, 13],
  };
  currentSnapshot.devices[0].learning = activeLearningTarget;
  const user = userEvent.setup();
  render(<App />);
  await openDeviceIo(user);
  await user.click(screen.getByText("适配新设备"));
  await user.selectOptions(screen.getByLabelText("在线设备"), "device-front-desk");

  await act(async () => emitRuntimeEvent(runtimeEvent({
    code: "learning_input",
    input: { type: "contact", source: 1, pin_a: 2, pin_b: 13 },
    learningTarget: activeLearningTarget,
  })));
  expect(screen.getByRole("combobox", { name: "2 A" })).toHaveValue("2");
  expect(screen.getByRole("combobox", { name: "2 B" })).toHaveValue("13");

  await user.click(screen.getByRole("button", { name: "结束学习" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("end_learning", {
    deviceId: "device-front-desk",
  }));
  expect(screen.getByRole("combobox", { name: "2 A" })).toHaveValue("2");
  expect(screen.getByRole("combobox", { name: "2 B" })).toHaveValue("13");
});

test("keeps a captured draft through Editor Profile switches until an ordinary edit", async () => {
  const secondProfile: DeviceProfile = {
    ...structuredClone(deviceProfile),
    profile: {
      ...deviceProfile.profile,
      id: "operator-console",
      name: "接线员控制台",
    },
  };
  const activeLearningTarget = {
    deviceId: "device-front-desk",
    deviceProfileId: deviceProfile.profile.id,
    hardwareProfileId: "front-desk",
    editingRevision: 17,
    firmwareRevision: 5,
    pins: [2, 13],
  };
  currentSnapshot.deviceProfiles.push(secondProfile);
  currentSnapshot.devices[0].learning = activeLearningTarget;
  const user = userEvent.setup();
  render(<App />);
  await openDeviceIo(user);
  await user.click(screen.getByText("适配新设备"));
  await user.selectOptions(screen.getByLabelText("在线设备"), "device-front-desk");

  await act(async () => emitRuntimeEvent(runtimeEvent({
    code: "learning_input",
    input: { type: "contact", source: 1, pin_a: 2, pin_b: 13 },
    learningTarget: activeLearningTarget,
  })));
  await user.click(screen.getByRole("button", { name: "结束学习" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("end_learning", {
    deviceId: "device-front-desk",
  }));

  await user.click(screen.getByRole("button", { name: "设置" }));
  await user.click(screen.getByRole("button", { name: "高级设置" }));
  await user.click(screen.getByRole("button", { name: `选择 ${secondProfile.profile.name}` }));
  await waitFor(() => expect(screen.getByText("当前编辑配置")).toBeInTheDocument());
  await user.click(screen.getByRole("button", { name: `选择 ${deviceProfile.profile.name}` }));
  await openDeviceIo(user);

  expect(screen.getByRole("combobox", { name: "2 A" })).toHaveValue("2");
  expect(screen.getByRole("combobox", { name: "2 B" })).toHaveValue("13");
  await new Promise((resolve) => setTimeout(resolve, 550));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);

  fireEvent.change(screen.getByLabelText("消抖"), { target: { value: "31" } });
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
    profile: expect.objectContaining({
      hardware_profiles: [expect.objectContaining({
        debounce_ms: 31,
        inputs: expect.arrayContaining([
          expect.objectContaining({ keys: { DIGIT_2: [2, 13] } }),
        ]),
      })],
    }),
  }), { timeout: 1600 });
});

test("keeps an older captured draft suppressed when a second begin fails", async () => {
  const activeLearningTarget = {
    deviceId: "device-front-desk",
    deviceProfileId: deviceProfile.profile.id,
    hardwareProfileId: "front-desk",
    editingRevision: 23,
    firmwareRevision: 5,
    pins: [2, 13],
  };
  currentSnapshot.devices[0].learning = activeLearningTarget;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "end_learning") {
      currentSnapshot.devices[0].learning = null;
      return structuredClone(currentSnapshot);
    }
    if (command === "begin_learning") throw new Error("learning unavailable");
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await openDeviceIo(user);
  await user.click(screen.getByText("适配新设备"));
  await user.selectOptions(screen.getByLabelText("在线设备"), "device-front-desk");

  await act(async () => emitRuntimeEvent(runtimeEvent({
    code: "learning_input",
    input: { type: "contact", source: 1, pin_a: 2, pin_b: 13 },
    learningTarget: activeLearningTarget,
  })));
  await user.click(screen.getByRole("button", { name: "结束学习" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("end_learning", {
    deviceId: "device-front-desk",
  }));

  await user.click(screen.getByRole("checkbox", { name: "键盘已与原电话电路及外部电压完全隔离" }));
  await user.click(screen.getByRole("checkbox", { name: "GPIO 2" }));
  await user.click(screen.getByRole("checkbox", { name: "GPIO 13" }));
  await user.click(screen.getByRole("button", { name: "开始学习" }));
  expect(await screen.findByText("逐键学习失败: learning unavailable")).toHaveClass("error-banner");
  await new Promise((resolve) => setTimeout(resolve, 550));

  expect(screen.getByRole("combobox", { name: "2 A" })).toHaveValue("2");
  expect(screen.getByRole("combobox", { name: "2 B" })).toHaveValue("13");
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);
});

test("targets learning and captured input to the explicitly selected non-first Hardware Profile and Device", async () => {
  currentSnapshot.deviceProfiles[0].hardware_profiles.push({
    id: "alternate-hardware",
    name: "备用硬件配置",
    board_profile_id: "luatos-esp32s3-aio",
    debounce_ms: 30,
    inputs: [{
      type: "contact_matrix",
      id: "alternate-matrix",
      pins: [1, 2, 12, 13],
      keys: { DIGIT_2: [1, 13] },
    }],
  });
  currentSnapshot.devices.push(device({
    deviceId: "device-alternate",
    name: "备用键盘",
    hardwareSerial: "ALTERNATE",
    port: "/dev/cu.alternate",
    capabilities: [1, 2, 12, 13],
    runtimeAssignment: {
      device_profile_id: deviceProfile.profile.id,
      hardware_profile_id: "alternate-hardware",
    },
  }));
  const user = userEvent.setup();
  render(<App />);
  await openDeviceIo(user);
  await user.selectOptions(
    within(screen.getByRole("tabpanel", { name: "I/O 映射" })).getByRole("combobox", { name: "硬件配置" }),
    "alternate-hardware",
  );
  await user.click(screen.getByRole("button", { name: "2" }));
  await user.click(screen.getByText("适配新设备"));
  await user.selectOptions(screen.getByLabelText("在线设备"), "device-alternate");
  await user.click(screen.getByRole("checkbox", { name: "键盘已与原电话电路及外部电压完全隔离" }));
  await user.click(screen.getByRole("checkbox", { name: "GPIO 2" }));
  await user.click(screen.getByRole("checkbox", { name: "GPIO 12" }));
  await user.click(screen.getByRole("button", { name: "开始学习" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("begin_learning", {
    deviceId: "device-alternate",
    deviceProfileId: deviceProfile.profile.id,
    hardwareProfileId: "alternate-hardware",
    editingRevision: 1,
    pins: [2, 12],
  }));
  const digitA = screen.getByRole("combobox", { name: "2 A" });
  const digitB = screen.getByRole("combobox", { name: "2 B" });

  await act(async () => emitRuntimeEvent(runtimeEvent({
    code: "learning_input",
    input: { type: "contact", source: 0, pin_a: 2, pin_b: 12 },
    learningTarget: {
      deviceId: "device-front-desk",
      deviceProfileId: deviceProfile.profile.id,
      hardwareProfileId: "alternate-hardware",
      editingRevision: 0,
      firmwareRevision: 0,
      pins: [2, 12],
    },
  })));
  expect(digitA).toHaveValue("1");
  expect(digitB).toHaveValue("13");

  await act(async () => emitRuntimeEvent(runtimeEvent({
    deviceId: "device-alternate",
    rawSerial: "ALTERNATE",
    code: "learning_input",
    input: { type: "contact", source: 0, pin_a: 2, pin_b: 12 },
    hardwareProfileId: "alternate-hardware",
    learningTarget: {
      deviceId: "device-alternate",
      deviceProfileId: deviceProfile.profile.id,
      hardwareProfileId: "alternate-hardware",
      editingRevision: 1,
      firmwareRevision: 0,
      pins: [2, 12],
    },
  })));
  expect(digitA).toHaveValue("2");
  expect(digitB).toHaveValue("12");
});

test("blocks autosave for a matrix pair endpoint missing from source pins until repaired", async () => {
  currentSnapshot.deviceProfiles[0].hardware_profiles[0].inputs[1] = {
    type: "contact_matrix",
    id: "carbon",
    pins: [1, 2],
    keys: { DIGIT_2: [1, 13] },
  };
  const user = userEvent.setup();
  render(<App />);
  await openDeviceIo(user);
  vi.mocked(invoke).mockClear();

  fireEvent.change(screen.getByLabelText("消抖"), { target: { value: "31" } });
  await new Promise((resolve) => setTimeout(resolve, 550));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);
  const endpoint = screen.getByRole("combobox", { name: "2 B" });
  expect(endpoint).toHaveValue("13");
  expect(endpoint).toHaveAttribute("aria-invalid", "true");

  await user.selectOptions(endpoint, "2");
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
    profile: expect.objectContaining({
      hardware_profiles: [expect.objectContaining({
        debounce_ms: 31,
        inputs: expect.arrayContaining([
          expect.objectContaining({ keys: { DIGIT_2: [1, 2] } }),
        ]),
      })],
    }),
  }), { timeout: 1600 });
});
