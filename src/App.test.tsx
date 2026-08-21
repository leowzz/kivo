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
      board_profile_id: "yd-esp32-s3",
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
    boardProfileId: "yd-esp32-s3",
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

function deviceRow(identifier: string): HTMLButtonElement {
  return screen.getByTitle(identifier).closest("button") as HTMLButtonElement;
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
  profile: { id: "rp-profile", name: "RP Profile", groups: [] },
  trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
  hardware_profiles: [
    {
      id: "rp-other",
      name: "RP Other",
      board_profile_id: "rp",
      debounce_ms: 30,
      inputs: [],
    },
    {
      id: "rp-hardware",
      name: "RP Hardware",
      board_profile_id: "rp",
      debounce_ms: 30,
      inputs: [],
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
      id: "yd-esp32-s3",
      controllerFamilyId: "esp32s3",
      displayName: "YD-ESP32-S3",
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
  await user.click(screen.getByRole("button", { name: "添加行为" }));
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
  const devicesButton = screen.getByRole("button", { name: "我的键盘" });
  if (!devicesButton.classList.contains("is-active")) await user.click(devicesButton);
  await user.click(await screen.findByRole("tab", { name: "高级 I/O" }));
}

async function openDeviceSettings(user: ReturnType<typeof userEvent.setup>) {
  const devicesButton = screen.getByRole("button", { name: "我的键盘" });
  if (!devicesButton.classList.contains("is-active")) await user.click(devicesButton);
  await user.click(await screen.findByRole("tab", { name: "设备设置" }));
}

function runtimeEvent(overrides: Partial<RuntimeEvent> = {}): RuntimeEvent {
  return {
    timestampMs: 1785396000000,
    level: "info",
    deviceId: "device-front-desk",
    rawSerial: "ABC123",
    controllerFamilyId: "esp32s3",
    boardProfileId: "yd-esp32-s3",
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

  await screen.findByRole("heading", { name: "我的键盘" });
  expect(document.documentElement.style.getPropertyValue("--app-height")).toBe(
    "",
  );
});

test("keeps the client surface focused on devices and editable actions", async () => {
  const user = userEvent.setup();
  render(<App client />);

  expect(await screen.findByRole("heading", { name: "我的键盘" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "数据与备份" })).toBeNull();
  expect(screen.queryByRole("tab", { name: "设备设置" })).toBeNull();
  expect(screen.queryByRole("tab", { name: "高级 I/O" })).toBeNull();
  expect(screen.queryByRole("button", { name: "配置设置" })).toBeNull();
  expect(screen.queryByRole("button", { name: "重命名设备" })).toBeNull();

  await screen.findByRole("complementary", { name: "2" });
  await addPasteAction(user, "client action");

  await waitFor(
    () => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
      profile: expect.objectContaining({
        actions: expect.objectContaining({
          DIGIT_2: expect.objectContaining({
            press: [{ type: "paste", text: "client action" }],
          }),
        }),
      }),
    }),
    { timeout: 1600 },
  );
});

