import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, test, vi } from "vitest";
import {
  DeviceSetupWizard,
  type DeviceSetupWizardProps,
} from "./DeviceSetupWizard";
import type { CandidateStatus, DeviceProfile, DeviceStatus } from "./types";

const boards = [
  {
    id: "rp",
    controllerFamilyId: "rp2040",
    displayName: "RP2040 Pad",
    runtimeUsb: "2e8a:102e",
    bootloaderUsb: "2e8a:0003",
    safePins: [0, 1],
  },
  {
    id: "esp",
    controllerFamilyId: "esp32s3",
    displayName: "ESP32 Pad",
    runtimeUsb: "303a:4002",
    bootloaderUsb: null,
    safePins: [1, 2],
  },
];

const profiles: DeviceProfile[] = [
  {
    schema_version: 3,
    profile: { id: "rp-profile", name: "RP Profile", groups: [{ id: "main", columns: 1, buttons: [{ id: "rp-key", label: "RP Key" }] }] },
    hardware_profiles: [
      {
        id: "rp-hardware",
        name: "RP Hardware",
        board_profile_id: "rp",
        debounce_ms: 30,
        inputs: [{ type: "direct", id: "buttons", keys: { "rp-key": 0 } }],
      },
    ],
    trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
    actions: {},
  },
  {
    schema_version: 3,
    profile: { id: "esp-profile", name: "ESP Profile", groups: [] },
    hardware_profiles: [
      {
        id: "esp-hardware",
        name: "ESP Hardware",
        board_profile_id: "esp",
        debounce_ms: 30,
        inputs: [],
      },
    ],
    trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
    actions: {},
  },
];

function candidate(overrides: Partial<CandidateStatus> = {}): CandidateStatus {
  return {
    key: "runtime:/dev/cu.usbmodem1101",
    deviceId: "rp-device-id",
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

function unassignedDevice(overrides: Partial<DeviceStatus> = {}): DeviceStatus {
  return {
    deviceId: "rp-device-id",
    name: "RP2040 Pad · 4E811C",
    connection: "online",
    mode: "runtime",
    identity: "valid",
    assignment: "unassigned",
    runtime: "inactive",
    hardwareSerial: "50031519384E811C",
    port: "/dev/cu.usbmodem1101",
    controllerFamilyId: "rp2040",
    boardProfileId: "rp",
    firmwareBuildId: "hello-v3",
    capabilities: [0, 1],
    runtimeAssignment: null,
    latestError: null,
    learning: null,
    ...overrides,
  };
}

function snapshot(candidateStatuses: CandidateStatus[] = []) {
  return {
    deviceProfiles: profiles,
    editorProfile: "rp-profile",
    boardProfiles: boards,
    devices: [],
    candidates: candidateStatuses,
    language: "zh-CN" as const,
    homeMetrics: null,
  };
}

function renderWizard(overrides: Partial<DeviceSetupWizardProps> = {}) {
  const props: DeviceSetupWizardProps = {
    open: true,
    targetId: "rp-device-id",
    language: "zh-CN",
    devices: [],
    candidates: [],
    boardProfiles: boards,
    deviceProfiles: profiles,
    inputEvent: null,
    onTargetChange: vi.fn(),
    onRetryCandidate: vi.fn().mockResolvedValue(undefined),
    onCreateProfile: vi.fn().mockResolvedValue(snapshot()),
    onComplete: vi.fn().mockResolvedValue(undefined),
    onOpenAdvanced: vi.fn(),
    onClose: vi.fn(),
    ...overrides,
  };
  return { ...render(<DeviceSetupWizard {...props} />), props };
}

test("traps keyboard focus, restores it, and closes with Escape", async () => {
  const user = userEvent.setup();
  const onClose = vi.fn();
  const background = document.createElement("button");
  background.textContent = "背景操作";
  document.body.appendChild(background);
  background.focus();
  const { unmount } = renderWizard({
    targetId: null,
    candidates: [candidate()],
    onClose,
  });

  const close = screen.getByRole("button", { name: "关闭" });
  const last = screen.getByRole("button", { name: /4E811C/ });
  expect(close).toHaveFocus();
  await user.tab({ shift: true });
  expect(last).toHaveFocus();
  await user.tab();
  expect(close).toHaveFocus();
  fireEvent.keyDown(window, { key: "Escape" });
  expect(onClose).toHaveBeenCalledOnce();
  unmount();
  expect(background).toHaveFocus();
  background.remove();
});

test("explains firmware failure, hides cu port until expanded, and retries the exact ID", async () => {
  const user = userEvent.setup();
  const onRetryCandidate = vi.fn().mockResolvedValue(undefined);
  renderWizard({
    targetId: "rp-device-id",
    candidates: [
      candidate({
        issue: "firmware_not_responding",
        latestError: "serial_handshake_timeout",
      }),
    ],
    onRetryCandidate,
  });

  expect(
    screen.getByRole("heading", { name: "Kivo 固件未响应" }),
  ).toBeInTheDocument();
  expect(screen.getByText("/dev/cu.usbmodem1101")).not.toBeVisible();
  await user.click(screen.getByText("查看技术详情"));
  expect(screen.getByText("/dev/cu.usbmodem1101")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "重新检测" }));
  expect(onRetryCandidate).toHaveBeenCalledWith("rp-device-id");
});

