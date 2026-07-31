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
  DeviceProfile,
  DeviceStatus,
  RuntimeEvent,
} from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

const deviceProfile: DeviceProfile = {
  schema_version: 2,
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
    if (command === "delete_device_profile") {
      currentSnapshot = {
        ...currentSnapshot,
        deviceProfiles: [],
        editorProfile: null,
      };
    }
    return structuredClone(currentSnapshot);
  });
});

afterEach(() => {
  vi.useRealTimers();
});

test("does not override the WebView viewport height", async () => {
  render(<App />);

  await screen.findByRole("heading", { name: "按键概览" });
  expect(document.documentElement.style.getPropertyValue("--app-height")).toBe(
    "",
  );
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
      rawSerial: null,
      port: null,
      controllerFamilyId: "rp2040",
      boardProfileId: "vccgnd-yd-rp2040",
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
  await user.click(screen.getByRole("button", { name: "按键行为" }));
  await user.click(screen.getByRole("button", { name: "按下按键" }));

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
      rawSerial: null,
      port: "/dev/cu.candidate",
      controllerFamilyId: "rp2040",
      boardProfileId: "vccgnd-yd-rp2040",
      latestError: null,
    },
  ];

  await act(async () => vi.advanceTimersByTimeAsync(2_000));

  expect(screen.getByLabelText("设备状态汇总")).toHaveTextContent(
    "1 就绪 · 1 需处理 · 0 离线",
  );
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
  expect(screen.getByText("等待设备")).toBeInTheDocument();

  await act(async () => vi.advanceTimersByTimeAsync(2_000));

  expect(screen.getByText("设备已连接")).toBeInTheDocument();
  expect(screen.getByText("/dev/cu.test")).toBeInTheDocument();
  expect(screen.queryByRole("alert")).toBeNull();
});

test("preserves a dirty Device Profile draft across a registry refresh", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "按键行为" }));
  await user.click(screen.getByRole("button", { name: "按下按键" }));
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

test("keeps device management and configuration-file actions as separate work destinations", async () => {
  render(<App />);

  expect(
    await screen.findByRole("heading", { name: "按键概览" }),
  ).toBeInTheDocument();
  const navigation = screen.getByRole("navigation", { name: "配置" });
  expect(navigation).not.toContainElement(
    screen.getByRole("button", { name: "首页" }),
  );
  expect(screen.getByRole("button", { name: "首页" })).toHaveClass("is-active");
  const devicesButton = screen.getByRole("button", { name: "设备管理" });
  expect(devicesButton.querySelector(".lucide-usb")).not.toBeNull();
  await userEvent.setup().click(devicesButton);
  expect(screen.getByRole("heading", { name: "设备管理" })).toBeInTheDocument();
  expect(screen.queryByLabelText("当前编辑配置")).toBeNull();
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "配置文件" }));
  expect(screen.getByLabelText("当前编辑配置")).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "删除设备配置" }),
  ).toBeInTheDocument();
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
  await user.click(await screen.findByRole("button", { name: "设备管理" }));
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
  await user.click(await screen.findByRole("button", { name: "设备管理" }));
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
  await user.click(await screen.findByRole("button", { name: "设备管理" }));
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
  await user.click(await screen.findByRole("button", { name: "设备管理" }));
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
  await user.click(await screen.findByRole("button", { name: "设备管理" }));
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
  await user.click(await screen.findByRole("button", { name: "设备管理" }));
  await user.selectOptions(screen.getByRole("combobox", { name: "设备配置" }), "call-center");
  await user.click(screen.getByRole("button", { name: "保存运行分配" }));
  await user.click(within(screen.getByRole("dialog", { name: "保存运行分配" })).getByRole("button", { name: "确认" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_runtime_assignment", {
    deviceId: "device-front-desk",
    assignment: {
      device_profile_id: "call-center",
      hardware_profile_id: "call-center-hardware",
    },
  }));
  expect(screen.getAllByText("呼叫中心键盘 / 呼叫中心硬件")).toHaveLength(2);
  expect(screen.getByRole("combobox", { name: "设备配置" })).toHaveValue("call-center");
  expect(screen.getByRole("button", { name: /后台键盘.*碳膜电话键盘 \/ 前台硬件配置/ })).toBeInTheDocument();
});