test("exposes configuration backup and restore in the entry app", async () => {
  const user = userEvent.setup();
  vi.setSystemTime(new Date(2026, 7, 21, 12, 11, 0));
  vi.mocked(save).mockResolvedValue("/tmp/kivo-backup.yaml");
  vi.mocked(open).mockResolvedValue("/tmp/kivo-backup.yaml");
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "preview_backup") {
      return {
        kind: "product_devices",
        profileCount: 0,
        buttonCount: 0,
        hardwareBindingCount: 0,
        actionCount: 2,
        deviceCount: 1,
        assignmentCount: 0,
        metricRowCount: 0,
        activityCount: 0,
      };
    }
    return structuredClone(currentSnapshot);
  });

  render(<App client />);
  await screen.findByRole("heading", { name: "我的键盘" });

  await user.click(screen.getByRole("button", { name: "备份配置文件" }));
  expect(save).toHaveBeenCalledWith(expect.objectContaining({
    defaultPath: "kivo-backup-20260821-1211.yaml",
  }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("export_backup", {
    path: "/tmp/kivo-backup.yaml",
  }));

  await user.click(screen.getByRole("button", { name: "恢复配置文件" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("preview_backup", {
    path: "/tmp/kivo-backup.yaml",
  }));
  expect(await screen.findByRole("dialog", { name: "恢复设备行为备份" })).toBeInTheDocument();
});

test("fills default trigger settings when a stale profile omits them", async () => {
  const staleProfile = structuredClone(currentSnapshot.deviceProfiles[0]) as Omit<DeviceProfile, "trigger_settings"> & {
    trigger_settings?: DeviceProfile["trigger_settings"];
  };
  delete staleProfile.trigger_settings;
  currentSnapshot.deviceProfiles = [staleProfile as DeviceProfile];

  const user = userEvent.setup();
  render(<App />);

  expect(await screen.findByRole("heading", { name: "我的键盘" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "我的键盘" }));
  await addPasteAction(user, "默认计时");
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", expect.anything()));
  const saved = vi.mocked(invoke).mock.calls.find(([command]) => command === "save_device_profile")?.[1] as {
    profile: DeviceProfile;
  } | undefined;
  expect(saved?.profile.trigger_settings).toEqual({ long_press_ms: 500, double_press_ms: 300 });
});

test("switches the editable actions with the selected Device", async () => {
  const secondProfile: DeviceProfile = {
    ...structuredClone(deviceProfile),
    profile: {
      ...deviceProfile.profile,
      id: "profile-b",
      name: "备用配置",
    },
  };
  currentSnapshot.deviceProfiles.push(secondProfile);
  currentSnapshot.devices.push(device({
    deviceId: "device-back-desk",
    name: "备用键盘",
    hardwareSerial: "BACK456",
    runtimeAssignment: {
      device_profile_id: secondProfile.profile.id,
      hardware_profile_id: "front-desk",
    },
  }));
  const user = userEvent.setup();
  render(<App />);

  await screen.findByTitle("BACK456");
  await user.click(deviceRow("BACK456"));
  expect(screen.getByLabelText("备用配置")).toBeInTheDocument();
  await addPasteAction(user, "配置 B");
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
    profile: expect.objectContaining({
      profile: expect.objectContaining({ id: "profile-b" }),
      actions: expect.objectContaining({
        DIGIT_2: expect.objectContaining({ press: [{ type: "paste", text: "配置 B" }] }),
      }),
    }),
  }));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_runtime_assignment")).toBe(false);
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_settings")).toBe(false);
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

  await user.click(screen.getByRole("button", { name: "我的键盘" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", expect.anything()));
  expect(screen.getByRole("heading", { name: "我的键盘" })).toBeInTheDocument();

  pendingSave.resolve(structuredClone(currentSnapshot));
  expect(await screen.findByRole("heading", { name: "我的键盘" })).toBeInTheDocument();
});

test("shows the selected keyboard without exposing its system port in the button workspace", async () => {
  render(<App />);
  await screen.findByRole("heading", { name: "我的键盘" });
  expect(screen.getByRole("heading", { name: "前台键盘" })).toBeInTheDocument();
  expect(screen.queryByText("/dev/cu.test")).toBeNull();
});

test("summarizes an empty device registry", async () => {
  currentSnapshot.devices = [];
  render(<App />);

  expect(await screen.findByLabelText("设备状态汇总")).toHaveTextContent(
    "0 就绪 · 0 需处理 · 0 离线",
  );
});

test("summarizes mixed devices and adds candidates only to attention", async () => {
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
      boardProfileId: "yd-rp2040",
      latestError: null,
    },
  ];
  render(<App />);

  expect(await screen.findByLabelText("设备状态汇总")).toHaveTextContent(
    "1 就绪 · 2 需处理 · 1 离线",
  );
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
  expect(await screen.findByLabelText("设备状态汇总")).toHaveTextContent(
    "0 就绪 · 0 需处理 · 0 离线",
  );
  await user.click(screen.getByRole("button", { name: "我的键盘" }));
  await addHotkeyAction(user);

  await waitFor(
    () =>
      expect(screen.getByLabelText("设备状态汇总")).toHaveTextContent(
        "1 就绪 · 0 需处理 · 0 离线",
      ),
    { timeout: 1600 },
  );
});

