import { Cable, Copy, LayoutGrid, Monitor, Pencil, Plus, Radio, SquareStop, ToggleRight, Trash2 } from "lucide-react";
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
    ? board.safePins.filter((pin) => pin >= 0 && (pin <= 22 || (pin >= 26 && pin <= 29)))
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

function conflictingPins(hardware: HardwareProfile) {
  const counts = new Map<number, number>();
  const add = (pin: number) => counts.set(pin, (counts.get(pin) ?? 0) + 1);
  ownedInputPins(hardware).forEach(add);
  if (hardware.ssd1306) {
    add(hardware.ssd1306.sda);
    add(hardware.ssd1306.scl);
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
  if (!hardware.ssd1306) return false;
  if (!board?.supportsOled) return true;
  const safe = new Set(boardSafePins(board));
  return !safe.has(hardware.ssd1306.sda) || !safe.has(hardware.ssd1306.scl);
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
}: HardwareMappingProps) {
  const initialHardware = hardwareProfiles.find(
    ({ id }) => id === initialHardwareProfileId,
  ) ?? hardwareProfiles[0];
  const [selectedId, setSelectedId] = useState(initialHardware?.id ?? "");
  const [renaming, setRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<HardwareProfile | null>(null);
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
  const hardware = hardwareProfiles.find(({ id }) => id === selectedId) ?? hardwareProfiles[0];
  const board = boardProfiles.find(({ id }) => id === hardware?.board_profile_id);
  const compatibleDevices = devices.filter((device) =>
    device.connection === "online" &&
    device.mode === "runtime" &&
    device.identity === "valid" &&
    device.boardProfileId === hardware?.board_profile_id
  );
  const selectedDevice = compatibleDevices.find(({ deviceId }) => deviceId === selectedDeviceId);
  const activeLearning = selectedDevice ? selectedDevice.learning : learning;
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
    () => new Set(hardware?.ssd1306
      ? [hardware.ssd1306.sda, hardware.ssd1306.scl]
      : []),
    [hardware],
  );
  const oledAvailablePins = editablePins.filter((pin) => !inputPins.has(pin));
  const learningPins = editablePins.filter((pin) => !oledPins.has(pin));
  const buttons = layout.groups.flatMap((group) => group.buttons);

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

  const replaceHardware = (next: HardwareProfile) => {
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
    if (!hardware) return;
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
    if (hardware && name) replaceHardware({ ...hardware, name });
    setRenaming(false);
  };

  return (
    <section className="hardware-view" aria-labelledby="hardware-title">
      <div className="hardware-toolbar">
        <label>
          <span>{t(language, "hardware.profile")}</span>
          <select
            aria-label={t(language, "hardware.profile")}
            value={hardware?.id ?? ""}
            disabled={hardwareProfiles.length === 0}
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
          <button className="icon-button" type="button" aria-label={t(language, "hardware.addProfile")} title={t(language, "hardware.addProfile")} disabled={boardProfiles.length === 0} onClick={addHardware}>
            <Plus size={16} />
          </button>
          <button className="icon-button" type="button" aria-label={t(language, "hardware.duplicateProfile")} title={t(language, "hardware.duplicateProfile")} disabled={!hardware} onClick={duplicateHardware}>
            <Copy size={16} />
          </button>
          <button className="icon-button" type="button" aria-label={t(language, "hardware.renameProfile")} title={t(language, "hardware.renameProfile")} disabled={!hardware} onClick={() => {
            if (!hardware) return;
            renameOrigin.current = { id: hardware.id, profile: JSON.stringify(hardware) };
            setRenameValue(hardware.name);
            setRenaming(true);
          }}>
            <Pencil size={16} />
          </button>
          <button className="icon-button is-danger" type="button" aria-label={t(language, "hardware.deleteProfile")} title={t(language, "hardware.deleteProfile")} disabled={!hardware} onClick={() => setDeleteTarget(hardware ?? null)}>
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
          <div className="content-heading hardware-heading">
            <div>
              <span>{layout.name}</span>
              <h2 id="hardware-title">{t(language, "hardware.title")}</h2>
            </div>
            <label className="board-profile-field">
              <span>{t(language, "hardware.boardProfile")}</span>
              <select value={hardware.board_profile_id} onChange={(event) => {
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
                value={hardware.debounce_ms}
                onChange={(event) => replaceHardware({ ...hardware, debounce_ms: Number(event.target.value) })}
              />
              <small>ms</small>
            </label>
          </div>

          <section className="oled-editor" aria-label={t(language, "hardware.oled")}>
            <div className="oled-summary">
              <Monitor size={16} />
              <label className="oled-toggle">
                <input
                  type="checkbox"
                  checked={Boolean(hardware.ssd1306)}
                  disabled={!hardware.ssd1306 && (!board?.supportsOled || oledAvailablePins.length < 2)}
                  onChange={(event) => {
                    if (event.target.checked) {
                      const [sda, scl] = oledAvailablePins;
                      if (sda === undefined || scl === undefined) return;
                      replaceHardware({ ...hardware, ssd1306: { sda, scl } });
                    } else {
                      const next = { ...hardware };
                      delete next.ssd1306;
                      replaceHardware(next);
                    }
                  }}
                />
                <span>{t(language, "hardware.oled")}</span>
              </label>
              <code>{t(language, "hardware.oledFormat")}</code>
            </div>
            {hardware.ssd1306 && ([
              ["sda", t(language, "hardware.oledSda")],
              ["scl", t(language, "hardware.oledScl")],
            ] as const).map(([field, label]) => {
              const current = hardware.ssd1306?.[field] ?? 0;
              const counterpart = hardware.ssd1306?.[field === "sda" ? "scl" : "sda"];
              const excluded = new Set(inputPins);
              if (counterpart !== undefined) excluded.add(counterpart);
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
                    onChange={(event) => replaceHardware({
                      ...hardware,
                      ssd1306: {
                        ...hardware.ssd1306!,
                        [field]: Number(event.target.value),
                      },
                    })}
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
            <button type="button" onClick={() => replaceHardware({
              ...hardware,
              inputs: [...hardware.inputs, { type: "direct", id: `direct-${hardware.inputs.length + 1}`, keys: {} }],
            })}><Plus size={16} />{t(language, "hardware.addDirect")}</button>
            <button type="button" onClick={() => replaceHardware({
              ...hardware,
              inputs: [...hardware.inputs, { type: "contact_matrix", id: `matrix-${hardware.inputs.length + 1}`, pins: [], keys: {} }],
            })}><Plus size={16} />{t(language, "hardware.addMatrix")}</button>
            <button type="button" onClick={() => replaceHardware({
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

          <details className="learning-panel">
            <summary>
              <Radio size={15} />
              <span>{t(language, "hardware.advanced")}</span>
              <small>{t(language, "hardware.advancedHint")}</small>
            </summary>
            <div className={activeLearning ? "learning-controls is-learning" : "learning-controls"}>
              <label className="field-stack compact-field">
                <span>{t(language, "hardware.onlineDevice")}</span>
                <select aria-label={t(language, "hardware.onlineDevice")} value={selectedDevice?.deviceId ?? ""} onChange={(event) => setSelectedDeviceId(event.target.value)}>
                  <option value="">{t(language, "hardware.offlineEditing")}</option>
                  {compatibleDevices.map((device) => <option value={device.deviceId} key={device.deviceId}>{device.name}</option>)}
                </select>
              </label>
              <label className="safety-check">
                <input type="checkbox" required />
                <span>{t(language, "hardware.safety")}</span>
              </label>
              <fieldset>
                <legend>{t(language, "hardware.candidatePins")}</legend>
                <div className="pin-grid">
                  {learningPins.map((gpio) => (
                    <label key={gpio}><input aria-label={`GPIO ${gpio}`} type="checkbox" name="learning-pin" value={gpio} />{gpio}</label>
                  ))}
                </div>
              </fieldset>
              {activeLearning && selectedDevice ? (
                <button type="button" className="learning-button" onClick={() => onEndLearning(selectedDevice.deviceId)}><SquareStop size={16} />{t(language, "hardware.stopLearning")}</button>
              ) : (
                <button type="button" className="learning-button" disabled={!selectedDevice} onClick={(event) => {
                  const container = event.currentTarget.closest(".learning-controls");
                  const safe = container?.querySelector<HTMLInputElement>(".safety-check input")?.checked;
                  const pins = [...(container?.querySelectorAll<HTMLInputElement>('input[name="learning-pin"]:checked') ?? [])].map((input) => Number(input.value));
                  if (safe && pins.length && hardware && selectedDevice) {
                    onBeginLearning(hardware.id, selectedDevice.deviceId, pins);
                  }
                }}><Radio size={16} />{t(language, "hardware.startLearning")}</button>
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
