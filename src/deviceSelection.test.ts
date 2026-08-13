import { describe, expect, test } from "vitest";
import { assignedProfile, selectDeviceId } from "./deviceSelection";
import type { DeviceProfile, DeviceStatus } from "./types";

const profile = { profile: { id: "profile" } } as DeviceProfile;

function device(overrides: Partial<DeviceStatus> = {}): DeviceStatus {
  return {
    deviceId: "ready",
    name: "Ready keyboard",
    connection: "online",
    mode: "runtime",
    identity: "valid",
    assignment: "valid",
    runtime: "ready",
    hardwareSerial: "SERIAL",
    port: "/dev/cu.test",
    controllerFamilyId: "esp32s3",
    boardProfileId: "board",
    firmwareBuildId: null,
    capabilities: [],
    runtimeAssignment: { device_profile_id: "profile", hardware_profile_id: "hardware" },
    latestError: null,
    learning: null,
    ...overrides,
  };
}

describe("device selection", () => {
  test("retains a present selection, including an offline device", () => {
    const ready = device();
    const offline = device({ deviceId: "offline", connection: "offline", runtime: "inactive" });

    expect(selectDeviceId("offline", [ready, offline])).toBe("offline");
  });

  test("falls back by online availability and source order", () => {
    const offline = device({ deviceId: "offline", connection: "offline", runtime: "inactive" });
    const attention = device({ deviceId: "attention", assignment: "unassigned", runtime: "inactive" });
    const ready = device();
    const unassigned = device({ deviceId: "unassigned", assignment: "unassigned", runtime: "inactive" });

    expect(selectDeviceId("missing", [offline, attention, ready])).toBe("ready");
    expect(selectDeviceId(null, [offline, unassigned])).toBe("unassigned");
    expect(selectDeviceId(null, [])).toBeNull();
  });

  test("looks up only the selected device runtime assignment", () => {
    const ready = device();
    const unassigned = device({ assignment: "unassigned", runtimeAssignment: null });

    expect(assignedProfile(ready, [profile])).toBe(profile);
    expect(assignedProfile(unassigned, [profile])).toBeUndefined();
  });
});
