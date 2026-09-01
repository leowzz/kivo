import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// Vitest runs this source assertion in Node, while the production tsconfig excludes Node globals.
// @ts-expect-error Test-only Node module.
import { readFileSync } from "node:fs";
import { expect, test, vi } from "vitest";
import { DeviceManagement } from "./DeviceManagement";
import type { BoardProfileSummary, CandidateStatus, DeviceProfile, DeviceStatus, HomeMetricsSnapshot, LearningTarget } from "./types";

const viewCss = readFileSync("src/styles/views.css", "utf8");

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

function candidate(overrides: Partial<CandidateStatus> = {}): CandidateStatus {
  return {
    key: "candidate:esp32-pad:bad-serial:/dev/cu.bad",
    deviceId: null,
    mode: "bootloader",
    identity: "invalid_identity",
    issue: "invalid_identity",
    rawSerial: "BAD-001",
    port: "/dev/cu.bad",
    controllerFamilyId: "esp32s3",
    boardProfileId: "esp32-pad",
    latestError: "identity rejected",
    ...overrides,
  };
}

const metrics: HomeMetricsSnapshot = {
  totalPresses: 8,
  todayPresses: 2,
  activeButtonCount: 1,
  topButton: { buttonId: "A", presses: 8 },
  heatmap: [],
  logs: [{ timestampMs: 1, kind: "button", message: "A pressed", deviceId: "rp-a", deviceName: "RP2040 A", deviceProfileId: "profile-a", hardwareProfileId: "hardware-a", buttonId: "A" }],
};

const profiles: DeviceProfile[] = [
  { schema_version: 3, profile: { id: "profile-a", name: "Counter Profile", groups: [] }, trigger_settings: { long_press_ms: 500, double_press_ms: 300 }, hardware_profiles: [{ id: "hardware-a", name: "Counter Hardware", board_profile_id: "rp2040-pad", debounce_ms: 20, inputs: [] }], actions: {} },
  { schema_version: 3, profile: { id: "profile-b", name: "Timer Profile", groups: [] }, trigger_settings: { long_press_ms: 500, double_press_ms: 300 }, hardware_profiles: [{ id: "hardware-b", name: "Timer Hardware", board_profile_id: "rp2040-pad", debounce_ms: 20, inputs: [] }, { id: "hardware-b-alt", name: "Timer Hardware Alt", board_profile_id: "rp2040-pad", debounce_ms: 20, inputs: [] }, { id: "hardware-esp", name: "ESP Hardware", board_profile_id: "esp32-pad", debounce_ms: 20, inputs: [] }], actions: {} },
  { schema_version: 3, profile: { id: "profile-esp", name: "ESP Profile", groups: [] }, trigger_settings: { long_press_ms: 500, double_press_ms: 300 }, hardware_profiles: [{ id: "hardware-esp-only", name: "ESP Only Hardware", board_profile_id: "esp32-pad", debounce_ms: 20, inputs: [] }], actions: {} },
];

function renderManagement(overrides: Partial<React.ComponentProps<typeof DeviceManagement>> = {}) {
  const props: React.ComponentProps<typeof DeviceManagement> = {
    language: "zh-CN",
    devices: [
      device(),
      device({ deviceId: "rp-b", name: "RP2040 B", hardwareSerial: "RP-B-002", port: "/dev/cu.rp-b" }),
      device({ deviceId: "esp-a", name: "ESP32 A", hardwareSerial: "ESP-A-003", port: "/dev/cu.esp-a", controllerFamilyId: "esp32s3", boardProfileId: "esp32-pad" }),
      device({ deviceId: "esp-offline", name: "ESP32 Offline", connection: "offline", mode: null, runtime: "inactive", hardwareSerial: "ESP-OFF-004", port: null, controllerFamilyId: "esp32s3", boardProfileId: "esp32-pad", runtimeAssignment: null }),
    ],
    candidates: [candidate()],
    boardProfiles: boards,
    deviceProfiles: profiles,
    metrics: { deviceId: "rp-a", snapshot: metrics },
    onRename: vi.fn(),
    onSaveRuntimeAssignment: vi.fn(),
    onMetricsChange: vi.fn(),
    onOpenSetup: vi.fn(),
    onRetryCandidate: vi.fn(),
    selectedButtonId: null,
    onSelectedButtonChange: vi.fn(),
    pressedButtonIds: new Set(),
    ...overrides,
  };
  return { ...render(<DeviceManagement {...props} />), props };
}

function rowByIdentifier(identifier: string) {
  return screen.getByTitle(identifier).closest("button") as HTMLButtonElement;
}

async function openAdvanced(user: ReturnType<typeof userEvent.setup>) {
  const makerMode = screen.getByRole("button", { name: "创客模式" });
  if (makerMode.getAttribute("aria-pressed") !== "true") {
    await user.click(makerMode);
  }
  const summary = screen.getByText("高级设置", { selector: "summary span" }).closest("summary");
  if (!summary) throw new Error("Advanced settings disclosure is missing");
  await user.click(summary);
}

