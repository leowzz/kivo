import { Copy, Pencil, Plus, Radio, SquareStop, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
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
  selectedButtonId: string | null;
  onSelectButton(buttonId: string): void;
  onChange(hardwareProfiles: HardwareProfile[]): void;
  onBeginLearning(pins: number[]): void;
  onEndLearning(): void;
}

function sourceName(language: Language, source: InputSource) {
  return t(language, source.type === "direct" ? "hardware.direct" : "hardware.matrix");
}

function uniqueValue(base: string, existing: Set<string>) {
  if (!existing.has(base)) return base;
  let suffix = 2;
  while (existing.has(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
}

function boardSafePins(board: BoardProfileSummary | undefined) {
  if (!board) return [];
  return board.id === "vccgnd-yd-rp2040"
    ? board.safePins.filter((pin) => pin >= 0 && pin <= 22)
    : board.safePins;
}

function invalidPins(hardware: HardwareProfile, boardProfiles: readonly BoardProfileSummary[]) {
  const board = boardProfiles.find(({ id }) => id === hardware.board_profile_id);
  if (!board) return new Set(hardware.inputs.flatMap((source) =>
    source.type === "direct" ? Object.values(source.keys) : source.pins
  ));
  const safe = new Set(boardSafePins(board));
  return new Set(hardware.inputs.flatMap((source) =>
    (source.type === "direct" ? Object.values(source.keys) : source.pins).filter((pin) => !safe.has(pin))
  ));
}

export function hardwareProfilesAreValid(
  profiles: readonly HardwareProfile[],
  boardProfiles: readonly BoardProfileSummary[],
) {
  return profiles.every((profile) =>
    boardProfiles.some(({ id }) => id === profile.board_profile_id) &&
    invalidPins(profile, boardProfiles).size === 0
  );
}

function invalidMessage(language: Language, pins: readonly number[]) {
  return t(language, "hardware.invalidPins", { pins: pins.join(language === "zh-CN" ? "、" : ", ") });
}

export function HardwareMapping({
  language,
  layout,
  hardwareProfiles,
  boardProfiles,
  devices,
  learning,
  selectedButtonId,
  onSelectButton,
  onChange,
  onBeginLearning,
  onEndLearning,
}: HardwareMappingProps) {
  const [selectedId, setSelectedId] = useState(hardwareProfiles[0]?.id ?? "");
  const [renaming, setRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<HardwareProfile | null>(null);
  const [selectedDeviceId, setSelectedDeviceId] = useState("");
  const hardware = hardwareProfiles.find(({ id }) => id === selectedId) ?? hardwareProfiles[0];
  const board = boardProfiles.find(({ id }) => id === hardware?.board_profile_id);
  const compatibleDevices = devices.filter((device) =>
    device.connection === "online" &&
    device.identity === "valid" &&
    device.boardProfileId === hardware?.board_profile_id
  );
  const selectedDevice = compatibleDevices.find(({ deviceId }) => deviceId === selectedDeviceId);
  const editablePins = selectEditablePins(boardSafePins(board), selectedDevice?.capabilities ?? null);
  const invalid = useMemo(
    () => hardware ? invalidPins(hardware, boardProfiles) : new Set<number>(),
    [boardProfiles, hardware],
  );
  const buttons = layout.groups.flatMap((group) => group.buttons);

  useEffect(() => {
    if (!compatibleDevices.some(({ deviceId }) => deviceId === selectedDeviceId)) {
      setSelectedDeviceId("");
    }
  }, [compatibleDevices, selectedDeviceId]);

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
    const number = id === `${selectedBoard.id}-hardware` ? "" : ` ${id.split("-").at(-1)}`;
    const next: HardwareProfile = {
      id,
      name: `${selectedBoard.displayName} ${t(language, "hardware.profile")}${number}`,
      board_profile_id: selectedBoard.id,
      debounce_ms: 30,
      inputs: [],
    };
    setSelectedId(id);
    onChange([...hardwareProfiles, next]);
  };

  const duplicateHardware = () => {
    if (!hardware) return;
    const ids = new Set(hardwareProfiles.map(({ id }) => id));
    const id = uniqueValue(`${hardware.id}-copy`, ids);
    const baseName = `${hardware.name} ${t(language, "hardware.copySuffix")}`;
    const names = new Set(hardwareProfiles.map(({ name }) => name));
    const name = names.has(baseName)
      ? `${baseName} ${id.split("-").at(-1)}`
      : baseName;
    const next = structuredClone({ ...hardware, id, name });
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

          <div className="source-list">
            {hardware.inputs.map((source, sourceIndex) => (
              <section className="source-editor" key={`${source.type}-${source.id}`}>
                <div className="source-heading">
                  <div><strong>{sourceName(language, source)}</strong><code>{source.id}</code></div>
                  <button className="icon-button is-danger" type="button" aria-label={t(language, "hardware.removeSource")} title={t(language, "hardware.removeSource")} onClick={() => replaceHardware({
                    ...hardware,
                    inputs: hardware.inputs.filter((_, index) => index !== sourceIndex),
                  })}>
                    <Trash2 size={16} />
                  </button>
                </div>

                {source.type === "contact_matrix" && (() => {
                  const invalidSourcePins = source.pins.filter((pin) => invalid.has(pin));
                  return (
                    <label className="field-stack compact-field">
                      <span>{t(language, "hardware.matrixPins")}</span>
                      <input
                        aria-invalid={invalidSourcePins.length > 0}
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
                        const currentInvalid = current !== undefined && invalid.has(current);
                        const options = currentInvalid ? [current, ...editablePins] : editablePins;
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
                            {currentInvalid && <small className="field-error">{invalidMessage(language, [current])}</small>}
                          </div>
                        );
                      })() : (
                        <div className="contact-inputs">
                          {[0, 1].map((side) => {
                            const current = source.keys[button.id]?.[side];
                            return (
                              <select
                                aria-label={`${button.label} ${side === 0 ? "A" : "B"}`}
                                aria-invalid={current !== undefined && invalid.has(current)}
                                value={current ?? ""}
                                key={side}
                                onChange={(event) => {
                                  const keys = { ...source.keys };
                                  const currentPair = source.keys[button.id];
                                  if (event.target.value === "") delete keys[button.id];
                                  else {
                                    const selected = Number(event.target.value);
                                    const other = currentPair?.[side === 0 ? 1 : 0] ?? source.pins.find((pin) => pin !== selected);
                                    if (other !== undefined && other !== selected) {
                                      keys[button.id] = selected < other ? [selected, other] : [other, selected];
                                    }
                                  }
                                  updateSource(sourceIndex, { ...source, keys });
                                }}
                              >
                                <option value="">-</option>
                                {source.pins.map((pin) => <option value={pin} key={pin}>{pin}</option>)}
                              </select>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  ))}
                </div>
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
          </div>

          <details className="learning-panel">
            <summary>{t(language, "hardware.advanced")}</summary>
            <div className="learning-controls">
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
                  {editablePins.map((gpio) => (
                    <label key={gpio}><input aria-label={`GPIO ${gpio}`} type="checkbox" name="learning-pin" value={gpio} />{gpio}</label>
                  ))}
                </div>
              </fieldset>
              {learning ? (
                <button type="button" className="learning-button" onClick={onEndLearning}><SquareStop size={16} />{t(language, "hardware.stopLearning")}</button>
              ) : (
                <button type="button" className="learning-button" onClick={(event) => {
                  const container = event.currentTarget.closest(".learning-controls");
                  const safe = container?.querySelector<HTMLInputElement>(".safety-check input")?.checked;
                  const pins = [...(container?.querySelectorAll<HTMLInputElement>('input[name="learning-pin"]:checked') ?? [])].map((input) => Number(input.value));
                  if (safe && pins.length) onBeginLearning(pins);
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
