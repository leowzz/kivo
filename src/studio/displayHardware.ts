import { controlPins, hardwarePins, inputPins } from "../shared/hardwarePins";
import type { DisplayPanel, HardwareProfile, ProductDefinition } from "../shared/types";
import type { StudioBoard } from "./types";

export type DisplaySelection = "none" | DisplayPanel;

function availablePins(board: StudioBoard, occupied: number[]) {
  const used = new Set(occupied);
  return [...board.safePins].sort((a, b) => a - b).filter((pin) => pin > 0 && !used.has(pin));
}

export function displayCanFit(hardware: HardwareProfile, board: StudioBoard) {
  return board.supportsDisplay &&
    availablePins(board, [...inputPins(hardware), ...controlPins(hardware.controls)]).length >= 2;
}

export function controlsCanFit(hardware: HardwareProfile, board: StudioBoard) {
  return Boolean(hardware.display) && availablePins(board,
    hardwarePins({ ...hardware, controls: undefined })).length >= 5;
}

export function configureDisplay(definition: ProductDefinition, board: StudioBoard, panel: DisplaySelection) {
  const hardware = definition.hardware_profile;
  if (panel === "none") {
    hardware.display = undefined;
    hardware.controls = undefined;
    return;
  }
  if (!displayCanFit(hardware, board)) return;
  const available = availablePins(board, [...inputPins(hardware), ...controlPins(hardware.controls)]);
  const existing = hardware.display;
  const keep = existing && existing.sda !== existing.scl &&
    available.includes(existing.sda) && available.includes(existing.scl);
  hardware.display = {
    panel,
    sda: keep ? existing.sda : available[available.length - 2],
    scl: keep ? existing.scl : available[available.length - 1],
    address: existing?.address ?? 60,
  };
  if (!definition.product.capabilities.includes("disp")) definition.product.capabilities.push("disp");
}

export function configureControls(definition: ProductDefinition, board: StudioBoard, enabled: boolean) {
  const hardware = definition.hardware_profile;
  if (!enabled) {
    hardware.controls = undefined;
    return;
  }
  if (!controlsCanFit(hardware, board)) return;
  const available = availablePins(board, hardwarePins({ ...hardware, controls: undefined }));
  const existing = controlPins(hardware.controls);
  const pins = existing.length === 5 && new Set(existing).size === 5 &&
    existing.every((pin) => available.includes(pin)) ? existing : available.slice(0, 5);
  const [confirm, encoder_press, encoder_a, encoder_b, back] = pins;
  hardware.controls = { type: "ec11_confirm_back", confirm, encoder_press, encoder_a, encoder_b, back };
  definition.product.capabilities = definition.product.capabilities.filter((value) => value !== "enc");
  if (!definition.product.capabilities.includes("encp")) definition.product.capabilities.push("encp");
}