test("shows enrolled devices, including offline ones, without search, filters, or table columns", () => {
  renderManagement({ devices: [
    device(),
    device({ deviceId: "rp-b", name: "RP2040 B", hardwareSerial: "RP-B-002", port: "/dev/cu.rp-b" }),
    device({ deviceId: "esp-a", name: "ESP32 A", hardwareSerial: "ESP-A-003", port: "/dev/cu.esp-a", controllerFamilyId: "esp32s3", boardProfileId: "esp32-pad", assignment: "unassigned" }),
    device({ deviceId: "esp-offline", name: "ESP32 Offline", connection: "offline", mode: null, runtime: "inactive", hardwareSerial: "ESP-OFF-004", port: null, controllerFamilyId: "esp32s3", boardProfileId: "esp32-pad", runtimeAssignment: null }),
  ] });

  expect(screen.queryByRole("searchbox")).toBeNull();
  expect(screen.queryByRole("group", { name: "设备筛选" })).toBeNull();
  expect(screen.queryByText("设备名称")).toBeNull();
  expect(rowByIdentifier("RP-A-001")).toBeInTheDocument();
  expect(rowByIdentifier("RP-B-002")).toBeInTheDocument();
  expect(rowByIdentifier("ESP-A-003")).toBeInTheDocument();
  expect(rowByIdentifier("ESP-OFF-004")).toBeInTheDocument();
  expect(within(screen.getByRole("region", { name: "设备列表" })).getAllByText("已连接")).toHaveLength(4);
  expect(within(rowByIdentifier("ESP-OFF-004")).getAllByText("未连接")).toHaveLength(2);
});

test("offers offline-device removal and never exposes it for online devices", async () => {
  const user = userEvent.setup();
  const onForgetDevice = vi.fn();
  renderManagement({ onForgetDevice });

  expect(screen.getByRole("button", { name: "忘记设备 ESP32 Offline" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "忘记设备 RP2040 A" })).toBeNull();

  await user.click(screen.getByRole("button", { name: "忘记设备 ESP32 Offline" }));
  expect(onForgetDevice).toHaveBeenCalledWith("esp-offline");
});

test("shows connected devices before offline devices while preserving source order within each group", () => {
  const { container } = renderManagement({ devices: [
    device({ deviceId: "offline-a", hardwareSerial: "OFFLINE-A", connection: "offline", mode: null, runtime: "inactive" }),
    device({ deviceId: "attention-a", hardwareSerial: "ATTENTION-A", assignment: "invalid_assignment", runtime: "inactive" }),
    device({ deviceId: "ready-a", hardwareSerial: "READY-A" }),
    device({ deviceId: "progress-a", hardwareSerial: "PROGRESS-A", runtime: "configuring" }),
    device({ deviceId: "offline-b", hardwareSerial: "OFFLINE-B", connection: "offline", mode: null, runtime: "inactive" }),
  ] });

  expect(
    Array.from(container.querySelectorAll(".connected-device-list .device-identifier"), (identifier) =>
      identifier.textContent,
    ),
  ).toEqual([
    "ATTENTION-A",
    "READY-A",
    "PROGRESS-A",
    "OFFLINE-A",
    "OFFLINE-B",
    "AD-001",
  ]);
});

test("never renders metrics owned by another Device", () => {
  renderManagement({ metrics: { deviceId: "rp-b", snapshot: metrics } });
  expect(screen.queryByText("A pressed")).toBeNull();
  expect(screen.queryByText("2 / 8")).toBeNull();
});

test("marks the selected Device ID for constrained wrapping", async () => {
  const user = userEvent.setup();
  renderManagement();
  await openAdvanced(user);

  expect(screen.getByText("rp-a", { selector: "dd" })).toHaveClass(
    "device-id-value",
  );
});

test("uses Board Profile as the primary line and identifier plus status as the second line", () => {
  renderManagement();
  const row = rowByIdentifier("RP-A-001");
  expect(within(row).getByText("RP2040 Pad")).toBeInTheDocument();
  expect(within(row).getByText("已连接")).toBeInTheDocument();
  expect(within(row).getByText("可用")).toBeInTheDocument();
});

test("Escape from the keypad returns focus to the selected device row", async () => {
  const user = userEvent.setup();
  const keypadProfile: DeviceProfile = {
    ...profiles[0],
    profile: {
      ...profiles[0].profile,
      groups: [{ id: "main", columns: 1, buttons: [{ id: "A", label: "A" }] }],
    },
    actions: {
      A: { press: [{ type: "paste", text: "hello" }], release: [], long_press: [], double_press: [] },
    },
  };
  const { props, rerender } = renderManagement({
    deviceProfiles: [keypadProfile],
    devices: [device({ runtimeAssignment: { device_profile_id: "profile-a", hardware_profile_id: "hardware-a" } })],
  });

  const key = screen.getByRole("button", { name: /A，1 项行为/ });
  await user.click(key);
  await user.keyboard("{Escape}");

  expect(rowByIdentifier("RP-A-001")).toHaveFocus();
});

test("copies Product Device actions only from the same Product Version", async () => {
  const user = userEvent.setup();
  const onCopyProductConfig = vi.fn().mockResolvedValue(undefined);
  const productDefinition = {
    schema_version: 1 as const,
    product: {
      display_name: "Kivo Key 1",
      family_id: "key",
      variant_id: "key-rp-k1",
      hardware_revision: 1,
      product_version_id: "key-rp-k1-r01",
      capabilities: [],
    },
    layout: {
      id: "key-rp-k1",
      name: "Kivo Key 1",
      groups: [{ id: "keys", columns: 1, buttons: [{ id: "K1", label: "K1" }] }],
    },
    hardware_profile: {
      id: "hardware",
      name: "Hardware",
      board_profile_id: "rp2040-pad",
      debounce_ms: 30,
      inputs: [],
    },
  };
  const productConfig = {
    product_version_id: "key-rp-k1-r01",
    trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
    actions: {},
  };
  const { props, rerender } = renderManagement({
    devices: [
      device({ productVersionId: "key-rp-k1-r01", productDefinition, productConfig }),
      device({
        deviceId: "rp-b",
        name: "RP2040 B",
        hardwareSerial: "RP-B-002",
        productVersionId: "key-rp-k1-r01",
        productDefinition,
        productConfig,
      }),
      device({
        deviceId: "rp-other",
        name: "Other product",
        hardwareSerial: "RP-C-003",
        productVersionId: "other-rp-k1-r01",
        productDefinition: {
          ...productDefinition,
          product: { ...productDefinition.product, product_version_id: "other-rp-k1-r01" },
        },
        productConfig: { ...productConfig, product_version_id: "other-rp-k1-r01" },
      }),
    ],
    onCopyProductConfig,
  });

  await openAdvanced(user);
  const source = screen.getByRole("combobox", { name: "来源设备" });
  expect(within(source).getByRole("option", { name: "RP2040 B" })).toBeInTheDocument();
  expect(within(source).queryByRole("option", { name: "Other product" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "复制行为与触发设置" }));

  expect(onCopyProductConfig).toHaveBeenCalledWith("rp-b", "rp-a");
});