test("keeps the wizard open and advances when a Candidate becomes the same Device ID", () => {
  const { rerender, props } = renderWizard({
    targetId: "rp-device-id",
    candidates: [candidate()],
  });
  expect(screen.getByText("正在确认设备")).toBeInTheDocument();

  rerender(
    <DeviceSetupWizard
      {...props}
      candidates={[]}
      devices={[unassignedDevice()]}
    />,
  );

  expect(screen.getByText("第 1 步，共 3 步")).toBeInTheDocument();
  expect(screen.getByText("新键盘")).toBeInTheDocument();
});

test("firmware failure can enter independent profile creation", async () => {
  const user = userEvent.setup();
  const onCreateProfile = vi
    .fn()
    .mockResolvedValue(
      snapshot([candidate({ issue: "firmware_incompatible" })]),
    );
  renderWizard({
    targetId: "rp-device-id",
    candidates: [
      candidate({
        issue: "firmware_incompatible",
        latestError: "protocol_mismatch",
      }),
    ],
    onCreateProfile,
  });

  await user.click(screen.getByRole("button", { name: "先新建配置" }));
  await user.click(screen.getByRole("radio", { name: "空白配置" }));
  await user.type(screen.getByRole("textbox", { name: "配置名称" }), "RP 离线配置");
  await user.click(screen.getByRole("button", { name: "创建配置" }));
  expect(onCreateProfile).toHaveBeenCalledWith({
    kind: "blank",
    name: "RP 离线配置",
    board_profile_id: "rp",
  });
});

test("selects among multiple setup targets explicitly", async () => {
  const user = userEvent.setup();
  const candidates = [
    candidate(),
    candidate({
      key: "runtime:/dev/cu.second",
      deviceId: "second",
      rawSerial: "50031519384E811D",
    }),
  ];

  function TargetSelectionHarness() {
    const [targetId, setTargetId] = useState<string | null>(null);
    return (
      <DeviceSetupWizard
        open
        targetId={targetId}
        language="zh-CN"
        devices={[]}
        candidates={candidates}
        boardProfiles={boards}
        deviceProfiles={profiles}
        inputEvent={null}
        onTargetChange={setTargetId}
        onRetryCandidate={vi.fn().mockResolvedValue(undefined)}
        onCreateProfile={vi.fn().mockResolvedValue(snapshot(candidates))}
        onComplete={vi.fn().mockResolvedValue(undefined)}
        onOpenAdvanced={vi.fn()}
        onClose={vi.fn()}
      />
    );
  }

  render(<TargetSelectionHarness />);
  expect(screen.getByRole("heading", { name: "选择键盘" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /50031519384E811D/ })).toBeNull();
  await user.click(screen.getByRole("button", { name: /4E811D/ }));
  expect(screen.getByText("正在确认设备")).toBeInTheDocument();
});

test("uses a friendly target label when a Candidate has no serial", () => {
  renderWizard({
    targetId: null,
    candidates: [candidate({ rawSerial: null })],
  });

  const target = screen.getByRole("button", { name: /待处理设备 1/ });
  expect(within(target).queryByText(/\/dev\/cu\./)).toBeNull();
});

test("recognizes an unassigned keyboard and recommends only compatible presets", async () => {
  const user = userEvent.setup();
  renderWizard({
    targetId: "rp-device-id",
    devices: [unassignedDevice()],
  });

  expect(screen.getByText("第 1 步，共 3 步")).toBeInTheDocument();
  expect(screen.getByText("新键盘")).toBeInTheDocument();
  expect(screen.queryByText("RP2040 Pad")).toBeNull();
  expect(screen.queryByText("4E811C")).toBeNull();
  expect(screen.getByText("RP Profile")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "继续设置" }));
  expect(screen.getByText("第 2 步，共 3 步")).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "选择按键布局" })).toBeInTheDocument();
  expect(screen.getByRole("combobox", { name: "按键布局" })).toBeInTheDocument();
  expect(screen.queryByText(/键盘配置|硬件配置|运行分配/)).toBeNull();
  expect(screen.getByRole("option", { name: "RP Profile" })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "ESP Profile" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "下一步" }));
  expect(screen.getByText("第 3 步，共 3 步")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /RP Key/ })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "重新检测" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "高级 I/O 设置" })).toBeInTheDocument();
});

