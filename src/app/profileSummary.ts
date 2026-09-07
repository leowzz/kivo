import type { DeviceProfile, ImportPreview, InputSource } from "./types";

export interface ProfileContentSummary {
  profileCount: number;
  buttonCount: number;
  hardwareBindingCount: number;
  actionCount: number;
}

function inputBindingCount(input: InputSource) {
  if (input.type === "feature_switch") return input.buttons.length;
  return Object.keys(input.keys).length;
}

export function summarizeProfiles(
  profiles: DeviceProfile[],
): ProfileContentSummary {
  const summary: ProfileContentSummary = {
    profileCount: profiles.length,
    buttonCount: 0,
    hardwareBindingCount: 0,
    actionCount: 0,
  };

  for (const profile of profiles) {
    for (const group of profile.profile.groups) {
      summary.buttonCount += group.buttons.length;
    }
    for (const hardware of profile.hardware_profiles) {
      for (const input of hardware.inputs) {
        summary.hardwareBindingCount += inputBindingCount(input);
      }
    }
    for (const triggerActions of Object.values(profile.actions)) {
      summary.actionCount += triggerActions.press.length +
        triggerActions.release.length +
        triggerActions.long_press.length +
        triggerActions.double_press.length;
    }
  }

  return summary;
}

export function projectImportedProfiles(
  profiles: DeviceProfile[],
  preview: ImportPreview,
): ProfileContentSummary {
  const current = summarizeProfiles(profiles);
  const replaced = preview.replacesExisting
    ? profiles.find((profile) => profile.profile.id === preview.profileId)
    : undefined;
  const removed = replaced ? summarizeProfiles([replaced]) : {
    profileCount: 0,
    buttonCount: 0,
    hardwareBindingCount: 0,
    actionCount: 0,
  };
  return {
    profileCount: current.profileCount - removed.profileCount + 1,
    buttonCount: current.buttonCount - removed.buttonCount + preview.buttonCount,
    hardwareBindingCount: current.hardwareBindingCount -
      removed.hardwareBindingCount + preview.hardwareBindingCount,
    actionCount: current.actionCount - removed.actionCount + preview.actionCount,
  };
}