test("keeps assignment details out of the compact rows while retaining selected activity", async () => {
  const user = userEvent.setup();
  const firstRender = renderManagement();
  expect(rowByIdentifier("RP-A-001")).not.toHaveTextContent("Counter Profile");
  expect(screen.queryByText("A pressed")).not.toBeInTheDocument();
  await user.click(screen.getByRole("tab", { name: "活动" }));
  expect(screen.getByText("A pressed")).toBeInTheDocument();
  await user.click(rowByIdentifier("ESP-A-003"));
  expect(screen.getByRole("heading", { name: "ESP32 A" })).toBeInTheDocument();
  firstRender.unmount();
  renderManagement({ devices: [device({ runtimeAssignment: { device_profile_id: "gone", hardware_profile_id: "missing" } })] });
  await openAdvanced(user);
  await user.click(screen.getByText("查看技术详情"));
  expect(screen.getByText("gone", { selector: "dd" })).toBeInTheDocument();
});

test("shows compact selected-Device metrics with event-time activity attribution", async () => {
  renderManagement({
    devices: [device({
      name: "Current Device Name",
      runtimeAssignment: {
        device_profile_id: "profile-b",
        hardware_profile_id: "hardware-b",
      },
    })],
    metrics: {
      deviceId: "rp-a",
      snapshot: {
        ...metrics,
        activeButtonCount: 3,
        topButton: { buttonId: "A", presses: 8 },
        logs: [{
          ...metrics.logs[0],
          deviceName: "Event-time Device Name",
          deviceProfileId: "profile-a",
          hardwareProfileId: "hardware-a",
          message: "A pressed before rename",
        }],
      },
    },
  });

  await userEvent.setup().click(screen.getByRole("tab", { name: "活动" }));
  const summary = screen.getByLabelText("设备指标");
  expect(summary).toHaveTextContent("今日按下2");
  expect(summary).toHaveTextContent("累计按下8");
  expect(summary).toHaveTextContent("活跃按键3");
  expect(summary).toHaveTextContent("最常用A");
  const activity = screen.getByText("A pressed before rename").closest("tr");
  expect(activity).toHaveTextContent("Event-time Device Name");
  expect(activity).toHaveTextContent("profile-a");
  expect(activity).toHaveTextContent("hardware-a");
  expect(activity).not.toHaveTextContent("Current Device Name");
  expect(activity).not.toHaveTextContent("profile-b");
  expect(activity).not.toHaveTextContent("hardware-b");
});

test("shows activity insights for the selected Device and links keys back to editing", async () => {
  const user = userEvent.setup();
  const onSelectedButtonChange = vi.fn();
  const activityProfile = structuredClone(profiles[0]);
  activityProfile.profile.groups = [{
    id: "main",
    columns: 2,
    buttons: [{ id: "A", label: "Alpha" }, { id: "B", label: "Beta" }],
  }];
  activityProfile.actions = {
    A: { press: [{ type: "paste", text: "hello" }], release: [], long_press: [], double_press: [] },
  };
  renderManagement({
    deviceProfiles: [activityProfile, profiles[1], profiles[2]],
    selectedButtonId: "A",
    onSelectedButtonChange,
    metrics: {
      deviceId: "rp-a",
      snapshot: {
        ...metrics,
        heatmap: [
          { buttonId: "A", day: "2026-08-20", presses: 2 },
          { buttonId: "A", day: "2026-08-21", presses: 3 },
        ],
      },
    },
    devices: [device({
      runtimeAssignment: { device_profile_id: "profile-a", hardware_profile_id: "hardware-a" },
      latestError: {
        code: "action_step_failed",
        params: { button: "A", step: "1" },
        detail: "paste denied",
        input: null,
        pressed: null,
        learningTarget: null,
      },
    })],
  });

  await user.click(screen.getByRole("tab", { name: "活动" }));
  expect(screen.getByRole("region", { name: "最近 7 天热力图" })).toHaveTextContent("2026-08-20");
  expect(screen.getByRole("region", { name: "最近 7 天热力图" })).toHaveTextContent("2026-08-21");
  expect(screen.getByRole("region", { name: "常用按键" })).toHaveTextContent("Alpha");
  expect(screen.getByRole("region", { name: "最近 7 天未使用" })).toHaveTextContent("Beta");
  expect(screen.getByRole("alert", { name: "最近动作失败" })).toHaveTextContent("Alpha · 粘贴: paste denied");

  await user.click(within(screen.getByRole("region", { name: "最近 7 天未使用" })).getByRole("button", { name: "Beta" }));
  expect(screen.getByRole("tab", { name: "按键" })).toHaveAttribute("aria-selected", "true");
  expect(onSelectedButtonChange).toHaveBeenLastCalledWith("B");
});