test("refreshes authoritative registry state after a runtime event", async () => {
  render(<App />);
  expect(await screen.findByLabelText("设备状态汇总")).toHaveTextContent(
    "1 就绪 · 0 需处理 · 0 离线",
  );
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
    expect(screen.getByLabelText("设备状态汇总")).toHaveTextContent(
      "0 就绪 · 0 需处理 · 1 离线",
    ),
  );
});

test("periodically refreshes candidates that produce no runtime event", async () => {
  vi.useFakeTimers();
  render(<App />);
  await act(async () => undefined);
  expect(screen.getByLabelText("设备状态汇总")).toHaveTextContent(
    "1 就绪 · 0 需处理 · 0 离线",
  );
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
      boardProfileId: "yd-rp2040",
      latestError: null,
    },
  ];

  await act(async () => vi.advanceTimersByTimeAsync(2_000));

  expect(screen.getByLabelText("设备状态汇总")).toHaveTextContent(
    "1 就绪 · 1 需处理 · 0 离线",
  );
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
      "选择键盘配置",
    ),
  );
  expect(screen.getAllByRole("dialog", { name: "添加键盘" })).toHaveLength(1);
});

test("configuration page creates a profile while no device is usable", async () => {
  const user = userEvent.setup();
  currentSnapshot.devices = [];
  currentSnapshot.candidates = [
    rpCandidate({ issue: "firmware_not_responding" }),
  ];
  currentSnapshot.boardProfiles = [rpBoard];
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "稍后处理" }));
  await user.click(screen.getByRole("button", { name: "数据与备份" }));
  await user.click(screen.getByRole("button", { name: "新建配置" }));
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
  expect(await screen.findByRole("heading", { name: "数据与备份" })).toBeInTheDocument();
  expect(screen.getByText("Offline RP")).toBeInTheDocument();
  expect(screen.queryByLabelText("当前编辑配置")).not.toBeInTheDocument();
  expect(currentSnapshot.devices).toHaveLength(0);
});

test("completes one exact Device and navigates to its Hardware Profile", async () => {
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
  await user.selectOptions(
    within(dialog).getByRole("combobox", { name: "键盘配置" }),
    "rp-profile",
  );
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
  expect(await screen.findByRole("heading", { name: "我的键盘" })).toBeInTheDocument();
  expect(screen.queryByRole("combobox", { name: "硬件配置" })).toBeNull();
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
  await user.selectOptions(
    within(dialog).getByRole("combobox", { name: "键盘配置" }),
    "rp-profile",
  );
  await user.click(within(dialog).getByRole("button", { name: "下一步" }));
  await user.click(
    within(dialog).getByRole("button", { name: "完成设置" }),
  );

  expect(
    await screen.findByText("保存失败: settings_write_failed"),
  ).toHaveClass("error-banner");
  expect(screen.queryByRole("dialog", { name: "添加键盘" })).toBeNull();
  expect(screen.getByRole("heading", { name: "我的键盘" })).toBeInTheDocument();
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
  expect(screen.getByText("选择一台设备查看详情")).toBeInTheDocument();

  await act(async () => vi.advanceTimersByTimeAsync(2_000));

  expect(screen.getByRole("heading", { name: "前台键盘" })).toBeInTheDocument();
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

test("keeps the keyboard workspace and backup tools as separate topbar destinations", async () => {
  render(<App />);

  expect(
    await screen.findByRole("heading", { name: "我的键盘" }),
  ).toBeInTheDocument();
  const navigation = screen.getByRole("navigation", { name: "主要功能" });
  expect(navigation).toContainElement(screen.getByRole("button", { name: "我的键盘" }));
  expect(screen.getByRole("button", { name: "我的键盘" })).toHaveClass("is-active");
  const devicesButton = screen.getByRole("button", { name: "我的键盘" });
  expect(devicesButton.querySelector(".lucide-keyboard")).not.toBeNull();
  expect(screen.getByRole("heading", { name: "我的键盘" })).toBeInTheDocument();
  expect(screen.queryByLabelText("当前编辑配置")).toBeNull();
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "数据与备份" }));
  expect(screen.queryByLabelText("当前编辑配置")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: /删除.*碳膜电话键盘/ })).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: /^保存$/ }),
  ).not.toBeInTheDocument();
  expect(document.body).not.toHaveTextContent("型号");
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
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  await openDeviceSettings(user);
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("get_device_metrics", {
      deviceId: "device-front-desk",
    }),
  );
  expect(
    await screen.findByText("device-front-desk pressed"),
  ).toBeInTheDocument();
  await user.click(deviceRow("SECOND"));
  await openDeviceSettings(user);
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
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  await openDeviceSettings(user);
  expect(await screen.findByText("first activity")).toBeInTheDocument();
  await user.click(deviceRow("SECOND"));
  expect(screen.queryByText("first activity")).toBeNull();
  await openDeviceSettings(user);
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
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  await openDeviceSettings(user);
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
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  await openDeviceSettings(user);
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
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
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

