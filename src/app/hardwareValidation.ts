import type {
  BoardProfileSummary,
  HardwareProfile,
  ModelLayout,
} from "./types";

function boardSafePins(board: BoardProfileSummary | undefined) {
  if (!board) return [];
  return board.id === "yd-rp2040"
    ? board.safePins.filter(
        (pin) => pin >= 0 && (pin <= 23 || (pin >= 26 && pin <= 29)),
      )
    : board.safePins;
}

function ownedInputPins(hardware: HardwareProfile) {
  return hardware.inputs.flatMap((source) =>
    source.type === "direct"
      ? Object.values(source.keys)
      : source.type === "contact_matrix"
        ? source.pins
        : [source.gpio],
  );
}

function conflictingPins(hardware: HardwareProfile) {
  const counts = new Map<number, number>();
  const add = (pin: number) => counts.set(pin, (counts.get(pin) ?? 0) + 1);
  ownedInputPins(hardware).forEach(add);
  if (hardware.ssd1306) {
    add(hardware.ssd1306.sda);
    add(hardware.ssd1306.scl);
  }
  if (hardware.sh1106) {
    add(hardware.sh1106.sda);
    add(hardware.sh1106.scl);
    if (hardware.sh1106.control_panel) {
      const panel = hardware.sh1106.control_panel;
      add(panel.confirm);
      add(panel.encoder_press);
      add(panel.encoder_a);
      add(panel.encoder_b);
      add(panel.back);
    }
  }
  return new Set(
    [...counts.entries()].filter(([, count]) => count > 1).map(([pin]) => pin),
  );
}

function invalidBoardPins(
  hardware: HardwareProfile,
  boardProfiles: readonly BoardProfileSummary[],
) {
  const board = boardProfiles.find(
    ({ id }) => id === hardware.board_profile_id,
  );
  if (!board)
    return new Set(
      hardware.inputs.flatMap((source) =>
        source.type === "direct"
          ? Object.values(source.keys)
          : source.type === "contact_matrix"
            ? [...source.pins, ...Object.values(source.keys).flat()]
            : [source.gpio],
      ),
    );
  const safe = new Set(boardSafePins(board));
  return new Set(
    hardware.inputs.flatMap((source) =>
      (source.type === "direct"
        ? Object.values(source.keys)
        : source.type === "contact_matrix"
          ? [...source.pins, ...Object.values(source.keys).flat()]
          : [source.gpio]
      ).filter((pin) => !safe.has(pin)),
    ),
  );
}

function hasInvalidContactPair(hardware: HardwareProfile) {
  return hardware.inputs.some(
    (source) =>
      source.type === "contact_matrix" &&
      Object.values(source.keys).some(
        ([left, right]) =>
          left === right ||
          !source.pins.includes(left) ||
          !source.pins.includes(right),
      ),
  );
}

function hasInvalidFeatureSwitch(
  hardware: HardwareProfile,
  buttons: readonly { id: string }[],
) {
  const knownButtons = new Set(buttons.map((button) => button.id));
  return hardware.inputs.some(
    (source) =>
      source.type === "feature_switch" &&
      (!source.name.trim() ||
        source.buttons.some((button) => !knownButtons.has(button))),
  );
}

function hasUnknownHardwareButton(
  hardware: HardwareProfile,
  buttons: readonly { id: string }[],
) {
  const knownButtons = new Set(buttons.map((button) => button.id));
  return hardware.inputs.some((source) => {
    const buttonIds =
      source.type === "feature_switch"
        ? source.buttons
        : Object.keys(source.keys);
    return buttonIds.some((buttonId) => !knownButtons.has(buttonId));
  });
}

function hasInvalidOled(
  hardware: HardwareProfile,
  board: BoardProfileSummary | undefined,
) {
  if (hardware.ssd1306 && hardware.sh1106) return true;
  const oled = hardware.sh1106 ?? hardware.ssd1306;
  if (!oled) return false;
  if (!board?.supportsOled) return true;
  const safe = new Set(boardSafePins(board));
  const pins = [oled.sda, oled.scl];
  if (oled.control_panel) {
    const panel = oled.control_panel;
    pins.push(
      panel.confirm,
      panel.encoder_press,
      panel.encoder_a,
      panel.encoder_b,
      panel.back,
    );
  }
  return (
    pins.some((pin) => !safe.has(pin)) || new Set(pins).size !== pins.length
  );
}

export function hardwareProfilesAreValid(
  profiles: readonly HardwareProfile[],
  boardProfiles: readonly BoardProfileSummary[],
  layout?: ModelLayout,
) {
  return profiles.every((profile) => {
    const board = boardProfiles.find(
      ({ id }) => id === profile.board_profile_id,
    );
    return (
      Boolean(board) &&
      invalidBoardPins(profile, boardProfiles).size === 0 &&
      conflictingPins(profile).size === 0 &&
      !hasInvalidContactPair(profile) &&
      !hasInvalidOled(profile, board) &&
      (!layout ||
        (!hasInvalidFeatureSwitch(
          profile,
          layout.groups.flatMap((group) => group.buttons),
        ) &&
          !hasUnknownHardwareButton(
            profile,
            layout.groups.flatMap((group) => group.buttons),
          )))
    );
  });
}
