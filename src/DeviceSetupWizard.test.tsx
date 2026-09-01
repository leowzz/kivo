import { render, screen, within } from "@testing-library/react";
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
    profile: { id: "rp-profile", name: "RP Profile", groups: [] },
    hardware_profiles: [
      {
        id: "rp-hardware",
        name: "RP Hardware",
        board_profile_id: "rp",
        debounce_ms: 30,
        inputs: [],
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
    onTargetChange: vi.fn(),
    onRetryCandidate: vi.fn().mockResolvedValue(undefined),
    onCreateProfile: vi.fn().mockResolvedValue(snapshot()),
    onComplete: vi.fn().mockResolvedValue(undefined),
    onClose: vi.fn(),
    ...overrides,
  };
  return { ...render(<DeviceSetupWizard {...props} />), props };
}

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
    screen.getByRole("heading", { name: "设备暂时没有回应" }),
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

  expect(screen.getByRole("heading", { name: "给你的键盘起个名字" })).toBeInTheDocument();
  expect(screen.getByText("RP2040 Pad")).toBeInTheDocument();
});

test("prefers a validated Device while a stale Candidate is still present", () => {
  renderWizard({
    targetId: "rp-device-id",
    candidates: [candidate()],
    devices: [unassignedDevice()],
  });

  expect(
    screen.getByRole("heading", { name: "给你的键盘起个名字" }),
  ).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "重新检测" })).toBeNull();
});

test("identity conflicts only show disambiguation guidance", () => {
  renderWizard({
    targetId: "rp-device-id",
    candidates: [
      candidate({
        issue: "duplicate_identity",
        identity: "duplicate_identity",
      }),
    ],
  });

  expect(screen.getByRole("heading", { name: "发现重复设备" })).toBeInTheDocument();
  expect(screen.getByText(/多个设备无法区分/)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "重新检测" })).toBeNull();
  expect(screen.queryByRole("button", { name: "先新建配置" })).toBeNull();
  expect(screen.getByRole("button", { name: "稍后处理" })).toBeInTheDocument();
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
        onTargetChange={setTargetId}
        onRetryCandidate={vi.fn().mockResolvedValue(undefined)}
        onCreateProfile={vi.fn().mockResolvedValue(snapshot(candidates))}
        onComplete={vi.fn().mockResolvedValue(undefined)}
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

test("shows three onboarding steps, auto-selects compatible hardware, and completes one exact Device", async () => {
  const user = userEvent.setup();
  const onComplete = vi.fn().mockResolvedValue(undefined);
  renderWizard({
    targetId: "rp-device-id",
    devices: [unassignedDevice()],
    onComplete,
  });

  expect(screen.getByRole("heading", { name: "给你的键盘起个名字" })).toBeInTheDocument();
  expect(screen.queryByText("键盘配置")).toBeNull();
  await user.click(screen.getByRole("button", { name: "下一步" }));
  expect(screen.getByRole("heading", { name: "选择一个用途开始" })).toBeInTheDocument();
  expect(screen.getByRole("radio", { name: /RP Profile/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  expect(screen.queryByRole("radio", { name: /ESP Profile/ })).toBeNull();
  expect(screen.getByText("已自动选择：RP Hardware")).not.toBeVisible();
  await user.click(screen.getByText("接线方案（高级）"));
  expect(screen.getByText("已自动选择：RP Hardware")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "下一步" }));

  expect(onComplete).toHaveBeenCalledTimes(1);
  expect(onComplete).toHaveBeenCalledWith("rp-device-id", "RP2040 Pad · 4E811C", {
    device_profile_id: "rp-profile",
    hardware_profile_id: "rp-hardware",
  });
  expect(screen.getByRole("heading", { name: "按一下实体按键" })).toBeInTheDocument();
  expect(screen.getByText("等待实体按键输入…")).toBeInTheDocument();
});

test("preserves the device name after setup failure", async () => {
  const user = userEvent.setup();
  const onComplete = vi.fn().mockRejectedValue(new Error("device_offline"));
  renderWizard({
    targetId: "rp-device-id",
    devices: [unassignedDevice()],
    onComplete,
  });
  await user.clear(screen.getByRole("textbox", { name: "设备名称" }));
  await user.type(screen.getByRole("textbox", { name: "设备名称" }), "保留名称");
  await user.click(screen.getByRole("button", { name: "下一步" }));
  await user.click(screen.getByRole("button", { name: "下一步" }));

  expect(await screen.findByRole("alert")).toHaveTextContent("device_offline");
  await user.click(screen.getByRole("button", { name: "返回" }));
  expect(screen.getByRole("textbox", { name: "设备名称" })).toHaveValue("保留名称");
});

test("preserves setup fields when the same Device reconnects", async () => {
  const user = userEvent.setup();
  const connected = unassignedDevice();
  const { rerender, props } = renderWizard({ devices: [connected] });
  await user.clear(screen.getByRole("textbox", { name: "设备名称" }));
  await user.type(
    screen.getByRole("textbox", { name: "设备名称" }),
    "重连后保留",
  );
  await user.click(screen.getByRole("button", { name: "下一步" }));

  rerender(<DeviceSetupWizard {...props} devices={[]} />);
  expect(
    screen.getByRole("heading", { name: /键盘已断开/ }),
  ).toBeInTheDocument();

  rerender(<DeviceSetupWizard {...props} devices={[connected]} />);
  expect(
    screen.getByRole("heading", { name: "选择一个用途开始" }),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "返回" }));
  expect(screen.getByRole("textbox", { name: "设备名称" })).toHaveValue("重连后保留");
});

