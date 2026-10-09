import type { ControlPanelConfig, HardwareProfile } from "./types";

export function controlPins(controls: ControlPanelConfig | undefined): number[] {
  return controls ? [controls.confirm, controls.encoder_press,
    controls.encoder_a, controls.encoder_b, controls.back] : [];
}

export function inputPins(hardware: HardwareProfile): number[] {
  return hardware.inputs.flatMap((source) => source.type === "direct"
    ? Object.values(source.keys)
    : source.type === "contact_matrix" ? source.pins : [source.gpio]);
}

export function hardwarePins(hardware: HardwareProfile): number[] {
  return [...inputPins(hardware),
    ...(hardware.display ? [hardware.display.sda, hardware.display.scl] : []),
    ...controlPins(hardware.controls)];
}
