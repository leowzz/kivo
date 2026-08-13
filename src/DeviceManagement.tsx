import { Check, Pencil, Plus, RefreshCw, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { ConfirmDialog } from "./ConfirmDialog";
import {
  candidateDisplayLabel,
  compareDeviceAvailability,
  matchesDeviceFilter,
  primaryDeviceLabel,
  type DeviceFilter,
} from "./deviceStatus";
import { candidateSetupId } from "./deviceSetupSession";
import { t, type MessageKey } from "./i18n";
import type {
  BoardProfileSummary,
  CandidateIssue,
  CandidateStatus,
  DeviceStatus,
  HomeMetricsSnapshot,
  Language,
} from "./types";

type Selection = { kind: "device" | "candidate"; id: string };
type Row = { selection: Selection };
type OperationError = { owner: Selection; message: string };

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error && "code" in error) {
    return String(error.code);
  }
  return String(error);
}

const candidateMessages: Record<
  CandidateIssue,
  { title: MessageKey; body: MessageKey }
> = {
  validating: {
    title: "candidate.validating.title",
    body: "candidate.validating.body",
  },
  firmware_not_responding: {
    title: "candidate.firmware_not_responding.title",
    body: "candidate.firmware_not_responding.body",
  },
  firmware_incompatible: {
    title: "candidate.firmware_incompatible.title",
    body: "candidate.firmware_incompatible.body",
  },
  bootloader: {
    title: "candidate.bootloader.title",
    body: "candidate.bootloader.body",
  },
  port_unavailable: {
    title: "candidate.port_unavailable.title",
    body: "candidate.port_unavailable.body",
  },
  invalid_identity: {
    title: "candidate.invalid_identity.title",
    body: "candidate.invalid_identity.body",
  },
  duplicate_identity: {
    title: "candidate.duplicate_identity.title",
    body: "candidate.duplicate_identity.body",
  },
  unknown: {
    title: "candidate.unknown.title",
    body: "candidate.unknown.body",
  },
};

interface DeviceManagementProps {
  language: Language;
  devices: DeviceStatus[];
  candidates: CandidateStatus[];
  boardProfiles: BoardProfileSummary[];
  metrics: { deviceId: string; snapshot: HomeMetricsSnapshot } | null;
  onRename(deviceId: string, name: string): void | Promise<void>;
  onForget(deviceId: string): void | Promise<void>;
  onMetricsChange(deviceId: string | null): void;
  onOpenSetup(targetId: string | null): void;
  onRetryCandidate(deviceId: string): void | Promise<void>;
  selectedDeviceId?: string | null;
  selectedCandidateKey?: string | null;
  onSelectedDeviceChange?(deviceId: string | null): void;
  onSelectedCandidateChange?(candidateKey: string | null): void;
}

