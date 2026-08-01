import { render, screen } from "@testing-library/react";
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
    schema_version: 2,
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
    actions: {},
  },
  {
    schema_version: 2,
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

  expect(screen.getByRole("heading", { name: "选择键盘配置" })).toBeInTheDocument();
  expect(screen.getByText("RP2040 Pad")).toBeInTheDocument();
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
      rawSerial: "SECOND",
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
  await user.click(screen.getByRole("button", { name: /SECOND/ }));
  expect(screen.getByText("正在确认设备")).toBeInTheDocument();
});

test("lists only exact-board profiles and completes one exact Device", async () => {
  const user = userEvent.setup();
  const onComplete = vi.fn().mockResolvedValue(undefined);
  renderWizard({
    targetId: "rp-device-id",
    devices: [unassignedDevice()],
    onComplete,
  });

  expect(screen.getByRole("option", { name: "RP Profile" })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "ESP Profile" })).toBeNull();
  await user.selectOptions(
    screen.getByRole("combobox", { name: "键盘配置" }),
    "rp-profile",
  );
  await user.selectOptions(
    screen.getByRole("combobox", { name: "硬件配置" }),
    "rp-hardware",
  );
  await user.click(screen.getByRole("button", { name: "下一步" }));
  await user.clear(screen.getByRole("textbox", { name: "键盘名称" }));
  await user.type(screen.getByRole("textbox", { name: "键盘名称" }), "桌面 RP2040");
  await user.click(screen.getByRole("button", { name: "完成设置" }));

  expect(onComplete).toHaveBeenCalledTimes(1);
  expect(onComplete).toHaveBeenCalledWith("rp-device-id", "桌面 RP2040", {
    device_profile_id: "rp-profile",
    hardware_profile_id: "rp-hardware",
  });
});

test("preserves confirmation fields after setup failure", async () => {
  const user = userEvent.setup();
  const onComplete = vi.fn().mockRejectedValue(new Error("device_offline"));
  renderWizard({
    targetId: "rp-device-id",
    devices: [unassignedDevice()],
    onComplete,
  });
  await user.click(screen.getByRole("button", { name: "下一步" }));
  await user.clear(screen.getByRole("textbox", { name: "键盘名称" }));
  await user.type(screen.getByRole("textbox", { name: "键盘名称" }), "保留名称");
  await user.click(screen.getByRole("button", { name: "完成设置" }));

  expect(await screen.findByRole("alert")).toHaveTextContent("device_offline");
  expect(screen.getByRole("textbox", { name: "键盘名称" })).toHaveValue("保留名称");
});
