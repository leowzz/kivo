import { Check, Pencil, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { ConfirmDialog } from "./ConfirmDialog";
import { matchesDeviceFilter, primaryDeviceLabel, type DeviceFilter } from "./deviceStatus";
import { t } from "./i18n";
import type { BoardProfileSummary, CandidateStatus, DeviceStatus, HomeMetricsSnapshot, Language } from "./types";

type Selection = { kind: "device"; id: string } | { kind: "candidate"; id: string };
type Row = { selection: Selection; label: string };

interface DeviceManagementProps {
  language: Language;
  devices: DeviceStatus[];
  candidates: CandidateStatus[];
  boardProfiles: BoardProfileSummary[];
  metrics: HomeMetricsSnapshot | null;
  onRename(deviceId: string, name: string): void | Promise<void>;
  onForget(deviceId: string): void | Promise<void>;
  onMetricsChange(deviceId: string | null): void;
}

function assignmentLabel(device: DeviceStatus) {
  return device.runtimeAssignment
    ? `${device.runtimeAssignment.device_profile_id} / ${device.runtimeAssignment.hardware_profile_id}`
    : "-";
}

function candidateMatches(candidate: CandidateStatus, query: string) {
  const normalized = query.trim().toLocaleLowerCase();
  return !normalized || [candidate.rawSerial ?? "", candidate.boardProfileId, candidate.port ?? "", candidate.controllerFamilyId]
    .some((value) => value.toLocaleLowerCase().includes(normalized));
}

export function DeviceManagement({
  language,
  devices,
  candidates,
  boardProfiles,
  metrics,
  onRename,
  onForget,
  onMetricsChange,
}: DeviceManagementProps) {
  const [filter, setFilter] = useState<DeviceFilter>("all");
  const [query, setQuery] = useState("");
  const [selection, setSelection] = useState<Selection | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [name, setName] = useState("");
  const [confirmForget, setConfirmForget] = useState<DeviceStatus | null>(null);
  const previousRows = useRef<Row[]>([]);
  const boardById = useMemo(() => new Map(boardProfiles.map((board) => [board.id, board])), [boardProfiles]);
  const visibleDevices = useMemo(() => devices.filter((device) => matchesDeviceFilter(device, filter, query)), [devices, filter, query]);
  const visibleCandidates = useMemo(() => (filter === "all" || filter === "attention")
    ? candidates.filter((candidate) => candidateMatches(candidate, query)) : [], [candidates, filter, query]);
  const rows = useMemo<Row[]>(() => [
    ...visibleDevices.map((device) => ({ selection: { kind: "device" as const, id: device.deviceId }, label: device.name })),
    ...visibleCandidates.map((candidate) => ({ selection: { kind: "candidate" as const, id: candidate.key }, label: candidate.rawSerial ?? candidate.key })),
  ], [visibleDevices, visibleCandidates]);

  useEffect(() => {
    const present = selection && rows.some((row) => row.selection.kind === selection.kind && row.selection.id === selection.id);
    if (!present) {
      const priorIndex = selection ? previousRows.current.findIndex((row) =>
        row.selection.kind === selection.kind && row.selection.id === selection.id
      ) : -1;
      setSelection(rows[Math.max(0, Math.min(priorIndex < 0 ? 0 : priorIndex, rows.length - 1))]?.selection ?? null);
    }
    previousRows.current = rows;
  }, [rows, selection]);

  const selectedDevice = selection?.kind === "device" ? devices.find((device) => device.deviceId === selection.id) ?? null : null;
  const selectedCandidate = selection?.kind === "candidate" ? candidates.find((candidate) => candidate.key === selection.id) ?? null : null;

  useEffect(() => {
    onMetricsChange(selectedDevice?.deviceId ?? null);
  }, [onMetricsChange, selectedDevice?.deviceId]);

  useEffect(() => {
    setRenaming(false);
    setName(selectedDevice?.name ?? "");
  }, [selectedDevice?.deviceId]);

  const select = (next: Selection) => setSelection(next);
  const saveName = async () => {
    if (!selectedDevice || !name.trim()) return;
    await onRename(selectedDevice.deviceId, name.trim());
    setRenaming(false);
  };

  return (
    <div className="device-management">
      <section className="device-list-region" aria-label={t(language, "devices.list")}>
        <header className="device-list-header">
          <h2>{t(language, "nav.devices")}</h2>
          <label className="device-search"><span>{t(language, "devices.search")}</span><input
            type="search"
            aria-label={t(language, "devices.search")}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          /></label>
          <div className="device-filter" role="group" aria-label={t(language, "devices.filters")}>
            {(["all", "attention", "ready", "offline"] as const).map((item) => <button
              key={item}
              type="button"
              className={filter === item ? "is-active" : ""}
              aria-pressed={filter === item}
              onClick={() => setFilter(item)}
            >{t(language, `devices.filter.${item}`)}</button>)}
          </div>
        </header>
        <div className="device-table" role="list">
          <div className="device-table-head" aria-hidden="true"><span>{t(language, "devices.name")}</span><span>{t(language, "devices.board")}</span><span>{t(language, "devices.status")}</span><span>{t(language, "devices.assignment")}</span><span>{t(language, "devices.port")}</span></div>
          {visibleDevices.map((device) => <button
            className={`device-row ${selection?.kind === "device" && selection.id === device.deviceId ? "is-selected" : ""}`}
            key={device.deviceId}
            type="button"
            aria-pressed={selection?.kind === "device" && selection.id === device.deviceId}
            onClick={() => select({ kind: "device", id: device.deviceId })}
          ><strong>{device.name}</strong><span>{boardById.get(device.boardProfileId)?.displayName ?? device.boardProfileId}</span><span>{primaryDeviceLabel(device)}</span><span>{assignmentLabel(device)}</span><span>{device.port ?? "-"}</span></button>)}
        </div>
        {visibleCandidates.length > 0 && <section className="candidate-section" aria-label={t(language, "devices.attentionSection")}>
          <h3>{t(language, "devices.attentionSection")}</h3>
          {visibleCandidates.map((candidate) => <button
            className={`device-row candidate-row ${selection?.kind === "candidate" && selection.id === candidate.key ? "is-selected" : ""}`}
            key={candidate.key}
            type="button"
            aria-pressed={selection?.kind === "candidate" && selection.id === candidate.key}
            onClick={() => select({ kind: "candidate", id: candidate.key })}
          ><strong>{candidate.rawSerial ?? candidate.key}</strong><span>{boardById.get(candidate.boardProfileId)?.displayName ?? candidate.boardProfileId}</span><span>{t(language, "devices.filter.attention")}</span><span>-</span><span>{candidate.port ?? "-"}</span></button>)}
        </section>}
      </section>

      <aside className="device-detail" aria-label={t(language, "devices.detail")}>
        {!selectedDevice && !selectedCandidate && <p className="panel-empty">{t(language, "devices.select")}</p>}
        {selectedCandidate && <>
          <h2>{t(language, "devices.diagnostics")}</h2>
          <Detail label={t(language, "devices.serial")} value={selectedCandidate.rawSerial ?? "-"} />
          <Detail label={t(language, "devices.board")} value={boardById.get(selectedCandidate.boardProfileId)?.displayName ?? selectedCandidate.boardProfileId} />
          <Detail label={t(language, "devices.controller")} value={selectedCandidate.controllerFamilyId} />
          <Detail label={t(language, "devices.mode")} value={selectedCandidate.mode} />
          <Detail label={t(language, "devices.port")} value={selectedCandidate.port ?? "-"} />
          <Detail label={t(language, "devices.error")} value={selectedCandidate.latestError ?? "-"} />
        </>}
        {selectedDevice && <>
          <div className="device-detail-title"><h2>{renaming ? t(language, "devices.rename") : selectedDevice.name}</h2>{!renaming && <button className="icon-button" type="button" aria-label={t(language, "devices.rename")} title={t(language, "devices.rename")} onClick={() => setRenaming(true)}><Pencil size={16} /></button>}</div>
          {renaming && <div className="device-rename"><input aria-label={t(language, "devices.name")} value={name} onChange={(event) => setName(event.target.value)} /><button className="icon-button" type="button" aria-label={t(language, "devices.confirmRename")} title={t(language, "devices.confirmRename")} onClick={() => void saveName()}><Check size={16} /></button><button className="icon-button" type="button" aria-label={t(language, "common.cancel")} title={t(language, "common.cancel")} onClick={() => { setName(selectedDevice.name); setRenaming(false); }}><X size={16} /></button></div>}
          <Detail label={t(language, "devices.serial")} value={selectedDevice.hardwareSerial} />
          <Detail label={t(language, "devices.id")} value={selectedDevice.deviceId} />
          <Detail label={t(language, "devices.controller")} value={selectedDevice.controllerFamilyId} />
          <Detail label={t(language, "devices.board")} value={boardById.get(selectedDevice.boardProfileId)?.displayName ?? selectedDevice.boardProfileId} />
          <Detail label={t(language, "devices.mode")} value={selectedDevice.mode ?? "-"} />
          <Detail label={t(language, "devices.port")} value={selectedDevice.port ?? "-"} />
          <Detail label={t(language, "devices.firmware")} value={selectedDevice.firmwareBuildId ?? "-"} />
          <Detail label={t(language, "devices.pins")} value={selectedDevice.capabilities.join(", ") || "-"} />
          <Detail label={t(language, "devices.assignment")} value={assignmentLabel(selectedDevice)} />
          <Detail label={t(language, "devices.error")} value={selectedDevice.latestError?.detail ?? "-"} />
          <Detail label={t(language, "devices.metrics")} value={metrics ? `${metrics.todayPresses} / ${metrics.totalPresses}` : "-"} />
          <button className="icon-button is-danger device-forget" type="button" aria-label={t(language, "devices.forget")} title={t(language, "devices.forget")} disabled={selectedDevice.connection !== "offline"} onClick={() => setConfirmForget(selectedDevice)}><Trash2 size={16} /></button>
        </>}
      </aside>
      {confirmForget && <ConfirmDialog title={t(language, "devices.forget")} body={t(language, "devices.forgetBody", { name: confirmForget.name })} confirmLabel={t(language, "common.confirm")} cancelLabel={t(language, "common.cancel")} danger onCancel={() => setConfirmForget(null)} onConfirm={() => { void onForget(confirmForget.deviceId); setConfirmForget(null); }} />}
    </div>
  );
}

function Detail({ label, value }: { label: string; value: string }) {
  return <div className="device-detail-field"><span>{label}</span><output>{value}</output></div>;
}