test("saves one runtime assignment through the authoritative snapshot without changing a same-board Device", async () => {
  const secondProfile: DeviceProfile = {
    ...deviceProfile,
    profile: { ...deviceProfile.profile, id: "call-center", name: "呼叫中心键盘" },
    hardware_profiles: [{ ...deviceProfile.hardware_profiles[0], id: "call-center-hardware", name: "呼叫中心硬件" }],
  };
  currentSnapshot.deviceProfiles.push(secondProfile);
  currentSnapshot.devices.push(device({
    deviceId: "device-back-desk",
    name: "后台键盘",
    hardwareSerial: "BACK456",
    runtimeAssignment: { device_profile_id: deviceProfile.profile.id, hardware_profile_id: "front-desk" },
  }));
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_device_metrics") return currentSnapshot.homeMetrics;
    if (command === "save_runtime_assignment") {
      expect(args).toEqual({
        deviceId: "device-front-desk",
        assignment: {
          device_profile_id: "call-center",
          hardware_profile_id: "call-center-hardware",
        },
      });
      currentSnapshot.devices[0] = device({
        runtimeAssignment: {
          device_profile_id: "call-center",
          hardware_profile_id: "call-center-hardware",
        },
      });
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  await openDeviceSettings(user);
  await user.selectOptions(screen.getByRole("combobox", { name: "使用配置" }), "call-center");
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_runtime_assignment", {
    deviceId: "device-front-desk",
    assignment: {
      device_profile_id: "call-center",
      hardware_profile_id: "call-center-hardware",
    },
  }));
  expect(screen.getAllByText("呼叫中心键盘")).toHaveLength(2);
  expect(screen.getByRole("combobox", { name: "使用配置" })).toHaveValue("call-center");
  expect(deviceRow("BACK456")).toBeInTheDocument();
});

test("keeps the existing assignment visible after runtime assignment rejection", async () => {
  const rejectedProfile: DeviceProfile = {
    ...deviceProfile,
    profile: { ...deviceProfile.profile, id: "rejected", name: "被拒绝的配置" },
    hardware_profiles: [{ ...deviceProfile.hardware_profiles[0], id: "rejected-hardware" }],
  };
  currentSnapshot.deviceProfiles.push(rejectedProfile);
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_device_metrics") return currentSnapshot.homeMetrics;
    if (command === "save_runtime_assignment") throw new Error("assignment denied");
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  await openDeviceSettings(user);
  await user.selectOptions(screen.getByRole("combobox", { name: "使用配置" }), "rejected");
  expect(await screen.findByText("保存失败: assignment denied")).toHaveClass("error-banner");
  expect(screen.getByRole("combobox", { name: "使用配置" })).toHaveValue(deviceProfile.profile.id);
  expect(screen.getAllByText("碳膜电话键盘")).toHaveLength(2);
});

