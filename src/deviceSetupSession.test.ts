import { expect, test } from "vitest";
import { reconcileSetupSession, setupPresence } from "./deviceSetupSession";
import type { CandidateStatus, DeviceStatus } from "./types";

const candidate = {
  key: "runtime:/dev/cu.usbmodem1101",
  deviceId: "stable-rp",
  mode: "runtime",
  identity: "validating",
  issue: "validating",
  rawSerial: "SERIAL",
  port: "/dev/cu.usbmodem1101",
  controllerFamilyId: "rp2040",
  boardProfileId: "rp",
  latestError: null,
} satisfies CandidateStatus;

const device = {
  deviceId: "stable-rp",
  name: "RP",
  connection: "online",
  mode: "runtime",
  identity: "valid",
  assignment: "unassigned",
  runtime: "inactive",
  hardwareSerial: "SERIAL",
  port: "/dev/cu.usbmodem1101",
  controllerFamilyId: "rp2040",
  boardProfileId: "rp",
  firmwareBuildId: "build",
  capabilities: [],
  runtimeAssignment: null,
  latestError: null,
  learning: null,
} satisfies DeviceStatus;

test("keeps one insertion identity across Candidate to Device transition", () => {
  expect(setupPresence([], [candidate])).toEqual([
    { id: "stable-rp", eligible: true },
  ]);
  expect(setupPresence([device], [])).toEqual([
    { id: "stable-rp", eligible: true },
  ]);
});

test("suppresses a dismissed identity until it fully disappears", () => {
  const opened = reconcileSetupSession(
    new Set(),
    setupPresence([], [candidate]),
  );
  expect(opened.autoTargetId).toBe("stable-rp");
  expect(opened.seen).toEqual(new Set(["stable-rp"]));

  const stillPresent = reconcileSetupSession(
    opened.seen,
    setupPresence([device], []),
  );
  expect(stillPresent.autoTargetId).toBeNull();
  expect(stillPresent.seen).toEqual(new Set(["stable-rp"]));

  const removed = reconcileSetupSession(stillPresent.seen, []);
  expect(removed.seen.size).toBe(0);
  const reinserted = reconcileSetupSession(
    removed.seen,
    setupPresence([], [candidate]),
  );
  expect(reinserted.autoTargetId).toBe("stable-rp");
});

test("retains assigned online identities for cycle suppression but does not auto-open them", () => {
  const assigned = {
    ...device,
    assignment: "valid",
    runtimeAssignment: {
      device_profile_id: "p",
      hardware_profile_id: "h",
    },
  } satisfies DeviceStatus;
  expect(setupPresence([assigned], [])).toEqual([
    { id: "stable-rp", eligible: false },
  ]);
  expect(
    reconcileSetupSession(new Set(), setupPresence([assigned], [])).autoTargetId,
  ).toBeNull();
});