test("aggregates persisted action failures by key and action type", async () => {
  const user = userEvent.setup();
  const activityProfile = structuredClone(profiles[0]);
  activityProfile.profile.groups = [{
    id: "main",
    columns: 2,
    buttons: [{ id: "A", label: "Alpha" }, { id: "B", label: "Beta" }],
  }];
  renderManagement({
    deviceProfiles: [activityProfile, profiles[1], profiles[2]],
    metrics: {
      deviceId: "rp-a",
      snapshot: {
        ...metrics,
        logs: [
          {
            ...metrics.logs[0],
            kind: "action_failed",
            message: "paste failed",
            buttonId: "A",
            actionKind: "paste",
            detail: "clipboard unavailable",
            timestampMs: 3,
          },
          {
            ...metrics.logs[0],
            kind: "action_failed",
            message: "paste failed",
            buttonId: "A",
            actionKind: "paste",
            detail: "clipboard unavailable",
            timestampMs: 2,
          },
        ],
      },
    },
    devices: [device({
      runtimeAssignment: { device_profile_id: "profile-a", hardware_profile_id: "hardware-a" },
      latestError: null,
    })],
  });

  await user.click(screen.getByRole("tab", { name: "活动" }));
  const failures = screen.getByRole("alert", { name: "最近动作失败" });
  expect(failures).toHaveTextContent("Alpha · 粘贴 · 2 次: clipboard unavailable");
  await user.click(within(failures).getByRole("button", { name: /编辑按键:Alpha/ }));
});

test("preserves selected device identity across live replacements", () => {
  const { rerender, props } = renderManagement();
  rowByIdentifier("ESP-A-003").click();
  rerender(<DeviceManagement {...props} devices={props.devices.map((item) =>
    item.deviceId === "esp-a" ? { ...item, runtime: "runtime_error" } : item
  )} />);
  expect(rowByIdentifier("ESP-A-003")).toHaveAttribute("aria-pressed", "true");
});

test("applies a controlled non-first device before publishing selection", () => {
  const onSelectedDeviceChange = vi.fn();
  renderManagement({
    devices: [
      device(),
      device({ deviceId: "rp-b", name: "RP2040 B", hardwareSerial: "RP-B-002" }),
    ],
    selectedDeviceId: "rp-b",
    onSelectedDeviceChange,
  });

  expect(rowByIdentifier("RP-B-002")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(onSelectedDeviceChange).not.toHaveBeenCalledWith("rp-a");
});

test("publishes explicit device and candidate row selections", async () => {
  const user = userEvent.setup();
  const onSelectedDeviceChange = vi.fn();
  renderManagement({ onSelectedDeviceChange });

  await user.click(rowByIdentifier("RP-B-002"));
  expect(onSelectedDeviceChange).toHaveBeenLastCalledWith("rp-b");

  await user.click(rowByIdentifier("BAD-001"));
  expect(onSelectedDeviceChange).toHaveBeenLastCalledWith(null);
});

test("moves candidate selection to the nearest remaining row when its observation disappears", () => {
  const { rerender, props } = renderManagement();
  rowByIdentifier("BAD-001").click();
  rerender(<DeviceManagement {...props} candidates={[]} />);
  expect(rowByIdentifier("ESP-OFF-004")).toHaveAttribute("aria-pressed", "true");
});

test("shows candidate diagnostics without mutable device actions", async () => {
  const user = userEvent.setup();
  renderManagement();
  await user.click(rowByIdentifier("BAD-001"));
  expect(screen.getByText("identity rejected")).toBeInTheDocument();
  expect(screen.queryByRole("textbox", { name: "设备名称" })).toBeNull();
  expect(screen.queryByRole("button", { name: "忘记设备" })).toBeNull();
});

test("removes communication ports from rows and reveals them only in technical details", async () => {
  const user = userEvent.setup();
  renderManagement({
    candidates: [candidate({ issue: "firmware_not_responding" })],
  });

  expect(
    screen.queryByText("端口", { selector: ".device-table-head span" }),
  ).toBeNull();
  expect(screen.queryByText("/dev/cu.rp-a")).not.toBeInTheDocument();
  await openAdvanced(user);
  await user.click(screen.getByText("查看技术详情"));
  expect(screen.getByText("/dev/cu.rp-a")).toBeVisible();
  await user.click(screen.getByRole("button", { name: /AD-001/ }));
  expect(screen.getByText("/dev/cu.bad")).not.toBeVisible();
  await user.click(screen.getByText("查看技术详情"));
  expect(screen.getByText("/dev/cu.bad")).toBeInTheDocument();
  expect(screen.getByText("系统通信端口")).toBeInTheDocument();
});

test("shows friendly firmware recovery and retries only the selected Candidate", async () => {
  const user = userEvent.setup();
  const onRetryCandidate = vi.fn().mockResolvedValue(undefined);
  renderManagement({
    candidates: [
      candidate({
        deviceId: "candidate-rp",
        issue: "firmware_not_responding",
        latestError: "serial_handshake_timeout",
      }),
    ],
    onRetryCandidate,
  });
  await user.click(screen.getByRole("button", { name: /AD-001/ }));

  expect(
    screen.getByRole("heading", { name: "设备暂时没有回应" }),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "重新检测" }));
  expect(onRetryCandidate).toHaveBeenCalledWith("candidate-rp");
});

test("disables Candidate retry and ignores repeated clicks while pending", async () => {
  let resolveRetry!: () => void;
  const onRetryCandidate = vi.fn(
    () =>
      new Promise<void>((resolve) => {
        resolveRetry = resolve;
      }),
  );
  renderManagement({
    candidates: [
      candidate({
        deviceId: "candidate-rp",
        issue: "firmware_not_responding",
      }),
    ],
    onRetryCandidate,
  });
  fireEvent.click(screen.getByRole("button", { name: /AD-001/ }));
  const retry = screen.getByRole("button", { name: "重新检测" });

  fireEvent.click(retry);
  fireEvent.click(retry);

  expect(onRetryCandidate).toHaveBeenCalledTimes(1);
  expect(retry).toBeDisabled();
  resolveRetry();
  await waitFor(() => expect(retry).toBeEnabled());
});