test("does not expose a clear runtime assignment action", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  expect(screen.queryByRole("button", { name: "保存运行分配" })).toBeNull();
  expect(screen.queryByRole("dialog", { name: "保存运行分配" })).toBeNull();
  expect(screen.queryByRole("button", { name: "清除运行分配" })).toBeNull();
  expect(screen.queryByRole("dialog", { name: "清除运行分配" })).toBeNull();
});

test("does not expose or mutate a global Editor Profile from the keyboard workspace", async () => {
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
  await screen.findByRole("heading", { name: "我的键盘" });

  expect(screen.queryByLabelText("当前编辑配置")).toBeNull();
  await user.click(screen.getByRole("button", { name: "数据与备份" }));
  expect(screen.queryByLabelText("当前编辑配置")).toBeNull();
  expect(
    vi
      .mocked(invoke)
      .mock.calls.some(
        ([command]) =>
          command === "save_settings" ||
          command === "save_runtime_assignment" ||
          command === "clear_runtime_assignment",
      ),
  ).toBe(false);
  expect(currentSnapshot.devices[0].runtimeAssignment).toEqual(
    baseSnapshot.devices[0].runtimeAssignment,
  );
});

test("keeps the interface in Simplified Chinese without a language selector", async () => {
  currentSnapshot.language = "en-US";
  render(<App />);

  expect(await screen.findByText("数据与备份")).toBeInTheDocument();
  expect(screen.queryByLabelText("语言")).toBeNull();
});

test("renders buttons in model order and selected Device metrics in Chinese", async () => {
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
  currentSnapshot.homeMetrics = {
    ...baseSnapshot.homeMetrics!,
    heatmap: [{ buttonId: "DIGIT_5", day: "2026-07-30", presses: 3 }],
    logs: [
      {
        ...baseSnapshot.homeMetrics!.logs[0],
        message: "DIGIT_5 pressed",
        buttonId: "DIGIT_5",
      },
      {
        ...baseSnapshot.homeMetrics!.logs[0],
        timestampMs: baseSnapshot.homeMetrics!.logs[0].timestampMs + 1,
        kind: "feature_disabled",
        message: "Action blocked by feature switch",
        buttonId: "ENTER",
      },
    ],
  };
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "get_device_metrics"
      ? structuredClone(currentSnapshot.homeMetrics)
      : structuredClone(currentSnapshot)
  );
  const user = userEvent.setup();
  render(<App />);

  await screen.findByRole("heading", { name: "我的键盘" });
  expect(
    [...document.querySelectorAll(".key-button")].map((item) => item.textContent),
  ).toEqual(["2", "5", "确认"]);
  await openDeviceSettings(user);
  const metrics = screen.getByRole("region", { name: "设备指标" });
  expect(within(metrics).getByText("今日按下")).toBeInTheDocument();
  expect(within(metrics).getByText("累计按下")).toBeInTheDocument();
  expect(screen.getByRole("table", { name: "设备动态" })).toHaveTextContent("DIGIT_5 pressed");
  expect(screen.queryByLabelText("当前编辑配置")).toBeNull();
  expect(screen.queryByLabelText("语言")).toBeNull();
});

test("opens the assigned profile of an online Device without a global editor selection", async () => {
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

  expect(await screen.findByLabelText("接线员控制台")).toBeInTheDocument();
  expect(screen.queryByLabelText("当前编辑配置")).toBeNull();
  expect(screen.queryByText("/dev/cu.unrelated")).toBeNull();
});

