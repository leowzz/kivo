import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { DeviceManagement } from "./DeviceManagement";
import type { BoardProfileSummary, CandidateStatus, DeviceStatus, HomeMetricsSnapshot } from "./types";


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
    metrics: { deviceId: "rp-a", snapshot: metrics },
    onRename: vi.fn(),
    onForget: vi.fn(),
    onMetricsChange: vi.fn(),
    onOpenSetup: vi.fn(),
    onRetryCandidate: vi.fn(),
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

test("keeps professional configuration controls out of ordinary device details", () => {
  renderManagement({ selectedDeviceId: "rp-a" });

  expect(screen.queryByLabelText("使用设备配置")).not.toBeInTheDocument();
  expect(screen.queryByRole("tab", { name: "按键布局" })).not.toBeInTheDocument();
  expect(screen.queryByRole("tab", { name: "I/O 映射" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "配置设置" })).not.toBeInTheDocument();
  expect(screen.queryByText("Counter Profile")).not.toBeInTheDocument();
  expect(screen.queryByText("运行分配")).not.toBeInTheDocument();
  expect(screen.getByText("查看技术详情").closest("details")).not.toHaveAttribute("open");
});

test("sorts Device rows by availability and preserves source order within each status", () => {
  const { container } = renderManagement({ devices: [
    device({ deviceId: "offline-a", name: "Offline A", connection: "offline", mode: null, runtime: "inactive" }),
    device({ deviceId: "attention-a", name: "Attention A", assignment: "invalid_assignment", runtime: "inactive" }),
    device({ deviceId: "inactive-a", name: "Inactive A", runtime: "inactive" }),
    device({ deviceId: "ready-a", name: "Ready A" }),
    device({ deviceId: "progress-a", name: "Progress A", runtime: "configuring" }),
    device({ deviceId: "offline-b", name: "Offline B", connection: "offline", mode: null, runtime: "inactive" }),
    device({ deviceId: "ready-b", name: "Ready B" }),
    device({ deviceId: "attention-b", name: "Attention B", runtime: "runtime_error" }),
  ] });

  expect(
    Array.from(container.querySelectorAll(".device-table .device-row strong"), (name) =>
      name.textContent,
    ),
  ).toEqual([
    "Ready A",
    "Ready B",
    "Progress A",
    "Attention A",
    "Attention B",
    "Inactive A",
    "Offline A",
    "Offline B",
  ]);
});

test("never renders metrics owned by another Device", () => {
  renderManagement({ metrics: { deviceId: "rp-b", snapshot: metrics } });
  expect(screen.queryByText("A pressed")).toBeNull();
  expect(screen.queryByText("2 / 8")).toBeNull();
});

test("marks the selected Device ID for constrained wrapping", () => {
  renderManagement();

  expect(screen.getByText("rp-a", { selector: "dd" })).toHaveClass(
    "device-id-value",
  );
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
  expect(screen.getByRole("button", { name: /AD-001/ })).toBeInTheDocument();
});

test("shows compact selected-Device metrics with profile-free activity attribution", () => {
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

  const summary = screen.getByLabelText("设备指标");
  expect(summary).toHaveTextContent("今日按下2");
  expect(summary).toHaveTextContent("累计按下8");
  expect(summary).toHaveTextContent("活跃按键3");
  expect(summary).toHaveTextContent("最常用A");
  const activity = screen.getByText("A pressed before rename").closest("tr");
  expect(activity).toHaveTextContent("Event-time Device Name");
  expect(activity).not.toHaveTextContent("Current Device Name");
  expect(activity).not.toHaveTextContent("profile-b");
  expect(activity).not.toHaveTextContent("hardware-b");
  expect(activity).not.toHaveTextContent("profile-a");
  expect(activity).not.toHaveTextContent("hardware-a");
});