function matches(values: string[], query: string) {
  const term = query.trim().toLocaleLowerCase();
  return (
    !term || values.some((value) => value.toLocaleLowerCase().includes(term))
  );
}
function status(device: DeviceStatus, language: Language) {
  const keys: Record<string, string> = {
    设备身份冲突: "identityConflict",
    设备身份无效: "identityInvalid",
    分配需要修复: "assignmentInvalid",
    运行错误: "runtimeError",
    引导加载模式: "bootloader",
    未分配: "unassigned",
    离线: "offline",
    正在验证: "validating",
    正在配置: "configuring",
    正在学习: "learning",
    就绪: "ready",
    未运行: "inactive",
  };
  return t(
    language,
    `devices.status.${keys[primaryDeviceLabel(device)]}` as never,
  );
}
function Detail({ label, value, valueClassName }: { label: string; value: string; valueClassName?: string }) {
  return (
    <div className="device-detail-field">
      <span>{label}</span>
      <span className={valueClassName}>{value}</span>
    </div>
  );
}
function TechnicalDetail({ label, value, valueClassName }: { label: string; value: string; valueClassName?: string }) {
  return (
    <>
      <dt>{label}</dt>
      <dd className={valueClassName}>{value}</dd>
    </>
  );
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
  onOpenSetup,
  onRetryCandidate,
  selectedDeviceId: controlledDeviceId,
  selectedCandidateKey,
  onSelectedDeviceChange,
  onSelectedCandidateChange,
}: DeviceManagementProps) {
  const [filter, setFilter] = useState<DeviceFilter>("all");
  const [query, setQuery] = useState("");
  const [selection, setSelection] = useState<Selection | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [name, setName] = useState("");
  const [confirmId, setConfirmId] = useState<string | null>(null);
  const [error, setError] = useState<OperationError | null>(null);
  const [candidateRetrying, setCandidateRetrying] = useState(false);
  const [forgetting, setForgetting] = useState(false);
  const previous = useRef<Row[]>([]);
  const candidateRetryInFlight = useRef(false);
  const boards = useMemo(
    () => new Map(boardProfiles.map((board) => [board.id, board])),
    [boardProfiles],
  );
  const candidateLabels = useMemo(
    () =>
      new Map(
        candidates.map((candidate, index) => [
          candidate.key,
          candidateDisplayLabel(candidate, index + 1, language),
        ]),
      ),
    [candidates, language],
  );
  const visibleDevices = devices
    .filter((device) =>
      matchesDeviceFilter(device, filter, "") &&
      matches([
        device.name,
        device.hardwareSerial,
        device.deviceId,
        device.boardProfileId,
        boards.get(device.boardProfileId)?.displayName ?? "",
        device.port ?? "",
      ], query),
    )
    .sort(compareDeviceAvailability);
  const visibleCandidates =
    filter === "all" || filter === "attention"
      ? candidates.filter((candidate) =>
          matches(
            [
              candidate.rawSerial ?? "",
              candidate.deviceId ?? "",
              candidate.boardProfileId,
              boards.get(candidate.boardProfileId)?.displayName ?? "",
              candidate.port ?? "",
            ],
            query,
          ),
        )
      : [];
  const rows = useMemo<Row[]>(
    () => [
      ...visibleDevices.map((device) => ({
        selection: { kind: "device" as const, id: device.deviceId },
      })),
      ...visibleCandidates.map((candidate) => ({
        selection: { kind: "candidate" as const, id: candidate.key },
      })),
    ],
    [visibleDevices, visibleCandidates],
  );
  const requestedSelection: Selection | null = controlledDeviceId
    ? { kind: "device", id: controlledDeviceId }
    : selectedCandidateKey
      ? { kind: "candidate", id: selectedCandidateKey }
      : selection;
  const requestedExists =
    requestedSelection &&
    rows.some(
      (row) =>
        row.selection.kind === requestedSelection.kind &&
        row.selection.id === requestedSelection.id,
    );
  const previousIndex = requestedSelection
    ? previous.current.findIndex(
        (row) =>
          row.selection.kind === requestedSelection.kind &&
          row.selection.id === requestedSelection.id,
      )
    : 0;
  const activeSelection = requestedExists
    ? requestedSelection
    : (rows[Math.max(0, Math.min(previousIndex, rows.length - 1))]
        ?.selection ?? null);
  useEffect(() => {
    if (
      selection?.kind !== activeSelection?.kind ||
      selection?.id !== activeSelection?.id
    ) {
      setSelection(activeSelection);
    }
    previous.current = rows;
  }, [activeSelection?.id, activeSelection?.kind, rows, selection?.id, selection?.kind]);
  const selectedDevice =
    activeSelection?.kind === "device"
      ? (devices.find((device) => device.deviceId === activeSelection.id) ?? null)
      : null;
  useEffect(() => {
    if (selectedCandidateKey && !candidates.some(({ key }) => key === selectedCandidateKey)) {
      onSelectedCandidateChange?.(null);
    }
  }, [candidates, onSelectedCandidateChange, selectedCandidateKey]);
  const selectRow = (next: Selection) => {
    setSelection(next);
    onSelectedDeviceChange?.(next.kind === "device" ? next.id : null);
    onSelectedCandidateChange?.(next.kind === "candidate" ? next.id : null);
  };
  const selectedCandidate =
    activeSelection?.kind === "candidate"
      ? (candidates.find((candidate) => candidate.key === activeSelection.id) ?? null)
      : null;
  const selectedError =
    error &&
    activeSelection?.kind === error.owner.kind &&
    activeSelection.id === error.owner.id
      ? error.message
      : null;
  const selectedMetrics =
    selectedDevice && metrics?.deviceId === selectedDevice.deviceId
      ? metrics.snapshot
      : null;
  const confirmDevice = confirmId
    ? (devices.find(
        (device) =>
          device.deviceId === confirmId && device.connection === "offline",
      ) ?? null)
    : null;
  useEffect(() => {
    onMetricsChange(selectedDevice?.deviceId ?? null);
  }, [onMetricsChange, selectedDevice?.deviceId]);
  useEffect(() => {
    setRenaming(false);
    setName(selectedDevice?.name ?? "");
  }, [selectedDevice?.deviceId]);
  useEffect(() => {
    if (confirmId && !confirmDevice) setConfirmId(null);
  }, [confirmDevice, confirmId]);
  const rename = async () => {
    if (!selectedDevice || !name.trim()) return;
    try {
      setError(null);
      await onRename(selectedDevice.deviceId, name.trim());
      setRenaming(false);
    } catch (reason) {
      setError({
        owner: { kind: "device", id: selectedDevice.deviceId },
        message: errorMessage(reason),
      });
    }
  };
  const forget = async () => {
    if (!confirmDevice || forgetting) return;
    try {
      setForgetting(true);
      setError(null);
      await onForget(confirmDevice.deviceId);
      setConfirmId(null);
    } catch (reason) {
      setError({
        owner: { kind: "device", id: confirmDevice.deviceId },
        message: errorMessage(reason),
      });
    } finally {
      setForgetting(false);
    }
  };
  const selectedCandidateMessages = selectedCandidate
    ? candidateMessages[selectedCandidate.issue]
    : null;
  const canRetryCandidate = Boolean(
    selectedCandidate?.deviceId &&
    [
      "validating",
      "firmware_not_responding",
      "firmware_incompatible",
      "port_unavailable",
      "unknown",
    ].includes(selectedCandidate.issue),
  );
  const retryCandidate = async () => {
    if (!selectedCandidate?.deviceId || candidateRetryInFlight.current) return;
    candidateRetryInFlight.current = true;
    setCandidateRetrying(true);
    setError(null);
    try {
      await onRetryCandidate(selectedCandidate.deviceId);
    } catch (reason) {
      setError({
        owner: { kind: "candidate", id: selectedCandidate.key },
        message: errorMessage(reason),
      });
    } finally {
      candidateRetryInFlight.current = false;
      setCandidateRetrying(false);
    }
  };
  return (
    <div className="device-management">
      <section
        className="device-list-region"
        aria-label={t(language, "devices.list")}
      >
        <header className="device-list-header">
          <h2>{t(language, "nav.devices")}</h2>
          <button
            className="primary-button device-list-command"
            type="button"
            onClick={() => onOpenSetup(null)}
          >
            <Plus size={16} />
            {t(language, "setup.addKeyboard")}
          </button>
          <label className="device-search">
            <span>{t(language, "devices.search")}</span>
            <input
              type="search"
              aria-label={t(language, "devices.search")}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>
          <div
            className="device-filter"
            role="group"
            aria-label={t(language, "devices.filters")}
          >
            {(["all", "attention", "ready", "offline"] as const).map((item) => (
              <button
                key={item}
                type="button"
                className={filter === item ? "is-active" : ""}
                aria-pressed={filter === item}
                onClick={() => setFilter(item)}
              >
                {t(language, `devices.filter.${item}`)}
              </button>
            ))}
          </div>
        </header>
        <div className="device-table">
          <div className="device-table-head" aria-hidden="true">
            <span>{t(language, "devices.name")}</span>
            <span>{t(language, "devices.board")}</span>
            <span>{t(language, "devices.status")}</span>
          </div>
          <ul>
            {visibleDevices.map((device) => (
              <li key={device.deviceId}>
                <button
                  className={`device-row ${activeSelection?.kind === "device" && activeSelection.id === device.deviceId ? "is-selected" : ""}`}
                  type="button"
                  aria-pressed={
                    activeSelection?.kind === "device" &&
                    activeSelection.id === device.deviceId
                  }
                  onClick={() =>
                    selectRow({ kind: "device", id: device.deviceId })
                  }
                >
                  <strong title={device.name}>{device.name}</strong>
                  <span title={boards.get(device.boardProfileId)?.displayName ?? device.boardProfileId}>
                    {boards.get(device.boardProfileId)?.displayName ??
                      device.boardProfileId}
                  </span>
                  <span title={status(device, language)}>{status(device, language)}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
        {visibleCandidates.length > 0 && (
          <section
            className="candidate-section"
            aria-label={t(language, "devices.attentionSection")}
          >
            <h3>{t(language, "devices.attentionSection")}</h3>
            <ul>
              {visibleCandidates.map((candidate) => (
                <li key={candidate.key}>
                  <button
                  className={`device-row candidate-row ${activeSelection?.kind === "candidate" && activeSelection.id === candidate.key ? "is-selected" : ""}`}
                    type="button"
                    aria-pressed={
                    activeSelection?.kind === "candidate" &&
                    activeSelection.id === candidate.key
                    }
                    onClick={() =>
                      selectRow({ kind: "candidate", id: candidate.key })
                    }
                  >
                    <strong title={candidateLabels.get(candidate.key)}>{candidateLabels.get(candidate.key)}</strong>
                    <span title={boards.get(candidate.boardProfileId)?.displayName ?? candidate.boardProfileId}>
                      {boards.get(candidate.boardProfileId)?.displayName ??
                        candidate.boardProfileId}
                    </span>
                    <span>{t(language, "devices.filter.attention")}</span>
                    <span>-</span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}
      </section>
      <aside
        className="device-detail"
        aria-label={t(language, "devices.detail")}
      >
        {!selectedDevice && !selectedCandidate && (
          <p className="panel-empty">{t(language, "devices.select")}</p>
        )}
        {selectedCandidate && (
          <>
            <h2>{t(language, "devices.diagnostics")}</h2>
            {selectedCandidateMessages && (
              <div className="candidate-issue">
                <h3>{t(language, selectedCandidateMessages.title)}</h3>
                <p>{t(language, selectedCandidateMessages.body)}</p>
              </div>
            )}
            <div className="candidate-actions">
              {canRetryCandidate && (
                <button
                  type="button"
                  disabled={candidateRetrying}
                  onClick={() => void retryCandidate()}
                >
                  <RefreshCw size={16} />
                  {t(language, "setup.retry")}
                </button>
              )}
              <button
                className="primary-button"
                type="button"
                onClick={() => onOpenSetup(candidateSetupId(selectedCandidate))}
              >
                {t(language, "setup.continue")}
              </button>
            </div>
            <details className="device-technical-details">
              <summary>{t(language, "setup.technicalDetails")}</summary>
              <dl>
                <TechnicalDetail
                  label={t(language, "devices.serial")}
                  value={selectedCandidate.rawSerial ?? "-"}
                />
                <TechnicalDetail
                  label={t(language, "devices.id")}
                  value={selectedCandidate.deviceId ?? "-"}
                />
                <TechnicalDetail
                  label={t(language, "devices.board")}
                  value={
                    boards.get(selectedCandidate.boardProfileId)?.displayName ??
                    selectedCandidate.boardProfileId
                  }
                />
                <TechnicalDetail
                  label={t(language, "devices.controller")}
                  value={selectedCandidate.controllerFamilyId}
                />
                <TechnicalDetail
                  label={t(language, "devices.mode")}
                  value={selectedCandidate.mode}
                />
                <TechnicalDetail
                  label={t(language, "setup.systemPort")}
                  value={selectedCandidate.port ?? "-"}
                />
                <TechnicalDetail
                  label={t(language, "devices.error")}
                  value={selectedCandidate.latestError ?? "-"}
                />
              </dl>
            </details>
            {selectedError && (
              <p className="field-error" role="alert">
                {selectedError}
              </p>
            )}
          </>
        )}
        {selectedDevice && (
          <>
            <div className="device-detail-title">
              <h2>
                {renaming ? t(language, "devices.rename") : selectedDevice.name}
              </h2>
              {!renaming && (
                <button
                  className="icon-button"
                  type="button"
                  aria-label={t(language, "devices.rename")}
                  title={t(language, "devices.rename")}
                  onClick={() => setRenaming(true)}
                >
                  <Pencil size={16} />
                </button>
              )}
            </div>
            {renaming && (
              <div className="device-rename">
                <input
                  aria-label={t(language, "devices.name")}
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                />
                <button
                  className="icon-button"
                  type="button"
                  aria-label={t(language, "devices.confirmRename")}
                  title={t(language, "devices.confirmRename")}
                  onClick={() => void rename()}
                >
                  <Check size={16} />
                </button>
                <button
                  className="icon-button"
                  type="button"
                  aria-label={t(language, "common.cancel")}
                  title={t(language, "common.cancel")}
                  onClick={() => setRenaming(false)}
                >
                  <X size={16} />
                </button>
              </div>
            )}
            <Detail
              label={t(language, "devices.board")}
              value={
                boards.get(selectedDevice.boardProfileId)?.displayName ??
                selectedDevice.boardProfileId
              }
            />
            <Detail
              label={t(language, "devices.status")}
              value={status(selectedDevice, language)}
            />
            {selectedDevice.connection === "online" &&
              selectedDevice.mode === "runtime" &&
              selectedDevice.identity === "valid" &&
              selectedDevice.assignment === "unassigned" && (
                <button
                  className="primary-button setup-command"
                  type="button"
                  onClick={() => onOpenSetup(selectedDevice.deviceId)}
                >
                  {t(language, "setup.continue")}
                </button>
              )}
            <div>
                <details className="device-technical-details">
                  <summary>{t(language, "setup.technicalDetails")}</summary>
                  <dl>
                    <TechnicalDetail
                      label={t(language, "devices.serial")}
                      value={selectedDevice.hardwareSerial}
                    />
                    <TechnicalDetail
                      label={t(language, "devices.id")}
                      value={selectedDevice.deviceId}
                      valueClassName="device-id-value"
                    />
                    <TechnicalDetail
                      label={t(language, "devices.controller")}
                      value={selectedDevice.controllerFamilyId}
                    />
                    <TechnicalDetail
                      label={t(language, "devices.mode")}
                      value={selectedDevice.mode ?? "-"}
                    />
                    <TechnicalDetail
                      label={t(language, "setup.systemPort")}
                      value={selectedDevice.port ?? "-"}
                    />
                    <TechnicalDetail
                      label={t(language, "devices.firmware")}
                      value={selectedDevice.firmwareBuildId ?? "-"}
                    />
                    <TechnicalDetail
                      label={t(language, "devices.pins")}
                      value={selectedDevice.capabilities.join(", ") || "-"}
                    />
                    <TechnicalDetail
                      label={t(language, "devices.error")}
                      value={selectedDevice.latestError?.detail ?? "-"}
                    />
                  </dl>
                </details>
                {selectedMetrics && (
                  <>
                    <section className="device-metrics" aria-label={t(language, "devices.metricsSummary")}>
                      <div><span>{t(language, "home.todayPresses")}</span><strong>{selectedMetrics.todayPresses}</strong></div>
                      <div><span>{t(language, "home.totalPresses")}</span><strong>{selectedMetrics.totalPresses}</strong></div>
                      <div><span>{t(language, "home.activeButtons")}</span><strong>{selectedMetrics.activeButtonCount}</strong></div>
                      <div><span>{t(language, "home.topButton")}</span><strong>{selectedMetrics.topButton?.buttonId ?? "-"}</strong></div>
                    </section>
                    <div className="device-activity-wrap">
                      <table className="device-activity" aria-label={t(language, "devices.activity")}>
                        <tbody>
                          {selectedMetrics.logs.map((log) => (
                            <tr key={`${log.timestampMs}:${log.deviceId}:${log.deviceProfileId}:${log.hardwareProfileId}:${log.message}`}>
                              <td><time>{new Date(log.timestampMs).toLocaleTimeString()}</time></td>
                              <td>{log.deviceName}</td>
                              <td>{log.message}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </>
                )}
              </div>
            {selectedError && (
              <p className="field-error" role="alert">
                {selectedError}
              </p>
            )}
            <button
              className="icon-button is-danger device-forget"
              type="button"
              aria-label={t(language, "devices.forget")}
              title={t(language, "devices.forget")}
              disabled={selectedDevice.connection !== "offline"}
              onClick={() => setConfirmId(selectedDevice.deviceId)}
            >
              <Trash2 size={16} />
            </button>
          </>
        )}
      </aside>
      {confirmDevice && (
        <ConfirmDialog
          title={t(language, "devices.forget")}
          body={t(language, "devices.forgetBody", { name: confirmDevice.name })}
          confirmLabel={t(language, "common.confirm")}
          cancelLabel={t(language, "common.cancel")}
          danger
          onCancel={() => setConfirmId(null)}
          onConfirm={() => void forget()}
        />
      )}
    </div>
  );
}