test("keeps the existing assignment visible after runtime assignment rejection", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_device_metrics") return currentSnapshot.homeMetrics;
    if (command === "save_runtime_assignment") throw new Error("assignment denied");
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设备管理" }));
  await user.click(screen.getByRole("button", { name: "保存运行分配" }));
  await user.click(within(screen.getByRole("dialog", { name: "保存运行分配" })).getByRole("button", { name: "确认" }));
  expect(await screen.findByText("保存失败: assignment denied")).toHaveClass("error-banner");
  expect(screen.getAllByText("碳膜电话键盘 / 前台硬件配置")).toHaveLength(2);
});

test("clears one runtime assignment through the authoritative snapshot", async () => {
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_device_metrics") return currentSnapshot.homeMetrics;
    if (command === "clear_runtime_assignment") {
      expect(args).toEqual({ deviceId: "device-front-desk" });
      currentSnapshot.devices[0] = device({ runtimeAssignment: null, assignment: "unassigned" });
    }
    return structuredClone(currentSnapshot);
  });
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "设备管理" }));
  await user.click(screen.getByRole("button", { name: "清除运行分配" }));
  await user.click(within(screen.getByRole("dialog", { name: "清除运行分配" })).getByRole("button", { name: "确认" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("clear_runtime_assignment", {
    deviceId: "device-front-desk",
  }));
  expect(screen.getByRole("button", { name: "清除运行分配" })).toBeDisabled();
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
  await user.click(await screen.findByRole("button", { name: "设备管理" }));
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
  await user.click(await screen.findByRole("button", { name: "配置文件" }));

  await user.selectOptions(
    screen.getByLabelText("当前编辑配置"),
    secondProfile.profile.id,
  );

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

test("keeps the interface in Simplified Chinese without a language selector", async () => {
  currentSnapshot.language = "en-US";
  render(<App />);

  expect(await screen.findByText("配置文件")).toBeInTheDocument();
  expect(screen.queryByLabelText("语言")).toBeNull();
});

test("renders seven-day metrics in model order with Chinese logs", async () => {
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
    ],
  };
  render(<App />);

  await screen.findByRole("heading", { name: "按键概览" });
  expect(
    [...document.querySelectorAll(".heat-cell")].map(
      (item) => item.textContent,
    ),
  ).toEqual([
    expect.stringContaining("2"),
    expect.stringContaining("5"),
    expect.stringContaining("确认"),
  ]);
  expect(screen.getByText("按下 DIGIT_5")).toBeInTheDocument();
  expect(screen.queryByLabelText("当前编辑配置")).toBeNull();
  expect(screen.queryByLabelText("语言")).toBeNull();
});

test("shows Home as searching when only another profile's Device is online", async () => {
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

  expect(await screen.findByText("等待设备")).toBeInTheDocument();
  expect(screen.queryByText("设备已连接")).toBeNull();
  expect(screen.queryByText("/dev/cu.unrelated")).toBeNull();
});

test("shows Home as connected when an online matching Device follows an offline one", async () => {
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

  expect(await screen.findByText("设备已连接")).toBeInTheDocument();
  expect(screen.getByText("/dev/cu.online")).toBeInTheDocument();
  expect(screen.queryByText("等待设备")).toBeNull();
});

test("isolates a shared pressed button by emitting Device", async () => {
  currentSnapshot.devices.push(
    device({ deviceId: "device-second", hardwareSerial: "SECOND" }),
  );
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "按键行为" }));
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
  await user.click(await screen.findByRole("button", { name: "按键行为" }));
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
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "硬件配置" }));
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
        learningTarget: {
          deviceId: "device-front-desk",
          deviceProfileId: deviceProfile.profile.id,
          hardwareProfileId: "front-desk",
          editingRevision: 0,
          firmwareRevision: 0,
          pins: [2, 13],
        },
      }),
    ),
  );
  expect(digitA).toHaveValue("2");
  expect(digitB).toHaveValue("13");
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

  await waitFor(
    () =>
      expect(invoke).toHaveBeenCalledWith("save_device_profile", {
        profile: expect.objectContaining({
          actions: {
            DIGIT_2: [
              { type: "paste", text: "你好" },
              { type: "hotkey", keys: ["enter"] },
            ],
          },
        }),
      }),
    { timeout: 1600 },
  );
  expect(
    screen.getByRole("button", { name: "2，2 项行为" }),
  ).toBeInTheDocument();
});

