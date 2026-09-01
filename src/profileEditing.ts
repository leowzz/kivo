import type { DeviceProfile, InputSource, ModelLayout } from "./types";

function reconcileInputSource(source: InputSource, buttonIds: ReadonlySet<string>): InputSource {
  if (source.type === "feature_switch") {
    return {
      ...source,
      buttons: source.buttons.filter((buttonId) => buttonIds.has(buttonId)),
    };
  }

  return {
    ...source,
    keys: Object.fromEntries(
      Object.entries(source.keys).filter(([buttonId]) => buttonIds.has(buttonId)),
    ),
  };
}

export function reconcileProfileLayout(
  profile: DeviceProfile,
  layout: ModelLayout,
): DeviceProfile {
  const buttonIds = new Set(
    layout.groups.flatMap((group) => group.buttons.map((button) => button.id)),
  );

  return {
    ...profile,
    profile: layout,
    hardware_profiles: profile.hardware_profiles.map((hardware) => ({
      ...hardware,
      inputs: hardware.inputs.map((source) => reconcileInputSource(source, buttonIds)),
    })),
    actions: Object.fromEntries(
      Object.entries(profile.actions).filter(([buttonId]) => buttonIds.has(buttonId)),
    ),
  };
}
