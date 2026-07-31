import { describe, expect, test } from "vitest";
import {
  compatibleHardwareProfiles,
  deviceSummary,
  editablePins,
  matchesDeviceFilter,
  primaryDeviceLabel,
} from "./deviceStatus";
import { previewSnapshot } from "./preview";
import type { DeviceStatus, HardwareProfile } from "./types";

function canonicalDeviceId(boardProfileId: string, hardwareSerial: string): string {
  const boardIdByteLength = new TextEncoder().encode(boardProfileId).byteLength;
  return `${boardIdByteLength}:${boardProfileId}${hardwareSerial}`;
}

function fixtureDevice(overrides: Partial<DeviceStatus> = {}): DeviceStatus {
  return {
    deviceId: "luatos-esp32s3-aio:ABCDEF123456",
    name: "前台电话键盘",
    connection: "online",
    mode: "runtime",
    identity: "valid",
    assignment: "valid",
    runtime: "ready",
    hardwareSerial: "ABCDEF123456",
    port: "/dev/cu.usbmodem-esp32",
    controllerFamilyId: "esp32s3",
    boardProfileId: "luatos-esp32s3-aio",
    firmwareBuildId: "esp32s3-20260731",
    capabilities: [0, 1, 2, 6, 7, 8],
    runtimeAssignment: {
      device_profile_id: "red-phone-v2",
      hardware_profile_id: "esp-primary",
    },
    latestError: null,
    learning: null,
    ...overrides,
  };
}

function fixtureHardware(overrides: Partial<HardwareProfile> = {}): HardwareProfile {
  return {
    id: "esp-primary",
    name: "前台接线",
    board_profile_id: "luatos-esp32s3-aio",
    debounce_ms: 30,
    inputs: [],
    ...overrides,
  };
}

describe("primaryDeviceLabel", () => {
  test("preserves attention priority across all five status dimensions", () => {
    expect(primaryDeviceLabel(fixtureDevice({ identity: "duplicate_identity", assignment: "invalid_assignment", runtime: "runtime_error", mode: "bootloader" }))).toBe("设备身份冲突");
    expect(primaryDeviceLabel(fixtureDevice({ assignment: "invalid_assignment", runtime: "runtime_error", mode: "bootloader" }))).toBe("分配需要修复");
    expect(primaryDeviceLabel(fixtureDevice({ runtime: "runtime_error", mode: "bootloader" }))).toBe("运行错误");
    expect(primaryDeviceLabel(fixtureDevice({ mode: "bootloader", assignment: "unassigned", runtime: "inactive" }))).toBe("引导加载模式");
    expect(primaryDeviceLabel(fixtureDevice({ assignment: "unassigned", runtime: "inactive" }))).toBe("未分配");
  });

  test("labels invalid identity, progress, ready, and offline states", () => {
    expect(primaryDeviceLabel(fixtureDevice({ identity: "invalid_identity" }))).toBe("设备身份无效");
    expect(primaryDeviceLabel(fixtureDevice({ identity: "validating", runtime: "inactive" }))).toBe("正在验证");
    expect(primaryDeviceLabel(fixtureDevice({ runtime: "configuring" }))).toBe("正在配置");
    expect(primaryDeviceLabel(fixtureDevice({ runtime: "learning" }))).toBe("正在学习");
    expect(primaryDeviceLabel(fixtureDevice())).toBe("就绪");
    expect(primaryDeviceLabel(fixtureDevice({ connection: "offline", mode: null, runtime: "inactive" }))).toBe("离线");
  });
});

describe("deviceSummary", () => {
  test("summarizes zero, one, and many device rows without collapsing same-board devices", () => {
    expect(deviceSummary([])).toEqual({ ready: 0, attention: 0, offline: 0, progress: 0 });
    expect(deviceSummary([fixtureDevice()])).toEqual({ ready: 1, attention: 0, offline: 0, progress: 0 });
    expect(deviceSummary([
      fixtureDevice({ deviceId: "same-board:A" }),
      fixtureDevice({ deviceId: "same-board:B", runtime: "configuring" }),
      fixtureDevice({ deviceId: "same-board:C", assignment: "invalid_assignment", runtime: "inactive" }),
      fixtureDevice({ deviceId: "same-board:D", connection: "offline", mode: null, runtime: "inactive" }),
    ])).toEqual({ ready: 1, attention: 1, offline: 1, progress: 1 });
  });

  test("does not count attention devices again as offline or progress", () => {
    const device = fixtureDevice({
      connection: "offline",
      mode: "bootloader",
      identity: "valid",
      assignment: "invalid_assignment",
      runtime: "configuring",
    });
    expect(deviceSummary([device])).toEqual({ ready: 0, attention: 1, offline: 0, progress: 0 });
  });
});

