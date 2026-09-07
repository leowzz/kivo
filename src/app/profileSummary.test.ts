import { expect, test } from "vitest";
import { projectImportedProfiles, summarizeProfiles } from "./profileSummary";
import type { DeviceProfile } from "./types";

const profile: DeviceProfile = {
  schema_version: 3,
  profile: {
    id: "desk",
    name: "Desk",
    groups: [
      {
        id: "main",
        columns: 2,
        buttons: [
          { id: "A", label: "A" },
          { id: "B", label: "B" },
        ],
      },
    ],
  },
  trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
  hardware_profiles: [
    {
      id: "hardware",
      name: "Hardware",
      board_profile_id: "board",
      debounce_ms: 30,
      inputs: [
        { type: "direct", id: "direct", keys: { A: 1 } },
        {
          type: "contact_matrix",
          id: "matrix",
          pins: [2, 3],
          keys: { B: [2, 3] },
        },
        {
          type: "feature_switch",
          id: "switch",
          name: "Mode",
          gpio: 4,
          buttons: ["A", "B"],
        },
      ],
    },
  ],
  actions: {
    A: {
      press: [{ type: "paste", text: "hello" }],
      release: [],
      long_press: [{ type: "media", command: "mute" }],
      double_press: [],
    },
    B: {
      press: [
        { type: "delay", duration_ms: 500 },
        { type: "hotkey", keys: ["enter"] },
      ],
      release: [],
      long_press: [],
      double_press: [],
    },
  },
};

test("summarizes profiles using the same content counts as backup previews", () => {
  expect(summarizeProfiles([profile])).toEqual({
    profileCount: 1,
    buttonCount: 2,
    hardwareBindingCount: 4,
    actionCount: 4,
  });
});

test("returns zero counts for an empty workspace", () => {
  expect(summarizeProfiles([])).toEqual({
    profileCount: 0,
    buttonCount: 0,
    hardwareBindingCount: 0,
    actionCount: 0,
  });
});

test("projects replacement imports against the existing profile", () => {
  expect(projectImportedProfiles([profile], {
    profileId: "desk",
    profileName: "New Desk",
    buttonCount: 5,
    hardwareBindingCount: 3,
    actionCount: 8,
    replacesExisting: true,
  })).toEqual({
    profileCount: 1,
    buttonCount: 5,
    hardwareBindingCount: 3,
    actionCount: 8,
  });
});

test("projects new imports without removing existing content", () => {
  expect(projectImportedProfiles([profile], {
    profileId: "meeting",
    profileName: "Meeting",
    buttonCount: 3,
    hardwareBindingCount: 2,
    actionCount: 4,
    replacesExisting: false,
  })).toEqual({
    profileCount: 2,
    buttonCount: 5,
    hardwareBindingCount: 6,
    actionCount: 8,
  });
});

test("does not remove a same-id profile when preview says the import is additive", () => {
  expect(projectImportedProfiles([profile], {
    profileId: "desk",
    profileName: "Desk copy",
    buttonCount: 5,
    hardwareBindingCount: 3,
    actionCount: 8,
    replacesExisting: false,
  })).toEqual({
    profileCount: 2,
    buttonCount: 7,
    hardwareBindingCount: 7,
    actionCount: 12,
  });
});