test("hides an offline Device and opens the connected Device directly", async () => {
  currentSnapshot.devices = [
    device({
      deviceId: "device-offline",
      name: "离线键盘",
      connection: "offline",
      mode: null,
      runtime: "inactive",
      port: null,
    }),
    device({
      deviceId: "device-online",
      name: "在线键盘",
      hardwareSerial: "ONLINE",
      port: "/dev/cu.online",
    }),
  ];

  const user = userEvent.setup();
  render(<App />);

  await screen.findByTitle("ONLINE");
  expect(screen.queryByTitle("ABC123")).toBeNull();
  await user.click(deviceRow("ONLINE"));
  expect(screen.getByRole("heading", { name: "在线键盘" })).toBeInTheDocument();
  expect(screen.getByLabelText("碳膜电话键盘")).toBeInTheDocument();
  expect(screen.queryByText("/dev/cu.online")).toBeNull();
});

test("isolates a shared pressed button by emitting Device", async () => {
  currentSnapshot.devices.push(
    device({ deviceId: "device-second", name: "Second Device", hardwareSerial: "SECOND" }),
  );
  const user = userEvent.setup();
  render(<App />);
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
  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({ deviceId: "device-front-desk", pressed: false }),
    ),
  );
  expect(enter).not.toHaveClass("is-pressed");

  await user.click(deviceRow("SECOND"));
  const selectedDeviceEnter = screen.getByRole("button", { name: "确认，0 项行为" });
  expect(selectedDeviceEnter).toHaveClass("is-pressed");

  await act(async () =>
    emitRuntimeEvent(
      runtimeEvent({
        deviceId: "device-second",
        rawSerial: "SECOND",
        pressed: false,
      }),
    ),
  );
  expect(selectedDeviceEnter).not.toHaveClass("is-pressed");
});

test("clears pressed feedback only for the disconnected Device regardless of the managed row", async () => {
  currentSnapshot.devices.push(device({
    deviceId: "device-second",
    name: "Second Device",
    hardwareSerial: "SECOND",
  }));
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  await user.click(deviceRow("ABC123"));
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
  await waitFor(() =>
    expect(screen.queryByRole("button", { name: "确认，0 项行为" })).toBeNull(),
  );
  expect(screen.queryByTitle("ABC123")).toBeNull();
  expect(screen.queryByTitle("SECOND")).toBeNull();
});

test("keeps actions, pressed feedback, and metrics scoped to the selected Device", async () => {
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
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_device_metrics") {
      metricRequests += 1;
      return (args as { deviceId: string }).deviceId === "device-other-profile"
        ? structuredClone(otherMetrics)
        : structuredClone(baseSnapshot.homeMetrics!);
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "我的键盘" }));
  await user.click(deviceRow("OTHER"));
  await waitFor(() => expect(metricRequests).toBe(2));
  const otherEnter = screen.getByRole("button", { name: "确认，0 项行为" });

  await act(async () => emitRuntimeEvent(runtimeEvent({
    deviceId: "device-other-profile",
    rawSerial: "OTHER",
    deviceProfileId: otherProfile.profile.id,
    homeUpdate: otherMetrics,
  })));
  expect(otherEnter).toHaveClass("is-pressed");
  await waitFor(() => expect(metricRequests).toBe(3));

  await openDeviceSettings(user);
  expect(within(screen.getByRole("region", { name: "设备指标" })).getByText("99")).toBeInTheDocument();
  expect(screen.getByText("other-profile activity")).toBeInTheDocument();

  await user.click(deviceRow("ABC123"));
  await act(async () => emitRuntimeEvent(runtimeEvent()));
  expect(screen.getByRole("button", { name: "确认，0 项行为" })).toHaveClass("is-pressed");
  await openDeviceSettings(user);
  expect(within(screen.getByRole("region", { name: "设备指标" })).getByText("12")).toBeInTheDocument();
  expect(screen.queryByText("other-profile activity")).toBeNull();
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
    board_profile_id: "yd-esp32-s3",
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
  expect(screen.getByRole("status")).toHaveTextContent("2 个设备");
  await user.click(screen.getByRole("tab", { name: "按键" }));
  await addPasteAction(user, "共享配置的新行为");

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
  fireEvent.click(deviceRow("SECOND"));
  fireEvent.click(screen.getByRole("tab", { name: "高级 I/O" }));
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