test("shows Candidate retry failures and allows another attempt", async () => {
  const user = userEvent.setup();
  const onRetryCandidate = vi
    .fn()
    .mockRejectedValue({ code: "retry_unavailable" });
  renderManagement({
    candidates: [
      candidate({
        deviceId: "candidate-rp",
        issue: "firmware_not_responding",
      }),
    ],
    onRetryCandidate,
  });
  await user.click(screen.getByRole("button", { name: /AD-001/ }));
  const retry = screen.getByRole("button", { name: "重新检测" });
  await user.click(retry);

  expect(await screen.findByRole("alert")).toHaveTextContent(
    "retry_unavailable",
  );
  expect(retry).toBeEnabled();
});

test("shows only the Candidate serial suffix in the primary row", () => {
  renderManagement({
    candidates: [candidate({ rawSerial: "50031519384E811C" })],
  });

  const row = screen.getByRole("button", { name: /4E811C/ });
  expect(within(row).getByText("4E811C")).toBeInTheDocument();
  expect(within(row).queryByText("50031519384E811C")).toBeNull();
});

test("uses a friendly Candidate label when no serial is available", () => {
  renderManagement({
    candidates: [
      candidate({
        key: "runtime:/dev/cu.usbmodem1101",
        rawSerial: null,
        port: "/dev/cu.usbmodem1101",
      }),
    ],
  });

  const row = screen.getByRole("button", { name: /待处理设备 1/ });
  expect(within(row).queryByText(/\/dev\/cu\./)).toBeNull();
});

