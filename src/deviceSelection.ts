import type { DeviceProfile, DeviceStatus } from "./types";

function availabilityRank(device: DeviceStatus) {
  if (device.connection === "offline") return 4;
  if (device.runtime === "ready") return 0;
  if (device.runtime === "configuring" || device.runtime === "learning") return 1;
  if (
    device.assignment === "unassigned" ||
    device.assignment === "invalid_assignment" ||
    device.runtime === "runtime_error" ||
    device.identity === "invalid_identity" ||
    device.identity === "duplicate_identity" ||
    device.mode === "bootloader"
  ) return 2;
  return 3;
}

export function selectDeviceId(
  currentId: string | null,
  devices: readonly DeviceStatus[],
): string | null {
  if (currentId && devices.some((device) => device.deviceId === currentId)) return currentId;

  let selected: DeviceStatus | undefined;
  for (const device of devices) {
    if (!selected || availabilityRank(device) < availabilityRank(selected)) selected = device;
  }
  return selected?.deviceId ?? null;
}

export function assignedProfile(
  device: DeviceStatus | null,
  profiles: readonly DeviceProfile[],
): DeviceProfile | undefined {
  const profileId = device?.runtimeAssignment?.device_profile_id;
  return profileId ? profiles.find((profile) => profile.profile.id === profileId) : undefined;
}