test("records a shortcut from the application window", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "按键行为" }));
  const editor = await screen.findByRole("complementary", { name: "2" });

  await user.click(screen.getByRole("button", { name: "按下按键" }));
  await user.click(within(editor).getByRole("button", { name: "录入按键" }));
  fireEvent.keyDown(window, {
    code: "KeyK",
    key: "k",
    metaKey: true,
    shiftKey: true,
  });

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
  await user.selectOptions(
    within(editor).getByRole("combobox", { name: "按键" }),
    "k",
  );

  expect(
    within(editor).getByText("Command + Control + Shift + K"),
  ).toBeInTheDocument();
  await waitFor(
    () =>
      expect(invoke).toHaveBeenCalledWith("save_device_profile", {
        profile: expect.objectContaining({
          actions: {
            DIGIT_2: [{ type: "hotkey", keys: ["cmd", "ctrl", "shift", "k"] }],
          },
        }),
      }),
    { timeout: 1600 },
  );
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

  await waitFor(
    () =>
      expect(invoke).toHaveBeenCalledWith("save_device_profile", {
        profile: expect.objectContaining({
          actions: {
            DIGIT_2: [
              { type: "hotkey", keys: ["enter"] },
              { type: "paste", text: "先粘贴" },
            ],
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
  await user.click(await screen.findByRole("button", { name: "按键行为" }));
  const key = await screen.findByRole("button", { name: "2，0 项行为" });

  await user.click(key);
  await user.click(screen.getByRole("button", { name: "按下按键" }));
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
  await screen.findByText("配置文件");

  await user.click(screen.getByRole("button", { name: "配置文件" }));
  await user.click(screen.getByRole("button", { name: "导入设备配置" }));
  const dialog = await screen.findByRole("dialog", {
    name: "替换现有设备配置",
  });
  expect(
    within(dialog).getByText("22 个按键，22 项硬件配置，8 项行为"),
  ).toBeInTheDocument();
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
      };
    return structuredClone(currentSnapshot);
  });
  render(<App />);
  await screen.findByText("配置文件");

  await user.click(screen.getByRole("button", { name: "配置文件" }));
  await user.click(screen.getByRole("button", { name: "恢复备份" }));
  const dialog = await screen.findByRole("dialog", { name: "恢复全量备份" });
  expect(
    within(dialog).getByText(
      "3 个设备配置，44 个按键，40 项硬件配置，19 项行为",
    ),
  ).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("restore_backup", {
      path: "/tmp/backup.yaml",
    }),
  );
});

test("deletes the last device profile and keeps configuration-file actions available", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("配置文件");

  await user.click(screen.getByRole("button", { name: "配置文件" }));
  await user.click(screen.getByRole("button", { name: "删除设备配置" }));
  const dialog = await screen.findByRole("dialog", { name: "删除设备配置" });
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  expect(
    await screen.findByRole("option", { name: "还没有设备配置" }),
  ).toBeInTheDocument();
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
  await screen.findByText("配置文件");

  await user.click(screen.getByRole("button", { name: "硬件配置" }));

  expect(screen.getByText("直连 GPIO")).toBeInTheDocument();
  expect(screen.getByText("接触矩阵")).toBeInTheDocument();
  expect(screen.getByText("适配新设备").closest("details")).not.toHaveAttribute(
    "open",
  );
});

test("autosaves a newly added Hardware Profile with its compiled Board Profile", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "硬件配置" }));

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
  await user.click(await screen.findByRole("button", { name: "硬件配置" }));
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
  await user.click(await screen.findByRole("button", { name: "硬件配置" }));

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