test("preserves selected device identity across live replacements", () => {
  const { rerender, props } = renderManagement();
  screen.getByRole("button", { name: /ESP32 A/ }).click();
  rerender(<DeviceManagement {...props} devices={props.devices.map((item) =>
    item.deviceId === "esp-a" ? { ...item, runtime: "runtime_error" } : item
  )} />);
  expect(screen.getByRole("button", { name: /ESP32 A/ })).toHaveAttribute("aria-pressed", "true");
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

  expect(screen.getByRole("button", { name: /RP2040 B/ })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(onSelectedDeviceChange).not.toHaveBeenCalledWith("rp-a");
});

test("publishes explicit device and candidate row selections", async () => {
  const user = userEvent.setup();
  const onSelectedDeviceChange = vi.fn();
  const onSelectedCandidateChange = vi.fn();
  renderManagement({ onSelectedDeviceChange, onSelectedCandidateChange });

  await user.click(screen.getByRole("button", { name: /RP2040 B/ }));
  expect(onSelectedDeviceChange).toHaveBeenLastCalledWith("rp-b");

  await user.click(screen.getByRole("button", { name: /AD-001/ }));
  expect(onSelectedDeviceChange).toHaveBeenLastCalledWith(null);
  expect(onSelectedCandidateChange).toHaveBeenLastCalledWith("candidate:esp32-pad:bad-serial:/dev/cu.bad");
});

test("does not publish an automatic physical fallback for a missing controlled candidate", () => {
  const onSelectedDeviceChange = vi.fn();
  const onSelectedCandidateChange = vi.fn();
  renderManagement({
    selectedCandidateKey: "candidate:missing",
    onSelectedDeviceChange,
    onSelectedCandidateChange,
  });

  expect(screen.getByRole("button", { name: /RP2040 A/ })).toHaveAttribute("aria-pressed", "true");
  expect(onSelectedDeviceChange).not.toHaveBeenCalled();
  expect(onSelectedCandidateChange).toHaveBeenCalledWith(null);
});

test("moves candidate selection to the nearest remaining row when its observation disappears", () => {
  const { rerender, props } = renderManagement();
  screen.getByRole("button", { name: /AD-001/ }).click();
  rerender(<DeviceManagement {...props} candidates={[]} />);
  expect(screen.getByRole("button", { name: /ESP32 Offline/ })).toHaveAttribute("aria-pressed", "true");
});

test("shows candidate diagnostics without mutable device actions", async () => {
  const user = userEvent.setup();
  renderManagement();
  await user.click(screen.getByRole("button", { name: /AD-001/ }));
  expect(screen.getByText("identity rejected")).toBeInTheDocument();
  expect(screen.queryByRole("textbox", { name: "设备名称" })).toBeNull();
  expect(screen.queryByRole("button", { name: "忘记设备" })).toBeNull();
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
    screen.getByRole("heading", { name: "Kivo 固件未响应" }),
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

  await user.click(screen.getByRole("button", { name: /RP2040 A/ }));
  expect(screen.queryByRole("alert")).toBeNull();
});

test("opens centralized setup from the page header and an unassigned Device", async () => {
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

  await user.click(screen.getByRole("button", { name: "添加键盘" }));
  expect(onOpenSetup).toHaveBeenCalledWith(null);
  await user.click(screen.getByRole("button", { name: "继续设置" }));
  expect(onOpenSetup).toHaveBeenCalledWith("rp-a");
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
  expect(screen.getByText(/多个设备声明了相同身份/)).toBeInTheDocument();
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


test("shows invalid assignment as a status without exposing stored profile IDs", () => {
  renderManagement({
    devices: [device({
      assignment: "invalid_assignment",
      runtimeAssignment: {
        device_profile_id: "profile-a",
        hardware_profile_id: "missing-hardware",
      },
    })],
  });

  expect(screen.getAllByText("分配需要修复")).toHaveLength(2);
  expect(screen.queryByText("profile-a")).not.toBeInTheDocument();
  expect(screen.queryByText("missing-hardware")).not.toBeInTheDocument();
});

test("keeps assignment diagnostics profile-free when stored hardware targets another board", () => {
  renderManagement({
    devices: [device({
      assignment: "invalid_assignment",
      runtimeAssignment: {
        device_profile_id: "profile-b",
        hardware_profile_id: "hardware-esp",
      },
    })],
  });

  expect(screen.getAllByText("分配需要修复")).toHaveLength(2);
  expect(screen.queryByText("profile-b")).not.toBeInTheDocument();
  expect(screen.queryByText("hardware-esp")).not.toBeInTheDocument();
});