test("shows controlled physical-key success and exposes retry for recovery", async () => {
  const user = userEvent.setup();
  const onComplete = vi.fn().mockResolvedValue(undefined);
  const onVerificationRetry = vi.fn().mockResolvedValue(undefined);
  const { rerender, props } = renderWizard({
    devices: [unassignedDevice()],
    onComplete,
    onVerificationRetry,
  });

  await user.click(screen.getByRole("button", { name: "下一步" }));
  await user.click(screen.getByRole("button", { name: "下一步" }));
  expect(screen.getByText("等待实体按键输入…")).toBeInTheDocument();

  rerender(
    <DeviceSetupWizard
      {...props}
      devices={[unassignedDevice()]}
      verification={{ status: "timeout", buttonLabel: "按键 1" }}
    />,
  );
  await user.click(screen.getByRole("button", { name: "重新按键" }));
  expect(onVerificationRetry).toHaveBeenCalledWith("rp-device-id");

  rerender(
    <DeviceSetupWizard
      {...props}
      devices={[unassignedDevice()]}
      verification={{ status: "error", detail: "paste denied" }}
    />,
  );
  expect(screen.getByText("验证时遇到问题")).toBeInTheDocument();
  expect(screen.getByText("paste denied")).toBeInTheDocument();

  rerender(
    <DeviceSetupWizard
      {...props}
      devices={[unassignedDevice()]}
      verification={{ status: "success", buttonLabel: "按键 1" }}
    />,
  );
  expect(screen.getByText("按键已响应")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "进入键盘工作区" })).toBeInTheDocument();
});

test("creates a compatible blank setup when the blank choice is selected", async () => {
  const user = userEvent.setup();
  const blank = {
    ...profiles[0],
    profile: { ...profiles[0].profile, id: "rp-blank", name: "RP2040 Pad · 4E811C" },
  };
  const onCreateProfile = vi.fn().mockResolvedValue({
    ...snapshot(),
    deviceProfiles: [...profiles, blank],
    editorProfile: "rp-blank",
  });
  const onComplete = vi.fn().mockResolvedValue(undefined);
  renderWizard({
    devices: [unassignedDevice()],
    onCreateProfile,
    onComplete,
  });

  await user.click(screen.getByRole("button", { name: "下一步" }));
  await user.click(screen.getByRole("radio", { name: /从空白开始/ }));
  await user.click(screen.getByRole("button", { name: "下一步" }));

  expect(onCreateProfile).toHaveBeenCalledWith({
    kind: "blank",
    name: "RP2040 Pad · 4E811C",
    board_profile_id: "rp",
  });
  expect(onComplete).toHaveBeenCalledWith("rp-device-id", "RP2040 Pad · 4E811C", {
    device_profile_id: "rp-blank",
    hardware_profile_id: "rp-hardware",
  });
});

test("lets the host prepare a device-specific assignment before completing setup", async () => {
  const user = userEvent.setup();
  const onPrepareProfile = vi.fn().mockResolvedValue({
    device_profile_id: "rp-device-copy",
    hardware_profile_id: "rp-hardware-copy",
  });
  const onComplete = vi.fn().mockResolvedValue(undefined);
  renderWizard({
    devices: [unassignedDevice()],
    onPrepareProfile,
    onComplete,
  });

  await user.click(screen.getByRole("button", { name: "下一步" }));
  await user.click(screen.getByRole("button", { name: "下一步" }));

  expect(onPrepareProfile).toHaveBeenCalledWith(
    "rp-device-id",
    "RP2040 Pad · 4E811C",
    "rp-profile",
    "rp-hardware",
  );
  expect(onComplete).toHaveBeenCalledWith("rp-device-id", "RP2040 Pad · 4E811C", {
    device_profile_id: "rp-device-copy",
    hardware_profile_id: "rp-hardware-copy",
  });
});

test("lets the host prepare a blank device-specific assignment", async () => {
  const user = userEvent.setup();
  const onPrepareProfile = vi.fn().mockResolvedValue({
    device_profile_id: "rp-device-blank",
    hardware_profile_id: "rp-hardware-blank",
  });
  const onComplete = vi.fn().mockResolvedValue(undefined);
  renderWizard({
    devices: [unassignedDevice()],
    onPrepareProfile,
    onComplete,
  });

  await user.click(screen.getByRole("button", { name: "下一步" }));
  await user.click(screen.getByRole("radio", { name: /从空白开始/ }));
  await user.click(screen.getByRole("button", { name: "下一步" }));

  expect(onPrepareProfile).toHaveBeenCalledWith(
    "rp-device-id",
    "RP2040 Pad · 4E811C",
    null,
    null,
  );
  expect(onComplete).toHaveBeenCalledWith("rp-device-id", "RP2040 Pad · 4E811C", {
    device_profile_id: "rp-device-blank",
    hardware_profile_id: "rp-hardware-blank",
  });
});
