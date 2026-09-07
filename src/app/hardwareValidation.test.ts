import { expect, test } from "vitest";
import { hardwareProfilesAreValid } from "./hardwareValidation";
import type { BoardProfileSummary, HardwareProfile, ModelLayout } from "./types";

const layout: ModelLayout = {
  id: "desk-phone",
  name: "Desk phone",
  groups: [{ id: "keys", columns: 2, buttons: [
    { id: "ONE", label: "1" },
    { id: "TWO", label: "2" },
  ] }],
};

const boardProfiles: BoardProfileSummary[] = [
  {
    id: "esp-board",
    controllerFamilyId: "esp32s3",
    displayName: "ESP Board",
    runtimeUsb: "303a:4002",
    bootloaderUsb: null,
    safePins: [1, 2, 6, 12, 13],
    supportsOled: false,
  },
  {
    id: "yd-rp2040",
    controllerFamilyId: "rp2040",
    displayName: "YD-RP2040",
    runtimeUsb: "2e8a:000a",
    bootloaderUsb: "2e8a:0003",
    safePins: Array.from({ length: 24 }, (_, pin) => pin),
    supportsOled: true,
  },
];

const hardwareProfiles: HardwareProfile[] = [
  {
    id: "front-desk",
    name: "Front desk",
    board_profile_id: "esp-board",
    debounce_ms: 30,
    inputs: [
      { type: "direct", id: "direct", keys: { ONE: 6 } },
      {
        type: "contact_matrix",
        id: "matrix",
        pins: [1, 2, 12, 13],
        keys: { TWO: [1, 12] },
      },
    ],
  },
  {
    id: "back-desk",
    name: "Back desk",
    board_profile_id: "esp-board",
    debounce_ms: 45,
    inputs: [{ type: "direct", id: "direct-back", keys: { ONE: 2 } }],
  },
];

test("accepts GPIO26 through GPIO29 across direct, matrix, and OLED ownership", () => {
  const rpBoards = boardProfiles.map((board) => board.id === "yd-rp2040"
    ? { ...board, safePins: Array.from({ length: 30 }, (_, pin) => pin) }
    : board);
  const profile: HardwareProfile = {
    id: "high-gpio",
    name: "High GPIO",
    board_profile_id: "yd-rp2040",
    debounce_ms: 30,
    ssd1306: { sda: 28, scl: 29 },
    inputs: [
      { type: "direct", id: "direct", keys: { ONE: 26 } },
      {
        type: "contact_matrix",
        id: "matrix",
        pins: [0, 27],
        keys: { TWO: [0, 27] },
      },
    ],
  };

  expect(hardwareProfilesAreValid([profile], rpBoards)).toBe(true);

});

test("rejects hardware bindings for buttons removed from the layout", () => {
  const inconsistent = structuredClone(hardwareProfiles);
  inconsistent[0].inputs[0] = {
    type: "direct",
    id: "direct",
    keys: { ONE: 6, REMOVED: 12 },
  };

  expect(hardwareProfilesAreValid(inconsistent, boardProfiles, layout)).toBe(false);
});

test("rejects unsupported, unsafe, same-pin, conflicting, and duplicate pin ownership", () => {
  const base = {
    id: "validation",
    name: "Validation",
    board_profile_id: "yd-rp2040",
    debounce_ms: 30,
    inputs: [] as HardwareProfile["inputs"],
  };
  const withOled = (sh1106: NonNullable<HardwareProfile["sh1106"]>, inputs = base.inputs) => ({
    ...base,
    inputs,
    sh1106,
  });

  expect(hardwareProfilesAreValid([
    { ...withOled({ sda: 1, scl: 2 }), board_profile_id: "esp-board" },
  ], boardProfiles)).toBe(false);
  expect(hardwareProfilesAreValid([withOled({ sda: 24, scl: 22 })], boardProfiles)).toBe(false);
  expect(hardwareProfilesAreValid([withOled({ sda: 18, scl: 18 })], boardProfiles)).toBe(false);
  expect(hardwareProfilesAreValid([
    withOled({ sda: 18, scl: 19 }, [{ type: "direct", id: "direct", keys: { ONE: 18 } }]),
  ], boardProfiles)).toBe(false);
  const controlPanel = {
    type: "ec11_confirm_back" as const,
    confirm: 3,
    encoder_press: 4,
    encoder_a: 5,
    encoder_b: 6,
    back: 7,
  };
  expect(hardwareProfilesAreValid([
    withOled({ sda: 18, scl: 19, control_panel: controlPanel }),
  ], boardProfiles)).toBe(true);
  expect(hardwareProfilesAreValid([
    withOled({
      sda: 18,
      scl: 19,
      control_panel: { ...controlPanel, confirm: 18 },
    }),
  ], boardProfiles)).toBe(false);
  expect(hardwareProfilesAreValid([
    withOled({
      sda: 18,
      scl: 19,
      control_panel: { ...controlPanel, back: 24 },
    }),
  ], boardProfiles)).toBe(false);
  expect(hardwareProfilesAreValid([{
    ...base,
    inputs: [
      { type: "direct", id: "first", keys: { ONE: 3 } },
      { type: "direct", id: "second", keys: { TWO: 3 } },
    ],
  }], boardProfiles)).toBe(false);


});
