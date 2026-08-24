import { expect, test } from "vitest";
import { reconcileProfileLayout } from "./profileEditing";
import type { DeviceProfile, ModelLayout } from "./types";

const layout: ModelLayout = {
  id: "desk",
  name: "Desk",
  groups: [{
    id: "keys",
    columns: 1,
    buttons: [{ id: "KEY_1", label: "One" }],
  }],
};

const profile: DeviceProfile = {
  schema_version: 3,
  profile: {
    ...layout,
    groups: [{
      ...layout.groups[0],
      buttons: [
        ...layout.groups[0].buttons,
        { id: "KEY_2", label: "Two" },
      ],
    }],
  },
  trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
  hardware_profiles: [{
    id: "hardware",
    name: "Hardware",
    board_profile_id: "yd-rp2040",
    debounce_ms: 30,
    inputs: [
      { type: "direct", id: "direct", keys: { KEY_1: 1, KEY_2: 2 } },
      { type: "contact_matrix", id: "matrix", pins: [3, 4], keys: { KEY_2: [3, 4] } },
      { type: "feature_switch", id: "switch", name: "Mode", gpio: 5, buttons: ["KEY_1", "KEY_2"] },
    ],
  }],
  actions: {
    KEY_1: { press: [], release: [], long_press: [], double_press: [] },
    KEY_2: { press: [], release: [], long_press: [], double_press: [] },
  },
};

test("removing a layout button removes every persisted reference to it", () => {
  const reconciled = reconcileProfileLayout(profile, layout);
  const hardware = reconciled.hardware_profiles[0];

  expect(reconciled.profile).toEqual(layout);
  expect(hardware.inputs[0]).toMatchObject({ keys: { KEY_1: 1 } });
  expect(hardware.inputs[1]).toMatchObject({ keys: {} });
  expect(hardware.inputs[2]).toMatchObject({ buttons: ["KEY_1"] });
  expect(Object.keys(reconciled.actions)).toEqual(["KEY_1"]);
});