test("keeps a shared Device Management draft gated after navigating away", async () => {
  currentSnapshot.devices.push(device({
    deviceId: "device-second",
    name: "后台键盘",
    hardwareSerial: "SECOND",
    port: "/dev/cu.second",
  }));
  const user = userEvent.setup();
  render(<App />);

  await openDeviceIo(user);
  await user.selectOptions(screen.getByRole("combobox", { name: "2 A" }), "2");
  expect(screen.getByRole("combobox", { name: "2 A" })).toHaveValue("2");

  await user.click(screen.getByRole("button", { name: "我的键盘" }));
  expect(await screen.findByRole("heading", { name: "我的键盘" })).toBeInTheDocument();
  await new Promise((resolve) => setTimeout(resolve, 550));

  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);

  await openDeviceIo(user);
  expect(screen.getByRole("combobox", { name: "2 A" })).toHaveValue("2");
  await user.click(screen.getByRole("button", { name: "保存共享配置" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
    profile: expect.objectContaining({
      hardware_profiles: [expect.objectContaining({
        inputs: expect.arrayContaining([
          expect.objectContaining({ keys: { DIGIT_2: [2, 12] } }),
        ]),
      })],
    }),
  }));
});

test("keeps a newer shared draft gated while an older explicit save completes", async () => {
  currentSnapshot.devices.push(device({
    deviceId: "device-second",
    name: "后台键盘",
    hardwareSerial: "SECOND",
    port: "/dev/cu.second",
  }));
  const firstSave = deferred<AppSnapshot>();
  let saveCalls = 0;
  let firstSavedProfile: DeviceProfile | null = null;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "save_device_profile") {
      const saved = (args as { profile: DeviceProfile }).profile;
      saveCalls += 1;
      if (saveCalls === 1) {
        firstSavedProfile = structuredClone(saved);
        return firstSave.promise;
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
  await user.selectOptions(screen.getByRole("combobox", { name: "2 A" }), "2");
  await user.click(screen.getByRole("button", { name: "保存共享配置" }));
  await waitFor(() => expect(saveCalls).toBe(1));

  await user.selectOptions(screen.getByRole("combobox", { name: "2 B" }), "13");
  currentSnapshot.deviceProfiles = [firstSavedProfile!];
  firstSave.resolve(structuredClone(currentSnapshot));
  await new Promise((resolve) => setTimeout(resolve, 550));

  expect(saveCalls).toBe(1);
  expect(screen.getByRole("combobox", { name: "2 B" })).toHaveValue("13");
  await user.click(screen.getByRole("button", { name: "保存共享配置" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_device_profile", {
    profile: expect.objectContaining({
      hardware_profiles: [expect.objectContaining({
        inputs: expect.arrayContaining([
          expect.objectContaining({ keys: { DIGIT_2: [2, 13] } }),
        ]),
      })],
    }),
  }));
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

test("previews a device profile before importing it", async () => {
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
  await screen.findByText("数据与备份");

  await user.click(screen.getByRole("button", { name: "数据与备份" }));
  await user.click(screen.getByRole("button", { name: "导入设备配置" }));
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

test("previews a full backup before restoring it", async () => {
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
  await screen.findByText("数据与备份");

  await user.click(screen.getByRole("button", { name: "数据与备份" }));
  await user.click(screen.getByRole("button", { name: "恢复备份" }));
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

test("previews a Product Device backup as a non-destructive merge", async () => {
  const user = userEvent.setup();
  vi.mocked(open).mockResolvedValue("/tmp/product-devices.yaml");
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "preview_backup")
      return {
        kind: "product_devices",
        profileCount: 0,
        buttonCount: 0,
        hardwareBindingCount: 0,
        actionCount: 7,
        deviceCount: 2,
        assignmentCount: 0,
        metricRowCount: 0,
        activityCount: 0,
      };
    return structuredClone(currentSnapshot);
  });
  render(<App />);
  await screen.findByText("数据与备份");

  await user.click(screen.getByRole("button", { name: "数据与备份" }));
  await user.click(screen.getByRole("button", { name: "恢复备份" }));
  const dialog = await screen.findByRole("dialog", { name: "恢复设备行为备份" });
  expect(within(dialog).getByText("2 台设备，7 项行为")).toBeInTheDocument();
  expect(dialog).toHaveTextContent("产品版本不一致的设备不会被修改");
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("restore_backup", {
      path: "/tmp/product-devices.yaml",
    }),
  );
});

test("deletes the last device profile and keeps configuration-file actions available", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("数据与备份");

  await user.click(screen.getByRole("button", { name: "数据与备份" }));
  await user.click(screen.getByRole("button", { name: /删除.*碳膜电话键盘/ }));
  const dialog = await screen.findByRole("dialog", { name: "删除设备配置" });
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  expect(await screen.findByText("还没有设备配置")).toBeInTheDocument();
  expect(
    screen.getAllByRole("button", { name: "导入设备配置" }).length,
  ).toBeGreaterThan(0);
  expect(
    screen.getAllByRole("button", { name: "恢复备份" }).length,
  ).toBeGreaterThan(0);
});

test("keeps key learning secondary and collapsed by default", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("数据与备份");

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
            id: "yd-esp32-s3-hardware",
            name: "YD-ESP32-S3 硬件配置",
            board_profile_id: "yd-esp32-s3",
          }),
        ]),
      }),
    }),
    { timeout: 1600 },
  );
});