test("restarts key detection or opens advanced I/O from the test step", async () => {
  const user = userEvent.setup();
  const onOpenAdvanced = vi.fn();
  const { props } = renderWizard({
    devices: [unassignedDevice()],
    onOpenAdvanced,
  });
  await user.click(screen.getByRole("button", { name: "继续设置" }));
  await user.click(screen.getByRole("button", { name: "下一步" }));
  await user.click(screen.getByRole("button", { name: "高级 I/O 设置" }));
  expect(onOpenAdvanced).toHaveBeenCalledWith(
    "rp-device-id",
    "rp-profile",
    "rp-hardware",
  );

  await user.click(screen.getByRole("button", { name: "重新检测" }));
  expect(screen.getByText("第 3 步，共 3 步")).toBeInTheDocument();
});

test("shows setup input on the selected layout and completes with its exact assignment", async () => {
  const user = userEvent.setup();
  const onComplete = vi.fn().mockResolvedValue(undefined);
  const { rerender, props } = renderWizard({ devices: [unassignedDevice()], onComplete });
  await user.click(screen.getByRole("button", { name: "继续设置" }));
  await user.click(screen.getByRole("button", { name: "下一步" }));
  rerender(<DeviceSetupWizard {...props} devices={[unassignedDevice()]} inputEvent={{ timestampMs: 1, deviceId: "rp-device-id", input: { type: "direct", gpio: 0 }, pressed: true }} />);
  expect(screen.getByRole("button", { name: /RP Key/ })).toHaveClass("is-pressed");
  rerender(<DeviceSetupWizard {...props} devices={[unassignedDevice()]} inputEvent={{ timestampMs: 2, deviceId: "rp-device-id", input: { type: "direct", gpio: 0 }, pressed: false }} />);
  expect(screen.getByRole("button", { name: /RP Key/ })).not.toHaveClass("is-pressed");
  await user.click(screen.getByRole("button", { name: "完成设置" }));
  expect(onComplete).toHaveBeenCalledWith("rp-device-id", "RP2040 Pad · 4E811C", {
    device_profile_id: "rp-profile", hardware_profile_id: "rp-hardware",
  });
});

test("skips the key test and resumes it after the same device reconnects", async () => {
  const user = userEvent.setup();
  const connected = unassignedDevice();
  const onComplete = vi.fn().mockResolvedValue(undefined);
  const { rerender, props } = renderWizard({ devices: [connected], onComplete });
  await user.click(screen.getByRole("button", { name: "继续设置" }));
  await user.click(screen.getByRole("button", { name: "下一步" }));
  rerender(<DeviceSetupWizard {...props} devices={[]} />);
  expect(screen.getByRole("heading", { name: /键盘已断开/ })).toBeInTheDocument();

  rerender(<DeviceSetupWizard {...props} devices={[connected]} />);
  expect(screen.getByText("第 3 步，共 3 步")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "跳过测试" }));
  expect(onComplete).toHaveBeenCalledWith("rp-device-id", "RP2040 Pad · 4E811C", {
    device_profile_id: "rp-profile", hardware_profile_id: "rp-hardware",
  });
});

test("clears pressed test keys while the same device is disconnected", async () => {
  const user = userEvent.setup();
  const connected = unassignedDevice();
  const pressedEvent = {
    timestampMs: 1,
    deviceId: "rp-device-id",
    input: { type: "direct" as const, gpio: 0 },
    pressed: true,
  };
  const { rerender, props } = renderWizard({ devices: [connected] });
  await user.click(screen.getByRole("button", { name: "继续设置" }));
  await user.click(screen.getByRole("button", { name: "下一步" }));
  rerender(<DeviceSetupWizard {...props} devices={[connected]} inputEvent={pressedEvent} />);
  expect(screen.getByRole("button", { name: /RP Key/ })).toHaveClass("is-pressed");
  rerender(<DeviceSetupWizard {...props} devices={[]} inputEvent={pressedEvent} />);
  expect(screen.getByRole("heading", { name: /键盘已断开/ })).toBeInTheDocument();
  rerender(<DeviceSetupWizard {...props} devices={[connected]} inputEvent={pressedEvent} />);
  expect(screen.getByRole("button", { name: /RP Key/ })).not.toHaveClass("is-pressed");
});
