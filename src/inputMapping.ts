import type { HardwareProfile, PhysicalInput } from "./types";

export function resolveButton(
  hardware: HardwareProfile | undefined,
  input: PhysicalInput,
): string | null {
  if (!hardware) return null;
  let runtimeSource = 0;
  for (const source of hardware.inputs) {
    if (Object.keys(source.keys).length === 0) continue;
    if (source.type === "direct" && input.type === "direct") {
      const match = Object.entries(source.keys).find(
        ([, gpio]) => gpio === input.gpio,
      );
      if (match) return match[0];
    }
    if (
      source.type === "contact_matrix" &&
      input.type === "contact" &&
      input.source === runtimeSource
    ) {
      const pair = [
        Math.min(input.pin_a, input.pin_b),
        Math.max(input.pin_a, input.pin_b),
      ];
      const match = Object.entries(source.keys).find(
        ([, pins]) => pins[0] === pair[0] && pins[1] === pair[1],
      );
      if (match) return match[0];
    }
    runtimeSource += 1;
  }
  return null;
}