describe("matchesDeviceFilter", () => {
  test("matches status tabs from derived state", () => {
    expect(matchesDeviceFilter(fixtureDevice(), "all", "")).toBe(true);
    expect(matchesDeviceFilter(fixtureDevice(), "ready", "")).toBe(true);
    expect(matchesDeviceFilter(fixtureDevice({ assignment: "invalid_assignment" }), "attention", "")).toBe(true);
    expect(matchesDeviceFilter(fixtureDevice({ connection: "offline", mode: null, runtime: "inactive" }), "offline", "")).toBe(true);
    expect(matchesDeviceFilter(fixtureDevice({ runtime: "configuring" }), "ready", "")).toBe(false);
  });

  test.each([
    "前台电话",
    "abcdef123456",
    "LUATOS-ESP32S3-AIO",
    "USBMODEM-ESP32",
  ])("searches name, serial, board, and port using %s", (query) => {
    expect(matchesDeviceFilter(fixtureDevice(), "all", query)).toBe(true);
  });

  test("combines search and status filters", () => {
    const offline = fixtureDevice({ connection: "offline", mode: null, runtime: "inactive" });
    expect(matchesDeviceFilter(offline, "offline", "abcdef")).toBe(true);
    expect(matchesDeviceFilter(offline, "ready", "abcdef")).toBe(false);
    expect(matchesDeviceFilter(offline, "offline", "not-present")).toBe(false);
  });
});

test("matches Hardware Profiles by exact Board Profile ID", () => {
  const profiles = [
    fixtureHardware(),
    fixtureHardware({ id: "esp-secondary", name: "备用接线" }),
    fixtureHardware({ id: "rp-primary", board_profile_id: "vcc-gnd-yd-rp2040" }),
  ];
  expect(compatibleHardwareProfiles(profiles, "luatos-esp32s3-aio").map(({ id }) => id)).toEqual([
    "esp-primary",
    "esp-secondary",
  ]);
  expect(compatibleHardwareProfiles(profiles, "esp32s3")).toEqual([]);
});

test("intersects board safety with online capability and keeps board safety offline", () => {
  expect(editablePins([0, 1, 2, 22], [0, 2, 11])).toEqual([0, 2]);
  expect(editablePins([0, 1, 2, 22], null)).toEqual([0, 1, 2, 22]);
});

describe("preview fixture consistency", () => {
  test("uses the Rust canonical DeviceId encoding for every enrolled device", () => {
    for (const device of previewSnapshot.devices) {
      expect(device.deviceId).toBe(
        canonicalDeviceId(device.boardProfileId, device.hardwareSerial),
      );
    }
  });

  test("attributes every metric log to an enrolled device", () => {
    const enrolledDeviceIds = new Set(previewSnapshot.devices.map(({ deviceId }) => deviceId));
    const homeMetrics = previewSnapshot.homeMetrics;

    expect(homeMetrics).not.toBeNull();
    if (!homeMetrics) throw new Error("preview metrics fixture is required");
    for (const log of homeMetrics.logs) {
      expect(enrolledDeviceIds.has(log.deviceId)).toBe(true);
    }
  });

  test("keeps firmware build identity absent for offline devices", () => {
    const offlineDevices = previewSnapshot.devices.filter(({ connection }) => connection === "offline");

    expect(offlineDevices.length).toBeGreaterThan(0);
    for (const device of offlineDevices) {
      expect(device.firmwareBuildId).toBeNull();
    }
  });

  test("uses production bootloader observation keys", () => {
    const bootloaderCandidates = previewSnapshot.candidates.filter(({ mode }) => mode === "bootloader");

    expect(bootloaderCandidates.length).toBeGreaterThan(0);
    for (const candidate of bootloaderCandidates) {
      expect(candidate.key).toMatch(/^bootloader:\d+:\d+$/);
    }
  });
});
