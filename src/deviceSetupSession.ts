import type { CandidateStatus, DeviceStatus } from "./types";

export interface SetupPresence {
  id: string;
  eligible: boolean;
}

export function candidateSetupId(candidate: CandidateStatus) {
  return candidate.deviceId ?? `candidate:${candidate.key}`;
}

export function setupPresence(
  devices: DeviceStatus[],
  candidates: CandidateStatus[],
): SetupPresence[] {
  const presence = new Map<string, SetupPresence>();
  for (const candidate of candidates) {
    const id = candidateSetupId(candidate);
    presence.set(id, { id, eligible: true });
  }
  for (const device of devices) {
    if (device.connection !== "online") continue;
    presence.set(device.deviceId, {
      id: device.deviceId,
      eligible:
        device.mode === "runtime" &&
        device.identity === "valid" &&
        device.assignment === "unassigned",
    });
  }
  return [...presence.values()];
}

export function reconcileSetupSession(
  previousSeen: Set<string>,
  presence: SetupPresence[],
) {
  const present = new Set(presence.map(({ id }) => id));
  const seen = new Set([...previousSeen].filter((id) => present.has(id)));
  const autoTargetId =
    presence.find(({ id, eligible }) => eligible && !seen.has(id))?.id ?? null;
  if (autoTargetId) seen.add(autoTargetId);
  return { seen, autoTargetId };
}