test("blocks autosave while a Board Profile change leaves invalid mappings and permits revert", async () => {
  currentSnapshot.boardProfiles.push({
    id: "yd-rp2040",
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

  await user.selectOptions(screen.getByLabelText("板型"), "yd-rp2040");
  expect(await screen.findByText("无效 GPIO 6")).toBeInTheDocument();
  await new Promise((resolve) => setTimeout(resolve, 550));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);

  await user.selectOptions(screen.getByLabelText("板型"), "yd-esp32-s3");
  expect(screen.queryByText("无效 GPIO 6")).toBeNull();
  await new Promise((resolve) => setTimeout(resolve, 550));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "save_device_profile")).toBe(false);
});

test("deletes a Hardware Profile without repairing its Device assignment", async () => {
  currentSnapshot.deviceProfiles[0].hardware_profiles.push({
    id: "spare",
    name: "备用硬件配置",
    board_profile_id: "yd-esp32-s3",
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

  await user.click(screen.getByRole("button", { name: "我的键盘" }));
  expect(deviceRow("ABC123")).toHaveTextContent("可用");
  expect(deviceRow("SECOND")).toHaveTextContent("正在学习");
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
  await user.click(screen.getByRole("button", { name: "保存共享配置" }));
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
  await user.click(screen.getByRole("button", { name: "我的键盘" }));
  expect(deviceRow("ABC123")).toHaveTextContent("正在配置");
  expect(deviceRow("SECOND")).toHaveTextContent("正在配置");
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

test("keeps a captured draft through Device switches until an ordinary edit", async () => {
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
  currentSnapshot.devices.push(device({
    deviceId: "device-operator-console",
    name: "接线员键盘",
    hardwareSerial: "OPERATOR",
    runtimeAssignment: {
      device_profile_id: secondProfile.profile.id,
      hardware_profile_id: "front-desk",
    },
  }));
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

  await user.click(deviceRow("OPERATOR"));
  await user.click(deviceRow("ABC123"));
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
    board_profile_id: "yd-esp32-s3",
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
    within(screen.getByRole("tabpanel", { name: "高级 I/O" })).getByRole("combobox", { name: "硬件配置" }),
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
