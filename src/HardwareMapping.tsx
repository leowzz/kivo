import { Cable, Check, ChevronLeft, ChevronRight, CircleAlert, Copy, LayoutGrid, Monitor, Pencil, Plus, Radio, SkipForward, SquareStop, ToggleRight, Trash2, Undo2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { ConfirmDialog } from "./ConfirmDialog";
import { editablePins as selectEditablePins } from "./deviceStatus";
import { t } from "./i18n";
import type {
  BoardProfileSummary,
  DeviceStatus,
  HardwareProfile,
  InputSource,
  Language,
  LearningTarget,
  ModelLayout,
} from "./types";

interface HardwareMappingProps {
  language: Language;
  layout: ModelLayout;
  hardwareProfiles: HardwareProfile[];
  boardProfiles: BoardProfileSummary[];
  devices: DeviceStatus[];
  learning: LearningTarget | null;
  initialHardwareProfileId?: string;
  initialDeviceId?: string | null;
  selectedButtonId: string | null;
  onSelectButton(buttonId: string): void;
  onChange(hardwareProfiles: HardwareProfile[]): void;
  onSelectionChange(hardwareProfileId: string | null, deviceId: string | null): void;
  onBeginLearning(hardwareProfileId: string, deviceId: string, pins: number[]): void;
  onEndLearning(deviceId: string): void;
  onFinishLearning?(deviceId: string): void;
}

interface ButtonBinding {
  description: string;
  pins: number[];
}

interface ButtonBindingStatus {
  bindings: ButtonBinding[];
  mapped: boolean;
  conflict: boolean;
  invalid: boolean;
}

interface LearningHistoryEntry {
  hardware: HardwareProfile;
  buttonId: string | null;
}

function sourceName(language: Language, source: InputSource) {
  return t(language, source.type === "direct"
    ? "hardware.direct"
    : source.type === "contact_matrix"
      ? "hardware.matrix"
      : "hardware.featureSwitch");
}

function uniqueValue(base: string, existing: Set<string>) {
  if (!existing.has(base)) return base;
  let suffix = 2;
  while (existing.has(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}

function uniqueName(base: string, existing: Set<string>) {
  if (!existing.has(base)) return base;
  let suffix = 2;
  while (existing.has(`${base} ${suffix}`)) suffix += 1;
  return `${base} ${suffix}`;
}

function boardSafePins(board: BoardProfileSummary | undefined) {
  if (!board) return [];
  return board.id === "yd-rp2040"
    ? board.safePins.filter((pin) => pin >= 0 && (pin <= 23 || (pin >= 26 && pin <= 29)))
    : board.safePins;
}

function ownedInputPins(hardware: HardwareProfile) {
  return hardware.inputs.flatMap((source) =>
    source.type === "direct"
      ? Object.values(source.keys)
      : source.type === "contact_matrix"
        ? source.pins
        : [source.gpio]
  );
}

function buttonBindings(hardware: HardwareProfile | undefined) {
  const bindings = new Map<string, ButtonBinding[]>();
  if (!hardware) return bindings;
  const add = (buttonId: string, binding: ButtonBinding) => {
    const current = bindings.get(buttonId) ?? [];
    current.push(binding);
    bindings.set(buttonId, current);
  };

  for (const source of hardware.inputs) {
    if (source.type === "direct") {
      for (const [buttonId, gpio] of Object.entries(source.keys)) {
        add(buttonId, { description: `GPIO ${gpio}`, pins: [gpio] });
      }
    } else if (source.type === "contact_matrix") {
      for (const [buttonId, pair] of Object.entries(source.keys)) {
        add(buttonId, {
          description: `${pair[0]} + ${pair[1]}`,
          pins: [...pair],
        });
      }
    } else {
      for (const buttonId of source.buttons) {
        add(buttonId, {
          description: `${source.name} / GPIO ${source.gpio}`,
          pins: [source.gpio],
        });
      }
    }
  }
  return bindings;
}

function bindingFingerprintForHardware(hardware: HardwareProfile | undefined) {
  return new Map(
    [...buttonBindings(hardware)].map(([buttonId, bindings]) => [
      buttonId,
      bindings.map(({ description }) => description).join("|") || "",
    ]),
  );
}

function changedButtonId(
  previous: HardwareProfile,
  current: HardwareProfile,
  buttons: readonly { id: string }[],
) {
  const previousBindings = bindingFingerprintForHardware(previous);
  const currentBindings = bindingFingerprintForHardware(current);
  return buttons.find(({ id }) =>
    (previousBindings.get(id) ?? "") !== (currentBindings.get(id) ?? "")
  )?.id ?? null;
}

function buttonBindingStatuses(
  hardware: HardwareProfile | undefined,
  conflicts: ReadonlySet<number>,
  invalid: ReadonlySet<number>,
) {
  const statuses = new Map<string, ButtonBindingStatus>();
  for (const [buttonId, bindings] of buttonBindings(hardware)) {
    statuses.set(buttonId, {
      bindings,
      mapped: bindings.length > 0,
      conflict: bindings.length > 1 || bindings.some(({ pins }) => pins.some((pin) => conflicts.has(pin))),
      invalid: bindings.some(({ pins }) => pins.some((pin) => invalid.has(pin))),
    });
  }
  return statuses;
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

function invalidBoardPins(hardware: HardwareProfile, boardProfiles: readonly BoardProfileSummary[]) {
  const board = boardProfiles.find(({ id }) => id === hardware.board_profile_id);
  if (!board) return new Set(hardware.inputs.flatMap((source) =>
    source.type === "direct"
      ? Object.values(source.keys)
      : source.type === "contact_matrix"
        ? [...source.pins, ...Object.values(source.keys).flat()]
        : [source.gpio]
  ));
  const safe = new Set(boardSafePins(board));
  return new Set(hardware.inputs.flatMap((source) =>
    (source.type === "direct"
      ? Object.values(source.keys)
      : source.type === "contact_matrix"
        ? [...source.pins, ...Object.values(source.keys).flat()]
        : [source.gpio]
    ).filter((pin) => !safe.has(pin))
  ));
}

function hasInvalidContactPair(hardware: HardwareProfile) {
  return hardware.inputs.some((source) => source.type === "contact_matrix" &&
    Object.values(source.keys).some(([left, right]) =>
      left === right || !source.pins.includes(left) || !source.pins.includes(right)
    )
  );
}

function hasInvalidFeatureSwitch(hardware: HardwareProfile, buttons: readonly { id: string }[]) {
  const knownButtons = new Set(buttons.map((button) => button.id));
  return hardware.inputs.some((source) => source.type === "feature_switch" && (
    !source.name.trim() ||
    source.buttons.some((button) => !knownButtons.has(button))
  ));
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
  return pins.some((pin) => !safe.has(pin)) || new Set(pins).size !== pins.length;
}

export function hardwareProfilesAreValid(
  profiles: readonly HardwareProfile[],
  boardProfiles: readonly BoardProfileSummary[],
  layout?: ModelLayout,
) {
  return profiles.every((profile) => {
    const board = boardProfiles.find(({ id }) => id === profile.board_profile_id);
    return Boolean(board) &&
      invalidBoardPins(profile, boardProfiles).size === 0 &&
      conflictingPins(profile).size === 0 &&
      !hasInvalidContactPair(profile) &&
      !hasInvalidOled(profile, board) &&
      (!layout || !hasInvalidFeatureSwitch(profile, layout.groups.flatMap((group) => group.buttons)));
  });
}

function invalidMessage(language: Language, pins: readonly number[]) {
  return t(language, "hardware.invalidPins", { pins: pins.join(language === "zh-CN" ? "、" : ", ") });
}

function conflictMessage(language: Language, pin: number) {
  return t(language, "hardware.pinConflict", { pin });
}

function pinOptions(
  current: number,
  candidates: readonly number[],
  excluded: ReadonlySet<number>,
) {
  const available = candidates.filter((pin) => !excluded.has(pin));
  return available.includes(current) ? available : [current, ...available];
}

type DisplayComponent = "none" | "ssd1306" | "sh1106";

function selectedDisplayComponent(hardware: HardwareProfile): DisplayComponent {
  if (hardware.sh1106) return "sh1106";
  if (hardware.ssd1306) return "ssd1306";
  return "none";
}

function configuredDisplayPins(hardware: HardwareProfile) {
  const display = hardware.sh1106 ?? hardware.ssd1306;
  if (!display) return [];
  const panel = display.control_panel;
  return [
    display.sda,
    display.scl,
    ...(panel
      ? [panel.confirm, panel.encoder_press, panel.encoder_a, panel.encoder_b, panel.back]
      : []),
  ];
}

function displayComponentCanFit(
  component: DisplayComponent,
  board: BoardProfileSummary | undefined,
  availablePins: readonly number[],
) {
  if (component === "none") return true;
  if (!board?.supportsOled) return false;
  return availablePins.length >= (component === "sh1106" ? 7 : 2);
}

function configureDisplayComponent(
  hardware: HardwareProfile,
  component: DisplayComponent,
  availablePins: readonly number[],
) {
  const next = { ...hardware };
  if (component === "none") {
    delete next.ssd1306;
    delete next.sh1106;
    return next;
  }

  const existing = hardware.sh1106 ?? hardware.ssd1306;
  const keepBus = existing
    && existing.sda !== existing.scl
    && availablePins.includes(existing.sda)
    && availablePins.includes(existing.scl);
  const [sda, scl] = keepBus
    ? [existing.sda, existing.scl]
    : availablePins.slice(0, 2);
  if (sda === undefined || scl === undefined) return hardware;

  if (component === "ssd1306") {
    next.ssd1306 = { sda, scl };
    delete next.sh1106;
    return next;
  }

  const controlCandidates = availablePins.filter((pin) => pin !== sda && pin !== scl);
  const existingControl = existing?.control_panel;
  const existingControlPins = existingControl
    ? [
        existingControl.confirm,
        existingControl.encoder_press,
        existingControl.encoder_a,
        existingControl.encoder_b,
        existingControl.back,
      ]
    : [];
  const keepControl = existingControlPins.length === 5
    && new Set(existingControlPins).size === existingControlPins.length
    && existingControlPins.every((pin) => controlCandidates.includes(pin));
  const [confirm, encoderPress, encoderA, encoderB, back] = keepControl
    ? existingControlPins
    : controlCandidates.slice(0, 5);
  if ([confirm, encoderPress, encoderA, encoderB, back].some((pin) => pin === undefined)) {
    return hardware;
  }
  next.sh1106 = {
    sda,
    scl,
    control_panel: {
      type: "ec11_confirm_back",
      confirm,
      encoder_press: encoderPress,
      encoder_a: encoderA,
      encoder_b: encoderB,
      back,
    },
  };
  delete next.ssd1306;
  return next;
}

export function HardwareMapping({
  language,
  layout,
  hardwareProfiles,
  boardProfiles,
  devices,
  learning,
  initialHardwareProfileId,
  initialDeviceId,
  selectedButtonId,
  onSelectButton,
  onChange,
  onSelectionChange,
  onBeginLearning,
  onEndLearning,
  onFinishLearning,
}: HardwareMappingProps) {
  const initialHardware = hardwareProfiles.find(
    ({ id }) => id === initialHardwareProfileId,
  ) ?? hardwareProfiles[0];
  const [selectedId, setSelectedId] = useState(initialHardware?.id ?? "");
  const [renaming, setRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<HardwareProfile | null>(null);
  const [learningPanelOpen, setLearningPanelOpen] = useState(false);
  const [finishRequested, setFinishRequested] = useState(false);
  const [selectedDeviceId, setSelectedDeviceId] = useState(
    initialDeviceId ?? "",
  );
  const pendingCreatedId = useRef<string | null>(null);
  const appliedNavigationTarget = useRef<string | null>(
    initialHardwareProfileId
      ? JSON.stringify([initialHardwareProfileId, initialDeviceId ?? null])
      : null,
  );
  const pendingNavigationSelection = useRef<{
    hardwareProfileId: string;
    deviceId: string;
  } | null>(null);
  const renameOrigin = useRef<{ id: string; profile: string } | null>(null);
  const learningProgressRef = useRef<{
    key: string;
    bindings: Map<string, string>;
  } | null>(null);
  const learningHistoryRef = useRef<{
    key: string;
    current: HardwareProfile;
    entries: LearningHistoryEntry[];
    pendingRestore: string | null;
  } | null>(null);
  const [, bumpLearningHistory] = useState(0);
  const selectedHardware = hardwareProfiles.find(({ id }) => id === selectedId) ?? hardwareProfiles[0];
  const selectedDeviceCandidate = devices.find(({ deviceId }) => deviceId === selectedDeviceId);
  const selectedDeviceLearning = selectedDeviceCandidate?.learning ?? null;
  const activeLearning = learning ?? selectedDeviceLearning;
  const learningHardware = activeLearning
    ? hardwareProfiles.find(({ id }) => id === activeLearning.hardwareProfileId)
    : undefined;
  // Never fall back to the manually selected profile during a live session.
  const hardware = activeLearning ? learningHardware : selectedHardware;
  const board = boardProfiles.find(({ id }) => id === hardware?.board_profile_id);
  const oled = hardware?.sh1106 ?? hardware?.ssd1306;
  const displayComponent = hardware ? selectedDisplayComponent(hardware) : "none";
  const controlPanel = hardware?.sh1106?.control_panel;
  const compatibleDevices = devices.filter((device) =>
    device.connection === "online" &&
    device.mode === "runtime" &&
    device.identity === "valid" &&
    device.boardProfileId === hardware?.board_profile_id
  );
  const selectedDevice = activeLearning
    ? devices.find(({ deviceId }) => deviceId === activeLearning.deviceId)
    : compatibleDevices.find(({ deviceId }) => deviceId === selectedDeviceId);
  const editablePins = selectEditablePins(boardSafePins(board), selectedDevice?.capabilities ?? null);
  const invalid = useMemo(
    () => hardware ? invalidBoardPins(hardware, boardProfiles) : new Set<number>(),
    [boardProfiles, hardware],
  );
  const conflicts = useMemo(
    () => hardware ? conflictingPins(hardware) : new Set<number>(),
    [hardware],
  );
  const inputPins = useMemo(
    () => new Set(hardware ? ownedInputPins(hardware) : []),
    [hardware],
  );
  const oledPins = useMemo(
    () => new Set(hardware ? configuredDisplayPins(hardware) : []),
    [hardware],
  );
  const oledAvailablePins = editablePins.filter((pin) => !inputPins.has(pin));
  const learningPins = editablePins.filter((pin) => !oledPins.has(pin));
  const buttons = layout.groups.flatMap((group) => group.buttons);
  const bindingStatuses = useMemo(
    () => buttonBindingStatuses(hardware, conflicts, invalid),
    [conflicts, hardware, invalid],
  );
  const mappedButtonIds = useMemo(
    () => new Set([...bindingStatuses].filter(([, status]) => status.mapped).map(([buttonId]) => buttonId)),
    [bindingStatuses],
  );
  const mappedCount = buttons.reduce(
    (count, button) => count + (mappedButtonIds.has(button.id) ? 1 : 0),
    0,
  );
  const targetButton = buttons.find(({ id }) => id === selectedButtonId)
    ?? buttons.find(({ id }) => !mappedButtonIds.has(id))
    ?? buttons[0]
    ?? null;
  const targetStatus = targetButton ? bindingStatuses.get(targetButton.id) : undefined;
  const learningLocked = Boolean(activeLearning);
  const activeLearningKey = activeLearning
    ? `${activeLearning.deviceId}:${activeLearning.deviceProfileId}:${activeLearning.hardwareProfileId}:${activeLearning.editingRevision}:${activeLearning.firmwareRevision}`
    : null;
  const bindingFingerprint = useMemo(
    () => new Map(
      [...bindingStatuses].map(([buttonId, status]) => [
        buttonId,
        status.bindings.map(({ description }) => description).join("|") || "",
      ]),
    ),
    [bindingStatuses],
  );

  useEffect(() => {
    if (!initialHardwareProfileId) {
      appliedNavigationTarget.current = null;
      return;
    }
    const navigationKey = JSON.stringify([
      initialHardwareProfileId,
      initialDeviceId ?? null,
    ]);
    if (appliedNavigationTarget.current === navigationKey) return;
    const targetHardware = hardwareProfiles.find(
      ({ id }) => id === initialHardwareProfileId,
    );
    if (!targetHardware) return;
    const targetDeviceId = initialDeviceId && devices.some((device) =>
      device.deviceId === initialDeviceId &&
      device.connection === "online" &&
      device.mode === "runtime" &&
      device.identity === "valid" &&
      device.boardProfileId === targetHardware.board_profile_id
    )
      ? initialDeviceId
      : "";
    appliedNavigationTarget.current = navigationKey;
    if (
      selectedId === targetHardware.id &&
      selectedDeviceId === targetDeviceId
    ) {
      return;
    }
    pendingNavigationSelection.current = {
      hardwareProfileId: targetHardware.id,
      deviceId: targetDeviceId,
    };
    setSelectedId(targetHardware.id);
    setSelectedDeviceId(targetDeviceId);
    setRenaming(false);
    setRenameValue("");
  }, [
    devices,
    hardwareProfiles,
    initialDeviceId,
    initialHardwareProfileId,
    selectedDeviceId,
    selectedId,
  ]);

  useEffect(() => {
    if (hardwareProfiles.some(({ id }) => id === selectedId)) {
      pendingCreatedId.current = null;
      return;
    }
    if (pendingCreatedId.current === selectedId) return;
    setSelectedId(hardwareProfiles[0]?.id ?? "");
    setSelectedDeviceId("");
    setRenaming(false);
    setRenameValue("");
  }, [hardwareProfiles, selectedId]);

  useEffect(() => {
    if (!renaming || !renameOrigin.current) return;
    const current = hardwareProfiles.find(({ id }) => id === renameOrigin.current?.id);
    if (selectedId !== renameOrigin.current.id || JSON.stringify(current) !== renameOrigin.current.profile) {
      renameOrigin.current = null;
      setRenaming(false);
      setRenameValue("");
    }
  }, [hardwareProfiles, renaming, selectedId]);

  useEffect(() => {
    const pending = pendingNavigationSelection.current;
    if (pending && selectedDeviceId !== pending.deviceId) return;
    if (!compatibleDevices.some(({ deviceId }) => deviceId === selectedDeviceId)) {
      setSelectedDeviceId("");
    }
  }, [compatibleDevices, selectedDeviceId]);

  useEffect(() => {
    const pending = pendingNavigationSelection.current;
    if (pending) {
      if (
        hardware?.id !== pending.hardwareProfileId ||
        (selectedDevice?.deviceId ?? "") !== pending.deviceId
      ) {
        return;
      }
      pendingNavigationSelection.current = null;
    }
    onSelectionChange(hardware?.id ?? null, selectedDevice?.deviceId ?? null);
  }, [hardware?.id, onSelectionChange, selectedDevice?.deviceId]);

  useEffect(() => {
    if (!activeLearning) return;
    if (selectedId !== activeLearning.hardwareProfileId) {
      setSelectedId(activeLearning.hardwareProfileId);
      setRenaming(false);
      setRenameValue("");
    }
    if (selectedDeviceId !== activeLearning.deviceId) {
      setSelectedDeviceId(activeLearning.deviceId);
    }
  }, [activeLearning?.deviceId, activeLearning?.hardwareProfileId, selectedDeviceId, selectedId]);

  useEffect(() => {
    if (!activeLearning || (selectedButtonId && buttons.some(({ id }) => id === selectedButtonId))) return;
    const nextButton = buttons.find(({ id }) => !mappedButtonIds.has(id)) ?? buttons[0];
    if (nextButton) onSelectButton(nextButton.id);
  }, [activeLearning, buttons, mappedButtonIds, onSelectButton, selectedButtonId]);

  useEffect(() => {
    if (!activeLearningKey) {
      learningProgressRef.current = null;
      return;
    }
    setLearningPanelOpen(true);
    if (learningProgressRef.current?.key !== activeLearningKey) {
      setFinishRequested(false);
      learningProgressRef.current = {
        key: activeLearningKey,
        bindings: new Map(bindingFingerprint),
      };
    }
  }, [activeLearningKey, bindingFingerprint]);

  useEffect(() => {
    if (!activeLearningKey || !activeLearning || !hardware) {
      if (learningHistoryRef.current) {
        learningHistoryRef.current = null;
        bumpLearningHistory((version) => version + 1);
      }
      return;
    }
    const current = structuredClone(hardware);
    const previous = learningHistoryRef.current;
    if (!previous || previous.key !== activeLearningKey) {
      learningHistoryRef.current = {
        key: activeLearningKey,
        current,
        entries: [],
        pendingRestore: null,
      };
      bumpLearningHistory((version) => version + 1);
      return;
    }
    const currentSerialized = JSON.stringify(hardware);
    if (previous.pendingRestore) {
      if (previous.pendingRestore === currentSerialized) previous.pendingRestore = null;
      return;
    }
    if (JSON.stringify(previous.current) === currentSerialized) return;
    previous.entries.push({
      hardware: structuredClone(previous.current),
      buttonId: changedButtonId(previous.current, hardware, buttons),
    });
    previous.current = current;
    bumpLearningHistory((version) => version + 1);
  }, [activeLearning, activeLearningKey, buttons, hardware]);

  useEffect(() => {
    if (!activeLearningKey || !activeLearning) {
      learningProgressRef.current = null;
      return;
    }
    const previous = learningProgressRef.current;
    if (!previous || previous.key !== activeLearningKey) {
      learningProgressRef.current = {
        key: activeLearningKey,
        bindings: new Map(bindingFingerprint),
      };
      return;
    }
    const selectedBefore = selectedButtonId ? previous.bindings.get(selectedButtonId) ?? "" : "";
    const selectedAfter = selectedButtonId ? bindingFingerprint.get(selectedButtonId) ?? "" : "";
    learningProgressRef.current = {
      key: activeLearningKey,
      bindings: new Map(bindingFingerprint),
    };
    if (!selectedButtonId || !selectedAfter || selectedAfter === selectedBefore) return;
    const currentIndex = buttons.findIndex(({ id }) => id === selectedButtonId);
    const nextButton = buttons.slice(Math.max(0, currentIndex + 1)).find(({ id }) => !mappedButtonIds.has(id))
      ?? buttons.find(({ id }) => !mappedButtonIds.has(id));
    if (nextButton && nextButton.id !== selectedButtonId) onSelectButton(nextButton.id);
  }, [activeLearning, activeLearningKey, bindingFingerprint, buttons, mappedButtonIds, onSelectButton, selectedButtonId]);

  const replaceHardware = (next: HardwareProfile) => {
    if (learningLocked) return;
    onChange(hardwareProfiles.map((item) => item.id === next.id ? next : item));
  };

  const updateSource = (index: number, source: InputSource) => {
    if (!hardware) return;
    replaceHardware({
      ...hardware,
      inputs: hardware.inputs.map((item, itemIndex) => itemIndex === index ? source : item),
    });
  };

  const addHardware = () => {
    if (learningLocked) return;
    const selectedBoard = boardProfiles[0];
    if (!selectedBoard) return;
    const ids = new Set(hardwareProfiles.map(({ id }) => id));
    const id = uniqueValue(`${selectedBoard.id}-hardware`, ids);
    const names = new Set(hardwareProfiles.map(({ name }) => name));
    const name = uniqueName(
      `${selectedBoard.displayName} ${t(language, "hardware.profile")}`,
      names,
    );
    const next: HardwareProfile = {
      id,
      name,
      board_profile_id: selectedBoard.id,
      debounce_ms: 30,
      inputs: [],
    };
    pendingCreatedId.current = id;
    setSelectedId(id);
    onChange([...hardwareProfiles, next]);
  };

  const duplicateHardware = () => {
    if (!hardware || learningLocked) return;
    const ids = new Set(hardwareProfiles.map(({ id }) => id));
    const id = uniqueValue(`${hardware.id}-copy`, ids);
    const baseName = `${hardware.name} ${t(language, "hardware.copySuffix")}`;
    const names = new Set(hardwareProfiles.map(({ name }) => name));
    const name = uniqueName(baseName, names);
    const next = structuredClone({ ...hardware, id, name });
    pendingCreatedId.current = id;
    setSelectedId(id);
    onChange([...hardwareProfiles, next]);
  };

  const commitRename = () => {
    const name = renameValue.trim();
    if (hardware && name && !learningLocked) replaceHardware({ ...hardware, name });
    setRenaming(false);
  };

  const targetIndex = targetButton ? buttons.findIndex(({ id }) => id === targetButton.id) : -1;
  const targetBindingDetail = targetStatus?.bindings.map(({ description }) => description).join(" / ") ?? "";
  const canUndoLearning = Boolean(
    activeLearning && !finishRequested && learningHistoryRef.current?.entries.length,
  );
  const previousTarget = targetIndex > 0 ? buttons[targetIndex - 1] : undefined;
  const nextTarget = targetIndex >= 0 && targetIndex < buttons.length - 1
    ? buttons[targetIndex + 1]
    : undefined;
  const skipTarget = buttons.slice(Math.max(0, targetIndex + 1)).find(({ id }) => !mappedButtonIds.has(id))
    ?? buttons.find(({ id }) => !mappedButtonIds.has(id))
    ?? nextTarget
    ?? previousTarget;
  const undoLearning = () => {
    if (!activeLearning || finishRequested || !hardware) return;
    const history = learningHistoryRef.current;
    const entry = history?.entries.pop();
    if (!history || !entry) return;
    const restored = structuredClone(entry.hardware);
    history.current = structuredClone(restored);
    history.pendingRestore = JSON.stringify(restored);
    bumpLearningHistory((version) => version + 1);
    if (entry.buttonId) onSelectButton(entry.buttonId);
    onChange(hardwareProfiles.map((item) => item.id === restored.id ? restored : item));
  };
  const finishLearning = () => {
    if (!activeLearning || finishRequested) return;
    setFinishRequested(true);
    onEndLearning(activeLearning.deviceId);
    onFinishLearning?.(activeLearning.deviceId);
  };

  return (
    <section className="hardware-view" aria-labelledby="hardware-title">
      <div className="hardware-toolbar">
        <label>
          <span>{t(language, "hardware.profile")}</span>
          <select
            aria-label={t(language, "hardware.profile")}
            value={hardware?.id ?? ""}
            disabled={hardwareProfiles.length === 0 || learningLocked}
            onChange={(event) => {
              setSelectedId(event.target.value);
              setRenaming(false);
            }}
          >
            {hardwareProfiles.length === 0 && <option value="">{t(language, "hardware.noProfiles")}</option>}
            {hardwareProfiles.map((profile) => <option value={profile.id} key={profile.id}>{profile.name}</option>)}
          </select>
        </label>
        <div className="hardware-profile-actions">
          <button className="icon-button" type="button" aria-label={t(language, "hardware.addProfile")} title={t(language, "hardware.addProfile")} disabled={boardProfiles.length === 0 || learningLocked} onClick={addHardware}>
            <Plus size={16} />
          </button>
          <button className="icon-button" type="button" aria-label={t(language, "hardware.duplicateProfile")} title={t(language, "hardware.duplicateProfile")} disabled={!hardware || learningLocked} onClick={duplicateHardware}>
            <Copy size={16} />
          </button>
          <button className="icon-button" type="button" aria-label={t(language, "hardware.renameProfile")} title={t(language, "hardware.renameProfile")} disabled={!hardware || learningLocked} onClick={() => {
            if (!hardware) return;
            renameOrigin.current = { id: hardware.id, profile: JSON.stringify(hardware) };
            setRenameValue(hardware.name);
            setRenaming(true);
          }}>
            <Pencil size={16} />
          </button>
          <button className="icon-button is-danger" type="button" aria-label={t(language, "hardware.deleteProfile")} title={t(language, "hardware.deleteProfile")} disabled={!hardware || learningLocked} onClick={() => setDeleteTarget(hardware ?? null)}>
            <Trash2 size={16} />
          </button>
        </div>
        {renaming && hardware && (
          <input
            className="hardware-rename-input"
            aria-label={t(language, "hardware.profileName")}
            autoFocus
            value={renameValue}
            onChange={(event) => setRenameValue(event.target.value)}
            onBlur={() => setRenaming(false)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                commitRename();
              }
              if (event.key === "Escape") setRenaming(false);
            }}
          />
        )}
      </div>

      {!hardware ? (
        <div className="empty-workspace">{t(language, "hardware.noProfiles")}</div>
      ) : (
        <>
          <fieldset className="hardware-editing-fieldset" disabled={learningLocked}>
          <div className="content-heading hardware-heading">
            <div>
              <span>{layout.name}</span>
              <h2 id="hardware-title">{t(language, "hardware.title")}</h2>
            </div>
            <label className="board-profile-field">
              <span>{t(language, "hardware.boardProfile")}</span>
              <select value={hardware.board_profile_id} disabled={learningLocked} onChange={(event) => {
                setSelectedDeviceId("");
                replaceHardware({ ...hardware, board_profile_id: event.target.value });
              }}>
                {boardProfiles.map((profile) => <option value={profile.id} key={profile.id}>{profile.displayName}</option>)}
              </select>
            </label>
            <label className="debounce-field">
              <span>{t(language, "hardware.debounce")}</span>
              <input
                aria-label={t(language, "hardware.debounce")}
                type="number"
                min="1"
                max="1000"
                disabled={learningLocked}
                value={hardware.debounce_ms}
                onChange={(event) => replaceHardware({ ...hardware, debounce_ms: Number(event.target.value) })}
              />
              <small>ms</small>
            </label>
          </div>

          <section className="oled-editor" aria-label={t(language, "hardware.displayComponent")}>
            <div className="oled-summary">
              <Monitor size={16} />
              <label className="oled-type-field">
                <span>{t(language, "hardware.displayComponent")}</span>
                <select
                  aria-label={t(language, "hardware.displayComponent")}
                  value={displayComponent}
                  onChange={(event) => {
                    replaceHardware(configureDisplayComponent(
                      hardware,
                      event.target.value as DisplayComponent,
                      oledAvailablePins,
                    ));
                  }}
                >
                  <option value="none">{t(language, "hardware.displayNone")}</option>
                  <option
                    value="ssd1306"
                    disabled={displayComponent !== "ssd1306" && !displayComponentCanFit("ssd1306", board, oledAvailablePins)}
                  >
                    {t(language, "hardware.oled")}
                  </option>
                  <option
                    value="sh1106"
                    disabled={displayComponent !== "sh1106" && !displayComponentCanFit("sh1106", board, oledAvailablePins)}
                  >
                    {t(language, "hardware.sh1106Oled")}
                  </option>
                </select>
              </label>
              {oled ? <code>{t(language, displayComponent === "sh1106" ? "hardware.sh1106Format" : "hardware.oledFormat")}</code> : null}
            </div>
            {oled && ([
              ["sda", t(language, "hardware.oledSda")],
              ["scl", t(language, "hardware.oledScl")],
            ] as const).map(([field, label]) => {
              const current = oled[field];
              const excluded = new Set([...inputPins, ...configuredDisplayPins(hardware)]);
              excluded.delete(current);
              const unsafe = !new Set(boardSafePins(board)).has(current);
              const unsupported = !board?.supportsOled;
              const conflict = conflicts.has(current);
              const currentInvalid = unsupported || unsafe || conflict;
              return (
                <label className="oled-pin-field" key={field}>
                  <span>{label}</span>
                  <select
                    aria-label={label}
                    aria-invalid={currentInvalid}
                    value={current}
                    onChange={(event) => {
                      const next = { ...hardware };
                      if (next.sh1106) {
                        next.sh1106 = { ...next.sh1106, [field]: Number(event.target.value) };
                      } else if (next.ssd1306) {
                        next.ssd1306 = { ...next.ssd1306, [field]: Number(event.target.value) };
                      } else {
                        return;
                      }
                      replaceHardware(next);
                    }}
                  >
                    {pinOptions(current, editablePins, excluded).map((pin) => (
                      <option value={pin} key={pin}>{pin}</option>
                    ))}
                  </select>
                  {currentInvalid && (
                    <small className="field-error">
                      {unsupported
                        ? t(language, "hardware.oledUnsupported")
                        : conflict
                          ? conflictMessage(language, current)
                          : invalidMessage(language, [current])}
                    </small>
                  )}
                </label>
              );
            })}
            {controlPanel && ([
              ["confirm", t(language, "hardware.oledConfirm")],
              ["encoder_press", t(language, "hardware.oledEncoderPress")],
              ["encoder_a", t(language, "hardware.oledEncoderA")],
              ["encoder_b", t(language, "hardware.oledEncoderB")],
              ["back", t(language, "hardware.oledBack")],
            ] as const).map(([field, label]) => {
              const current = controlPanel[field];
              const excluded = new Set([...inputPins, ...configuredDisplayPins(hardware)]);
              excluded.delete(current);
              const unsafe = !new Set(boardSafePins(board)).has(current);
              const unsupported = !board?.supportsOled;
              const conflict = conflicts.has(current);
              const currentInvalid = unsupported || unsafe || conflict;
              return (
                <label className="oled-pin-field" key={field}>
                  <span>{label}</span>
                  <select
                    aria-label={label}
                    aria-invalid={currentInvalid}
                    value={current}
                    onChange={(event) => {
                      if (!hardware.sh1106?.control_panel) return;
                      replaceHardware({
                        ...hardware,
                        sh1106: {
                          ...hardware.sh1106,
                          control_panel: {
                            ...hardware.sh1106.control_panel,
                            [field]: Number(event.target.value),
                          },
                        },
                      });
                    }}
                  >
                    {pinOptions(current, editablePins, excluded).map((pin) => (
                      <option value={pin} key={pin}>{pin}</option>
                    ))}
                  </select>
                  {currentInvalid && (
                    <small className="field-error">
                      {unsupported
                        ? t(language, "hardware.oledUnsupported")
                        : conflict
                          ? conflictMessage(language, current)
                          : invalidMessage(language, [current])}
                    </small>
                  )}
                </label>
              );
            })}
          </section>

          <div className="source-list">
            {hardware.inputs.map((source, sourceIndex) => (
              <section className="source-editor" key={`${source.type}-${source.id}`}>
                <div className="source-heading">
                  <div>
                    {source.type === "direct"
                      ? <Cable size={14} />
                      : source.type === "contact_matrix"
                        ? <LayoutGrid size={14} />
                        : <ToggleRight size={14} />}
                    <strong>{sourceName(language, source)}</strong>
                    <code>{source.id}</code>
                  </div>
                  <button className="icon-button is-danger" type="button" aria-label={t(language, "hardware.removeSource")} title={t(language, "hardware.removeSource")} onClick={() => replaceHardware({
                    ...hardware,
                    inputs: hardware.inputs.filter((_, index) => index !== sourceIndex),
                  })}>
                    <Trash2 size={16} />
                  </button>
                </div>

                {source.type === "feature_switch" ? (
                  <div className="feature-switch-editor">
                    <div className="feature-switch-fields">
                      <label className="field-stack compact-field">
                        <span>{t(language, "hardware.switchName")}</span>
                        <input value={source.name} onChange={(event) => updateSource(sourceIndex, { ...source, name: event.target.value })} />
                      </label>
                      <label className="field-stack compact-field">
                        <span>{t(language, "hardware.switchGpio")}</span>
                        <select
                          aria-label={t(language, "hardware.switchGpio")}
                          aria-invalid={invalid.has(source.gpio) || conflicts.has(source.gpio)}
                          value={source.gpio}
                          onChange={(event) => updateSource(sourceIndex, { ...source, gpio: Number(event.target.value) })}
                        >
                          {pinOptions(source.gpio, editablePins.filter((pin) => !oledPins.has(pin)), new Set([...inputPins].filter((pin) => pin !== source.gpio))).map((pin) => <option value={pin} key={pin}>{pin}</option>)}
                        </select>
                        {(invalid.has(source.gpio) || conflicts.has(source.gpio)) && <small className="field-error">{conflicts.has(source.gpio) ? conflictMessage(language, source.gpio) : invalidMessage(language, [source.gpio])}</small>}
                      </label>
                    </div>
                    <fieldset className="feature-switch-buttons">
                      <legend>{t(language, "hardware.switchButtons")}</legend>
                      <div className="feature-switch-button-grid">
                        {buttons.map((button) => (
                          <label key={button.id}>
                            <input
                              type="checkbox"
                              checked={source.buttons.includes(button.id)}
                              onChange={(event) => updateSource(sourceIndex, {
                                ...source,
                                buttons: event.target.checked
                                  ? [...new Set([...source.buttons, button.id])]
                                  : source.buttons.filter((id) => id !== button.id),
                              })}
                            />
                            <span>{button.label}</span>
                          </label>
                        ))}
                      </div>
                    </fieldset>
                  </div>
                ) : <>
                {source.type === "contact_matrix" && (() => {
                  const invalidSourcePins = source.pins.filter((pin) => invalid.has(pin));
                  const conflictingSourcePins = [...new Set(
                    source.pins.filter((pin) => conflicts.has(pin)),
                  )];
                  return (
                    <label className="field-stack compact-field">
                      <span>{t(language, "hardware.matrixPins")}</span>
                      <input
                        aria-invalid={invalidSourcePins.length > 0 || conflictingSourcePins.length > 0}
                        value={source.pins.join(", ")}
                        onChange={(event) => {
                          const pins = [...new Set(event.target.value.split(",")
                            .map((pin) => Number(pin.trim()))
                            .filter((pin) => Number.isInteger(pin) && pin >= 0 && pin <= 255))]
                            .sort((left, right) => left - right);
                          updateSource(sourceIndex, {
                            ...source,
                            pins,
                            keys: Object.fromEntries(Object.entries(source.keys).filter(([, pair]) =>
                              pins.includes(pair[0]) && pins.includes(pair[1])
                            )),
                          });
                        }}
                      />
                      {invalidSourcePins.length > 0 && <small className="field-error">{invalidMessage(language, invalidSourcePins)}</small>}
                      {conflictingSourcePins.map((pin) => (
                        <small className="field-error" key={pin}>{conflictMessage(language, pin)}</small>
                      ))}
                    </label>
                  );
                })()}

                <div className="mapping-table">
                  <div className="mapping-head">
                    <span>{t(language, "hardware.key")}</span>
                    <span>{source.type === "direct" ? "GPIO" : t(language, "hardware.contacts")}</span>
                  </div>
                  {buttons.map((button) => (
                    <div className={selectedButtonId === button.id ? "mapping-row is-selected" : "mapping-row"} key={button.id}>
                      <button type="button" onClick={() => onSelectButton(button.id)}>{button.label}</button>
                      {source.type === "direct" ? (() => {
                        const current = source.keys[button.id];
                        const currentInvalid = current !== undefined &&
                          (invalid.has(current) || conflicts.has(current));
                        const availableInputPins = editablePins.filter((pin) => !oledPins.has(pin));
                        const currentStale = current !== undefined && !availableInputPins.includes(current);
                        const options = currentStale ? [current, ...availableInputPins] : availableInputPins;
                        return (
                          <div className="mapping-control">
                            <select
                              aria-label={`${button.label} GPIO`}
                              aria-invalid={currentInvalid}
                              value={current ?? ""}
                              onChange={(event) => {
                                const keys = { ...source.keys };
                                if (event.target.value === "") delete keys[button.id];
                                else keys[button.id] = Number(event.target.value);
                                updateSource(sourceIndex, { ...source, keys });
                              }}
                            >
                              <option value="">-</option>
                              {options.map((pin) => <option value={pin} key={pin}>{pin}</option>)}
                            </select>
                            {currentInvalid && (
                              <small className="field-error">
                                {conflicts.has(current)
                                  ? conflictMessage(language, current)
                                  : invalidMessage(language, [current])}
                              </small>
                            )}
                          </div>
                        );
                      })() : (
                        <div className="contact-inputs">
                          {[0, 1].map((side) => {
                            const current = source.keys[button.id]?.[side];
                            const currentInvalid = current !== undefined &&
                              (invalid.has(current) || conflicts.has(current) || !source.pins.includes(current));
                            const availableSourcePins = source.pins.filter((pin) => !oledPins.has(pin));
                            const options = current !== undefined && !availableSourcePins.includes(current)
                              ? [current, ...availableSourcePins]
                              : availableSourcePins;
                            return (
                              <select
                                aria-label={`${button.label} ${side === 0 ? "A" : "B"}`}
                                aria-invalid={currentInvalid}
                                value={current ?? ""}
                                key={side}
                                onChange={(event) => {
                                  const keys = { ...source.keys };
                                  const currentPair = source.keys[button.id];
                                  if (event.target.value === "") delete keys[button.id];
                                  else {
                                    const selected = Number(event.target.value);
                                    const other = currentPair?.[side === 0 ? 1 : 0] ?? availableSourcePins.find((pin) => pin !== selected);
                                    if (other !== undefined && other !== selected) {
                                      keys[button.id] = selected < other ? [selected, other] : [other, selected];
                                    }
                                  }
                                  updateSource(sourceIndex, { ...source, keys });
                                }}
                              >
                                <option value="">-</option>
                                {options.map((pin) => <option value={pin} key={pin}>{pin}</option>)}
                              </select>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  ))}
                </div>
                </>}
              </section>
            ))}
          </div>

          <div className="source-actions">
            <button type="button" disabled={learningLocked} onClick={() => replaceHardware({
              ...hardware,
              inputs: [...hardware.inputs, { type: "direct", id: `direct-${hardware.inputs.length + 1}`, keys: {} }],
            })}><Plus size={16} />{t(language, "hardware.addDirect")}</button>
            <button type="button" disabled={learningLocked} onClick={() => replaceHardware({
              ...hardware,
              inputs: [...hardware.inputs, { type: "contact_matrix", id: `matrix-${hardware.inputs.length + 1}`, pins: [], keys: {} }],
            })}><Plus size={16} />{t(language, "hardware.addMatrix")}</button>
            <button type="button" disabled={learningLocked} onClick={() => replaceHardware({
              ...hardware,
              inputs: [...hardware.inputs, {
                type: "feature_switch",
                id: `switch-${hardware.inputs.length + 1}`,
                name: t(language, "hardware.switchDefaultName"),
                gpio: editablePins.find((pin) => !inputPins.has(pin) && !oledPins.has(pin)) ?? 0,
                buttons: [],
              }],
            })}><Plus size={16} />{t(language, "hardware.addSwitch")}</button>
          </div>
          </fieldset>

          <details
            className={activeLearning ? "learning-panel is-learning-session" : "learning-panel"}
            open={learningPanelOpen || Boolean(activeLearning)}
            onToggle={(event) => {
              if (!activeLearning) setLearningPanelOpen(event.currentTarget.open);
            }}
          >
            <summary>
              <Radio size={15} />
              <span>{t(language, "hardware.advanced")}</span>
              <small>{activeLearning ? t(language, "hardware.learningActive") : t(language, "hardware.advancedHint")}</small>
            </summary>
            {activeLearning && (
              <section className="learning-session" aria-label={t(language, "hardware.learningSession")}>
                <div className="learning-session-heading">
                  <div>
                    <span className="learning-session-kicker"><Radio size={14} />{t(language, "hardware.learningActive")}</span>
                    <h3>{t(language, "hardware.learningSession")}</h3>
                  </div>
                  <div className="learning-progress" role="status" aria-label={t(language, "hardware.learningProgress", { mapped: mappedCount, total: buttons.length })}>
                    <strong>{mappedCount} / {buttons.length}</strong>
                    <progress max={Math.max(buttons.length, 1)} value={mappedCount} />
                    <span>{t(language, "hardware.learningProgress", { mapped: mappedCount, total: buttons.length })}</span>
                  </div>
                </div>
                <div className="learning-target" role="status" aria-live="polite">
                  <span>{t(language, "hardware.learningTarget")}</span>
                  <strong>{targetButton?.label ?? t(language, "hardware.learningUnmapped")}</strong>
                  {targetStatus?.conflict ? (
                    <small className="learning-status is-conflict">
                      <CircleAlert size={14} />
                      <span>{t(language, "hardware.learningConflict")}</span>
                      {targetBindingDetail && <span>{targetBindingDetail}</span>}
                    </small>
                  ) : targetStatus?.invalid ? (
                    <small className="learning-status is-conflict"><CircleAlert size={14} />{t(language, "hardware.learningInvalid")}</small>
                  ) : targetStatus?.mapped ? (
                    <small className="learning-status is-mapped"><Check size={14} />{t(language, "hardware.learningMapped", { binding: targetStatus.bindings[0]?.description ?? "-" })}</small>
                  ) : (
                    <small className="learning-status is-pending">{t(language, "hardware.learningUnmapped")}</small>
                  )}
                </div>
                <div className="learning-key-list" aria-label={t(language, "hardware.learningTarget")}>
                  {buttons.map((button) => {
                    const status = bindingStatuses.get(button.id);
                    const isTarget = targetButton?.id === button.id;
                    const className = [
                      "learning-key",
                      isTarget ? "is-target" : "",
                      status?.mapped ? "is-mapped" : "is-pending",
                      status?.conflict || status?.invalid ? "is-conflict" : "",
                    ].filter(Boolean).join(" ");
                    return (
                      <button
                        className={className}
                        type="button"
                        key={button.id}
                        aria-current={isTarget ? "step" : undefined}
                        onClick={() => onSelectButton(button.id)}
                      >
                        <span className="learning-key-marker">
                          {status?.conflict || status?.invalid ? <CircleAlert size={14} /> : status?.mapped ? <Check size={14} /> : null}
                        </span>
                        <span>{button.label}</span>
                        <small>
                          {status?.conflict ? (
                            <>
                              <span>{t(language, "hardware.learningConflict")}</span>
                              {status.bindings.map(({ description }, index) => <span key={`${description}:${index}`}>{description}</span>)}
                            </>
                          ) : status?.invalid ? t(language, "hardware.learningInvalid") : status?.mapped ? t(language, "hardware.learningMapped", { binding: status.bindings[0]?.description ?? "-" }) : t(language, "hardware.learningUnmapped")}
                        </small>
                      </button>
                    );
                  })}
                </div>
                <div className="learning-session-actions">
                  <button
                    className="icon-button"
                    type="button"
                    aria-label={t(language, "common.undo")}
                    title={t(language, "common.undo")}
                    disabled={!canUndoLearning}
                    onClick={undoLearning}
                  >
                    <Undo2 size={16} />
                  </button>
                  <button
                    className="icon-button"
                    type="button"
                    aria-label={t(language, "hardware.learningPrevious")}
                    title={t(language, "hardware.learningPrevious")}
                    disabled={!previousTarget}
                    onClick={() => previousTarget && onSelectButton(previousTarget.id)}
                  >
                    <ChevronLeft size={16} />
                  </button>
                  <button
                    className="icon-button"
                    type="button"
                    aria-label={t(language, "hardware.learningNext")}
                    title={t(language, "hardware.learningNext")}
                    disabled={!nextTarget}
                    onClick={() => nextTarget && onSelectButton(nextTarget.id)}
                  >
                    <ChevronRight size={16} />
                  </button>
                  <button
                    className="learning-skip-button"
                    type="button"
                    aria-label={t(language, "hardware.learningSkip")}
                    disabled={!skipTarget || skipTarget.id === targetButton?.id}
                    onClick={() => skipTarget && onSelectButton(skipTarget.id)}
                  >
                    <SkipForward size={15} />{t(language, "hardware.learningSkip")}
                  </button>
                </div>
                {buttons.length > 0 && mappedCount === buttons.length && (
                  <p className="learning-complete-hint" role="status">{t(language, "hardware.learningAllMapped")}</p>
                )}
              </section>
            )}
            <div className={activeLearning ? "learning-controls is-learning" : "learning-controls"}>
              <label className="field-stack compact-field">
                <span>{t(language, "hardware.onlineDevice")}</span>
                <select aria-label={t(language, "hardware.onlineDevice")} value={activeLearning?.deviceId ?? selectedDevice?.deviceId ?? ""} disabled={learningLocked} onChange={(event) => setSelectedDeviceId(event.target.value)}>
                  <option value="">{t(language, "hardware.offlineEditing")}</option>
                  {compatibleDevices.map((device) => <option value={device.deviceId} key={device.deviceId}>{device.name}</option>)}
                </select>
              </label>
              <label className="safety-check">
                <input type="checkbox" required disabled={learningLocked} />
                <span>{t(language, "hardware.safety")}</span>
              </label>
              <fieldset>
                <legend>{t(language, "hardware.candidatePins")}</legend>
                <div className="pin-grid">
                  {learningPins.map((gpio) => (
                    <label key={gpio}><input aria-label={`GPIO ${gpio}`} disabled={learningLocked} type="checkbox" name="learning-pin" value={gpio} />{gpio}</label>
                  ))}
                </div>
              </fieldset>
              {activeLearning ? (
                <button type="button" className="learning-button" disabled={finishRequested} onClick={finishLearning}><SquareStop size={16} />{finishRequested ? t(language, "hardware.learningFinishing") : t(language, "hardware.finishLearning")}</button>
              ) : (
                <button type="button" className="learning-button" disabled={!selectedDevice} onClick={(event) => {
                  const container = event.currentTarget.closest(".learning-controls");
                  const safe = container?.querySelector<HTMLInputElement>(".safety-check input")?.checked;
                  const pins = [...(container?.querySelectorAll<HTMLInputElement>('input[name="learning-pin"]:checked') ?? [])].map((input) => Number(input.value));
                  const targetId = targetButton?.id ?? buttons[0]?.id;
                  if (safe && pins.length && hardware && selectedDevice && targetId) {
                    setFinishRequested(false);
                    if (selectedButtonId !== targetId) onSelectButton(targetId);
                    onBeginLearning(hardware.id, selectedDevice.deviceId, pins);
                  }
                }}><Radio size={16} />{t(language, "hardware.startLearning")}</button>
              )}
              {!activeLearning && finishRequested && (
                <div className="learning-finished" role="status" aria-live="polite">
                  <strong>{t(language, "hardware.learningFinished")}</strong>
                  <span>{t(language, "hardware.learningFinishedHint")}</span>
                </div>
              )}
            </div>
          </details>
        </>
      )}

      {deleteTarget && (
        <ConfirmDialog
          title={t(language, "hardware.deleteProfile")}
          body={t(language, "hardware.deleteProfileBody", { name: deleteTarget.name })}
          confirmLabel={t(language, "common.confirm")}
          cancelLabel={t(language, "common.cancel")}
          danger
          onCancel={() => setDeleteTarget(null)}
          onConfirm={() => {
            if (learningLocked) {
              setDeleteTarget(null);
              return;
            }
            const next = hardwareProfiles.filter(({ id }) => id !== deleteTarget.id);
            setSelectedId(next[0]?.id ?? "");
            setDeleteTarget(null);
            onChange(next);
          }}
        />
      )}
    </section>
  );
}
