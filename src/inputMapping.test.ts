import { expect, test } from "vitest";
import { resolveButton } from "./inputMapping";
import type { HardwareProfile } from "./types";

const hardware: HardwareProfile = {
  id: "rp-hardware",
  name: "RP Hardware",
  board_profile_id: "rp",
  debounce_ms: 30,
  inputs: [
    { type: "direct", id: "empty", keys: {} },
    { type: "direct", id: "direct", keys: { button_a: 1 } },
    {
      type: "contact_matrix",
      id: "matrix",
      pins: [2, 3],
      keys: { button_b: [2, 3] },
    },
  ],
};

test("resolves a direct GPIO input", () => {
  expect(resolveButton(hardware, { type: "direct", gpio: 1 })).toBe("button_a");
});

test("normalizes a contact pair and uses the runtime source index", () => {
  expect(
    resolveButton(hardware, { type: "contact", source: 1, pin_a: 3, pin_b: 2 }),
  ).toBe("button_b");
});

test("returns null for an unknown physical input", () => {
  expect(resolveButton(hardware, { type: "direct", gpio: 99 })).toBeNull();
});

test("returns null when no hardware profile is selected", () => {
  expect(resolveButton(undefined, { type: "direct", gpio: 1 })).toBeNull();
});