test("scopes an operation error to its originating row", async () => {
  const user = userEvent.setup();
  renderManagement({
    candidates: [
      candidate({
        deviceId: "candidate-rp",
        issue: "firmware_not_responding",
      }),
    ],
    onRetryCandidate: vi
      .fn()
      .mockRejectedValue({ code: "retry_unavailable" }),
  });
  await user.click(screen.getByRole("button", { name: /AD-001/ }));
  await user.click(screen.getByRole("button", { name: "重新检测" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "retry_unavailable",
  );

  await user.click(rowByIdentifier("RP-A-001"));
  expect(screen.queryByRole("alert")).toBeNull();
});

test("relies on automatic discovery and opens setup from an unassigned Device", async () => {
  const user = userEvent.setup();
  const onOpenSetup = vi.fn();
  renderManagement({
    devices: [
      device({
        assignment: "unassigned",
        runtimeAssignment: null,
        runtime: "inactive",
      }),
    ],
    candidates: [],
    onOpenSetup,
  });

  await user.click(screen.getByRole("button", { name: "继续设置" }));
  expect(onOpenSetup).toHaveBeenCalledWith("rp-a");
});

test("offers starter profiles in the action area for an empty layout", async () => {
  const user = userEvent.setup();
  const onDuplicateProfileForDevice = vi.fn().mockResolvedValue(undefined);
  const blank = structuredClone(profiles[0]);
  const starter = structuredClone(profiles[0]);
  starter.profile.id = "daily-shortcuts";
  starter.profile.name = "Daily Shortcuts";
  starter.profile.groups = [{ id: "main", columns: 1, buttons: [{ id: "A", label: "A" }] }];
  starter.actions = {
    A: { press: [{ type: "paste", text: "hello" }], release: [], long_press: [], double_press: [] },
  };
  renderManagement({
    devices: [device()],
    candidates: [],
    deviceProfiles: [blank, starter],
    onDuplicateProfileForDevice,
  });

  expect(screen.getByRole("heading", { name: "从用途模板开始" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Daily Shortcuts/ }));

  expect(onDuplicateProfileForDevice).toHaveBeenCalledWith({
    deviceId: "rp-a",
    sourceProfile: starter,
    name: "Daily Shortcuts (RP2040 A)",
  });
});

test("shows starter profiles directly on the no-device workspace", async () => {
  const user = userEvent.setup();
  const onCreateFromTemplate = vi.fn();
  const starter = structuredClone(profiles[0]);
  starter.profile.id = "daily-shortcuts";
  starter.profile.name = "Daily Shortcuts";
  renderManagement({
    devices: [],
    candidates: [],
    deviceProfiles: [starter],
    metrics: null,
    onCreateFromTemplate,
  });

  expect(screen.getByRole("heading", { name: "从用途模板开始" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Daily Shortcuts/ }));

  expect(onCreateFromTemplate).toHaveBeenCalledWith("daily-shortcuts");
});

test("opens setup from the device list header", async () => {
  const user = userEvent.setup();
  const onOpenSetup = vi.fn();
  renderManagement({ onOpenSetup });

  await user.click(screen.getByRole("button", { name: "添加键盘" }));

  expect(onOpenSetup).toHaveBeenCalledWith(null);
});

test("identity conflicts never expose retry or assignment actions", async () => {
  const user = userEvent.setup();
  renderManagement({
    candidates: [
      candidate({
        deviceId: "conflict",
        issue: "duplicate_identity",
        identity: "duplicate_identity",
      }),
    ],
  });
  await user.click(screen.getByRole("button", { name: /AD-001/ }));
  expect(screen.queryByRole("button", { name: "重新检测" })).toBeNull();
  expect(screen.queryByRole("button", { name: "保存运行分配" })).toBeNull();
  expect(screen.getByText(/多个设备无法区分/)).toBeInTheDocument();
});

test("renames exactly the selected device", async () => {
  const user = userEvent.setup();
  const onRename = vi.fn();
  renderManagement({ onRename });
  await user.click(rowByIdentifier("RP-B-002"));
  await user.click(screen.getByRole("button", { name: "重命名设备" }));
  await user.clear(screen.getByRole("textbox", { name: "设备名称" }));
  await user.type(screen.getByRole("textbox", { name: "设备名称" }), "Counter B");
  await user.click(screen.getByRole("button", { name: "确认重命名" }));
  expect(onRename).toHaveBeenCalledWith("rp-b", "Counter B");
});

test("saves the selected Device Profile immediately with its automatic hardware mapping", async () => {
  const user = userEvent.setup();
  const onSaveRuntimeAssignment = vi.fn();
  renderManagement({ onSaveRuntimeAssignment });

  await openAdvanced(user);
  await user.selectOptions(screen.getByRole("combobox", { name: "设备配置" }), "profile-b");
  expect(screen.getByRole("combobox", { name: "硬件配置" })).toHaveValue("hardware-b");
  expect(onSaveRuntimeAssignment).toHaveBeenCalledWith("rp-a", {
    device_profile_id: "profile-b",
    hardware_profile_id: "hardware-b",
  });
  expect(screen.queryByRole("button", { name: "保存运行分配" })).toBeNull();
  expect(screen.queryByRole("dialog", { name: "保存运行分配" })).toBeNull();
});

test("does not fan an assignment out to another Device with the same Board Profile", async () => {
  const user = userEvent.setup();
  const onSaveRuntimeAssignment = vi.fn();
  renderManagement({ onSaveRuntimeAssignment });

  await openAdvanced(user);
  await user.selectOptions(screen.getByRole("combobox", { name: "设备配置" }), "profile-b");
  expect(onSaveRuntimeAssignment).toHaveBeenCalledTimes(1);
  expect(document.querySelectorAll(".connected-device-list .device-row")).toHaveLength(5);
});

test("shows no compatible hardware state without attempting an automatic save", async () => {
  const user = userEvent.setup();
  const onSaveRuntimeAssignment = vi.fn();
  renderManagement({ onSaveRuntimeAssignment });

  await user.click(rowByIdentifier("ESP-A-003"));
  await openAdvanced(user);
  await user.selectOptions(screen.getByRole("combobox", { name: "设备配置" }), "profile-a");
  expect(screen.getByText("此配置不适用于当前设备")).toBeInTheDocument();
  expect(onSaveRuntimeAssignment).not.toHaveBeenCalled();
  expect(screen.queryByRole("button", { name: "保存运行分配" })).toBeNull();
});

test("does not save when the selected Device Profile is unchanged", async () => {
  const user = userEvent.setup();
  const onSaveRuntimeAssignment = vi.fn();
  renderManagement({ onSaveRuntimeAssignment });

  await openAdvanced(user);
  await user.selectOptions(screen.getByRole("combobox", { name: "设备配置" }), "profile-a");
  expect(screen.getByRole("combobox", { name: "硬件配置" })).toHaveValue("hardware-a");
  expect(onSaveRuntimeAssignment).not.toHaveBeenCalled();
  expect(screen.queryByRole("button", { name: "保存运行分配" })).toBeNull();
});

test("does not turn an assigned Device into an empty draft", async () => {
  const user = userEvent.setup();
  renderManagement();

  await openAdvanced(user);
  const select = screen.getByRole("combobox", { name: "设备配置" });
  await user.selectOptions(select, "");

  expect(select).toHaveValue("profile-a");
});

test("repairs invalid stored IDs without exposing a clear-assignment action", async () => {
  const user = userEvent.setup();
  const onSaveRuntimeAssignment = vi.fn();
  renderManagement({
    devices: [device({ assignment: "invalid_assignment", runtimeAssignment: { device_profile_id: "gone", hardware_profile_id: "missing" } })],
    onSaveRuntimeAssignment,
  });

  await openAdvanced(user);
  expect(screen.getByText("gone", { selector: "dd" })).toBeInTheDocument();
  expect(screen.getByRole("combobox", { name: "设备配置" })).toHaveValue("");
  await user.selectOptions(screen.getByRole("combobox", { name: "设备配置" }), "profile-a");
  expect(onSaveRuntimeAssignment).toHaveBeenCalledWith("rp-a", {
    device_profile_id: "profile-a",
    hardware_profile_id: "hardware-a",
  });
  expect(screen.queryByRole("button", { name: "清除运行分配" })).toBeNull();
});

test("retains both raw IDs when only the stored Hardware Profile is missing", async () => {
  const user = userEvent.setup();
  renderManagement({
    devices: [device({
      assignment: "invalid_assignment",
      runtimeAssignment: {
        device_profile_id: "profile-a",
        hardware_profile_id: "missing-hardware",
      },
    })],
  });

  await openAdvanced(user);
  expect(screen.getAllByText("profile-a")).toHaveLength(2);
});

test("retains both raw IDs when the stored Hardware Profile has the wrong Board Profile", async () => {
  const user = userEvent.setup();
  renderManagement({
    devices: [device({
      assignment: "invalid_assignment",
      runtimeAssignment: {
        device_profile_id: "profile-b",
        hardware_profile_id: "hardware-esp",
      },
    })],
  });

  await openAdvanced(user);
  expect(screen.getByText("profile-b", { selector: "dd" })).toBeInTheDocument();
});

test("locks assignment selection while an automatic save is pending", async () => {
  const user = userEvent.setup();
  let resolveSave!: () => void;
  const onSaveRuntimeAssignment = vi.fn(
    () => new Promise<void>((resolve) => { resolveSave = resolve; }),
  );
  renderManagement({ onSaveRuntimeAssignment });

  await openAdvanced(user);
  const select = screen.getByRole("combobox", { name: "设备配置" });
  await user.selectOptions(select, "profile-b");
  expect(onSaveRuntimeAssignment).toHaveBeenCalledTimes(1);
  expect(select).toBeDisabled();
  resolveSave();
  await waitFor(() => expect(select).toBeEnabled());
});

test("restores the current assignment when an automatic save fails", async () => {
  const user = userEvent.setup();
  const onSaveRuntimeAssignment = vi.fn().mockRejectedValue(new Error("assignment denied"));
  renderManagement({ onSaveRuntimeAssignment });

  await openAdvanced(user);
  const select = screen.getByRole("combobox", { name: "设备配置" });
  await user.selectOptions(select, "profile-b");

  expect(await screen.findByRole("alert")).toHaveTextContent("assignment denied");
  expect(select).toHaveValue("profile-a");
});

test("embeds I/O Mapping and Key Layout as device workspace tabs", async () => {
  const user = userEvent.setup();
  const onSelectedDeviceChange = vi.fn();
  renderManagement({
    selectedDeviceId: "rp-a",
    onSelectedDeviceChange,
  });

  expect(
    within(screen.getByRole("tablist", { name: "设备工作区" }))
      .getAllByRole("tab")
      .map((tab) => tab.textContent),
  ).toEqual(["按键", "测试", "活动"]);
  await openAdvanced(user);
  await user.click(screen.getByRole("tab", { name: "高级 I/O" }));
  expect(screen.getByRole("tabpanel", { name: "高级 I/O" })).toHaveTextContent("硬件配置");
  expect(screen.queryByRole("dialog", { name: "高级 I/O" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("tab", { name: "按键布局" }));
  expect(screen.getByRole("tabpanel", { name: "按键布局" })).toContainElement(
    screen.getByRole("button", { name: "添加按键组" }),
  );
  expect(screen.queryByRole("dialog", { name: "按键布局" })).not.toBeInTheDocument();
});

test("keeps device selection, keypad, and actions in one workspace", async () => {
  const onSelectedButtonChange = vi.fn();
  const profile = structuredClone(profiles[0]);
  profile.profile.groups = [{
    id: "main",
    columns: 2,
    buttons: [{ id: "A", label: "A" }, { id: "B", label: "B" }],
  }];
  profile.actions = {
    A: {
      press: [{ type: "paste", text: "hello" }],
      release: [],
      long_press: [],
      double_press: [],
    },
  };

  const { props, rerender } = renderManagement({
    deviceProfiles: [profile, profiles[1], profiles[2]],
    selectedButtonId: "A",
    onSelectedButtonChange,
    pressedButtonIds: new Set(["A"]),
    onChangeProfile: vi.fn(),
  });

  expect(screen.getByRole("tab", { name: "按键" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("button", { name: "A，1 项行为" })).toHaveClass("is-pressed");
  expect(screen.getByRole("complementary", { name: "A" })).toHaveTextContent("粘贴 - hello");

  await userEvent.setup().click(screen.getByRole("button", { name: "B，0 项行为" }));
  expect(onSelectedButtonChange).toHaveBeenCalledWith("B");
});

test("offers a live physical-key test without changing the action editor contract", async () => {
  const user = userEvent.setup();
  const profile = structuredClone(profiles[0]);
  profile.profile.groups = [{ id: "main", columns: 1, buttons: [{ id: "A", label: "A" }] }];
  renderManagement({
    deviceProfiles: [profile, profiles[1], profiles[2]],
    selectedButtonId: "A",
    pressedButtonIds: new Set(["A"]),
  });

  await user.click(screen.getByRole("tab", { name: "测试" }));
  const panel = screen.getByRole("tabpanel", { name: "测试" });
  expect(panel).toHaveTextContent("按键测试");
  expect(within(panel).getByRole("status")).toHaveTextContent("已识别：A");
  expect(within(panel).getByRole("button", { name: "A，0 项行为" })).toHaveClass("is-pressed");

  await user.click(within(panel).getByRole("button", { name: "重新识别按键" }));
  expect(screen.getByText("高级设置", { selector: "summary span" }).closest("details")).toHaveAttribute("open");
  expect(screen.getByRole("tab", { name: "高级 I/O" })).toHaveAttribute("aria-selected", "true");
});

test("associates an action failure with its key in test mode", async () => {
  const user = userEvent.setup();
  const profile = structuredClone(profiles[0]);
  profile.profile.groups = [{ id: "main", columns: 1, buttons: [{ id: "A", label: "A" }] }];
  renderManagement({
    deviceProfiles: [profile, profiles[1], profiles[2]],
    selectedButtonId: "A",
    executionFeedback: {
      buttonId: "A",
      status: "error",
      detail: "paste denied",
    },
  });

  await user.click(screen.getByRole("tab", { name: "测试" }));
  expect(screen.getByRole("alert")).toHaveTextContent("动作执行失败: paste denied");
  expect(
    screen.getByRole("button", { name: "A，0 项行为，动作执行失败" }),
  ).toHaveClass("is-failed");
});

test("locks the learning Device and keeps test mode unavailable until learning ends", async () => {
  const user = userEvent.setup();
  const target: LearningTarget = {
    deviceId: "rp-a",
    deviceProfileId: "profile-a",
    hardwareProfileId: "hardware-a",
    editingRevision: 3,
    firmwareRevision: 4,
    pins: [1, 2],
  };
  const onSelectedDeviceChange = vi.fn();
  const { props, rerender } = renderManagement({
    selectedDeviceId: "rp-a",
    onSelectedDeviceChange,
    devices: [
      device({ learning: target, runtime: "learning" }),
      device({ deviceId: "rp-b", name: "RP2040 B", hardwareSerial: "RP-B-002" }),
    ],
  });

  const testTab = screen.getByRole("tab", { name: "测试" });
  expect(testTab).toBeDisabled();
  expect(rowByIdentifier("RP-A-001")).toHaveAttribute("aria-pressed", "true");
  expect(rowByIdentifier("RP-B-002")).toBeDisabled();
  await user.click(rowByIdentifier("RP-B-002"));
  expect(onSelectedDeviceChange).not.toHaveBeenCalledWith("rp-b");
  const advanced = screen.getByText("高级设置", { selector: "summary span" }).closest("details");
  await waitFor(() => expect(advanced).toHaveAttribute("open", ""));
  await user.click(screen.getByRole("button", { name: "完成学习并进入测试" }));
  expect(testTab).not.toHaveAttribute("aria-selected", "true");

  rerender(<DeviceManagement {...props} devices={[
    device({ learning: null }),
    device({ deviceId: "rp-b", name: "RP2040 B", hardwareSerial: "RP-B-002" }),
  ]} />);
  await waitFor(() => expect(testTab).toHaveAttribute("aria-selected", "true"));
});

test("clears a prior action result when testing a different key", async () => {
  const user = userEvent.setup();
  const profile = structuredClone(profiles[0]);
  profile.profile.groups = [{
    id: "main",
    columns: 2,
    buttons: [{ id: "A", label: "A" }, { id: "B", label: "B" }],
  }];
  const { props, rerender } = renderManagement({
    deviceProfiles: [profile, profiles[1], profiles[2]],
    selectedButtonId: "A",
    executionFeedback: { buttonId: "A", status: "success", detail: null },
  });

  await user.click(screen.getByRole("tab", { name: "测试" }));
  const panel = screen.getByRole("tabpanel", { name: "测试" });
  expect(within(panel).getByRole("status")).toHaveTextContent("动作已发送");
  await user.click(screen.getByRole("button", { name: "B，0 项行为" }));
  rerender(<DeviceManagement {...props} selectedButtonId="B" />);
  expect(within(panel).getByRole("status")).toHaveTextContent("等待实体按键输入");
});

test("supports arrow-key navigation across task tabs", async () => {
  const user = userEvent.setup();
  renderManagement();

  const buttonsTab = screen.getByRole("tab", { name: "按键" });
  buttonsTab.focus();
  await user.keyboard("{ArrowRight}");
  expect(screen.getByRole("tab", { name: "测试" })).toHaveAttribute("aria-selected", "true");
  await user.keyboard("{End}");
  expect(screen.getByRole("tab", { name: "活动" })).toHaveAttribute("aria-selected", "true");
});

test("keeps task tabs focused while advanced technical details stay disclosed", async () => {
  const user = userEvent.setup();
  renderManagement();

  expect(screen.getByRole("tab", { name: "按键" })).toHaveAttribute("aria-selected", "true");
  expect(screen.queryByText("A pressed")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "成品模式" })).toHaveAttribute("aria-pressed", "true");
  expect(screen.queryByText("高级设置", { selector: "summary span" })).not.toBeInTheDocument();

  await user.click(screen.getByRole("tab", { name: "活动" }));
  expect(screen.getByLabelText("设备指标")).toBeInTheDocument();
  expect(screen.getByRole("table", { name: "设备动态" })).toHaveTextContent("A pressed");

  await openAdvanced(user);
  const technical = screen.getByText("查看技术详情", { selector: "summary" }).closest("details");
  expect(technical).not.toHaveAttribute("open");
  expect(screen.getByRole("tab", { name: "活动" })).toHaveAttribute("aria-selected", "true");
});

test("keeps overview diagnostics for a device without an editing profile", () => {
  renderManagement({
    devices: [device({ assignment: "unassigned", runtimeAssignment: null })],
  });

  expect(screen.queryByRole("tablist", { name: "设备工作区" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "成品模式" })).toHaveAttribute("aria-pressed", "true");
  expect(screen.queryByText("高级设置", { selector: "summary span" })).not.toBeInTheDocument();
  expect(screen.queryByText("查看技术详情")).not.toBeInTheDocument();
});

test("lets the embedded Key Layout grow inside the device detail scroller", () => {
  const embeddedRule = viewCss.match(/\.embedded-layout-editor\s*\{([^}]*)\}/)?.[1];
  expect(embeddedRule).toMatch(/max-height:\s*none/);
});

test("uses shared button primitives for device workspace commands", async () => {
  const user = userEvent.setup();
  renderManagement();

  expect(screen.queryByRole("button", { name: "保存运行分配" })).toBeNull();
  expect(screen.queryByRole("button", { name: "清除运行分配" })).toBeNull();
  expect(screen.queryByRole("button", { name: "配置设置" })).not.toBeInTheDocument();

  await openAdvanced(user);
  expect(screen.getByRole("button", { name: "配置设置" })).toHaveClass("secondary-button");
  await user.click(screen.getByRole("tab", { name: "高级 I/O" }));
  expect(screen.getByRole("button", { name: "保存共享配置" })).toHaveClass("secondary-button");
  expect(screen.getByRole("button", { name: "复制并仅用于此设备" })).toHaveClass("secondary-button");
});

test("shows a persistent shared configuration warning with save action", async () => {
  renderManagement({
    devices: [device(), device({ deviceId: "rp-b", name: "RP2040 B" })],
    selectedDeviceId: "rp-a",
  });
  expect(screen.getByText(/2 个设备/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "复制并仅用于此设备" })).toBeInTheDocument();
  const user = userEvent.setup();
  await openAdvanced(user);
  await user.click(screen.getByRole("tab", { name: "高级 I/O" }));
  expect(screen.getByRole("button", { name: "保存共享配置" })).toBeInTheDocument();
});
