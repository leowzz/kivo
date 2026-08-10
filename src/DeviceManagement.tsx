import { Check, Pencil, Plus, RefreshCw, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ConfirmDialog } from "./ConfirmDialog";
import { ConfigurationSettingsDialog } from "./ConfigurationSettingsDialog";
import { HardwareMapping } from "./HardwareMapping";
import { LayoutEditor } from "./LayoutEditor";
import {
  candidateDisplayLabel,
  compatibleHardwareProfiles,
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
  DeviceProfile,
  DeviceStatus,
  HomeMetricsSnapshot,
  Language,
  RuntimeAssignment,
  TriggerSettings,
} from "./types";

type Selection = { kind: "device" | "candidate"; id: string };
type Row = { selection: Selection };
type AssignmentDraft = { deviceProfileId: string; hardwareProfileId: string };
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
  deviceProfiles: DeviceProfile[];
  metrics: { deviceId: string; snapshot: HomeMetricsSnapshot } | null;
  onRename(deviceId: string, name: string): void | Promise<void>;
  onForget(deviceId: string): void | Promise<void>;
  onSaveRuntimeAssignment(
    deviceId: string,
    assignment: RuntimeAssignment,
  ): void | Promise<void>;
  onClearRuntimeAssignment(deviceId: string): void | Promise<void>;
  onMetricsChange(deviceId: string | null): void;
  onOpenSetup(targetId: string | null): void;
  onRetryCandidate(deviceId: string): void | Promise<void>;
  selectedDeviceId?: string | null;
  onSelectedDeviceChange?(deviceId: string | null): void;
  onChangeProfile?(profile: DeviceProfile): void;
  onSaveSharedProfile?(profile: DeviceProfile): void | Promise<void>;
  onDuplicateProfileForDevice?(request: { deviceId: string; sourceProfile: DeviceProfile; name: string }): Promise<void>;
  onHardwareSelectionChange?(hardwareProfileId: string | null, deviceId: string | null): void;
  onBeginLearning?(hardwareProfileId: string, deviceId: string, pins: number[]): void;
  onEndLearning?(deviceId: string): void;
}

function assignmentLabel(device: DeviceStatus, profiles: DeviceProfile[]) {
  if (!device.runtimeAssignment) return "-";
  const profile = profiles.find(
    (item) => item.profile.id === device.runtimeAssignment?.device_profile_id,
  );
  const hardware = profile?.hardware_profiles.find(
    (item) => item.id === device.runtimeAssignment?.hardware_profile_id,
  );
  if (device.assignment === "invalid_assignment") {
    return `${device.runtimeAssignment.device_profile_id} / ${device.runtimeAssignment.hardware_profile_id}`;
  }
  return `${profile?.profile.name ?? device.runtimeAssignment.device_profile_id} / ${hardware?.name ?? device.runtimeAssignment.hardware_profile_id}`;
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
  deviceProfiles,
  metrics,
  onRename,
  onForget,
  onSaveRuntimeAssignment,
  onClearRuntimeAssignment,
  onMetricsChange,
  onOpenSetup,
  onRetryCandidate,
  selectedDeviceId: controlledDeviceId,
  onSelectedDeviceChange,
  onChangeProfile,
  onSaveSharedProfile,
  onDuplicateProfileForDevice,
  onHardwareSelectionChange,
  onBeginLearning,
  onEndLearning,
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
  const [assignmentDraft, setAssignmentDraft] = useState<AssignmentDraft>({
    deviceProfileId: "",
    hardwareProfileId: "",
  });
  const [assignmentSaving, setAssignmentSaving] = useState(false);
  const [assignmentConfirmation, setAssignmentConfirmation] = useState<
    "save" | "clear" | null
  >(null);
  const [workspaceTab, setWorkspaceTab] = useState<"overview" | "io" | "layout">("overview");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [selectedButtonId, setSelectedButtonId] = useState<string | null>(null);
  const previous = useRef<Row[]>([]);
  const candidateRetryInFlight = useRef(false);
  const pendingAssignment = useRef<RuntimeAssignment | null>(null);
  const pendingClearAssignment = useRef(false);
  const assignmentMutationInFlight = useRef(false);
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
  const visibleDevices = devices.filter((device) =>
    matchesDeviceFilter(device, filter, "") &&
    matches([
      device.name,
      device.hardwareSerial,
      device.deviceId,
      device.boardProfileId,
      boards.get(device.boardProfileId)?.displayName ?? "",
      device.port ?? "",
    ], query),
  );
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
  useEffect(() => {
    const exists =
      selection &&
      rows.some(
        (row) =>
          row.selection.kind === selection.kind &&
          row.selection.id === selection.id,
      );
    if (!exists) {
      const index = selection
        ? previous.current.findIndex(
            (row) =>
              row.selection.kind === selection.kind &&
              row.selection.id === selection.id,
          )
        : 0;
      setSelection(
        rows[Math.max(0, Math.min(index, rows.length - 1))]?.selection ?? null,
      );
    }
    previous.current = rows;
  }, [rows, selection]);
  const activeSelection = selection ?? rows[0]?.selection ?? null;
  const selectedDevice =
    activeSelection?.kind === "device"
      ? (devices.find((device) => device.deviceId === activeSelection.id) ?? null)
      : null;
  useEffect(() => {
    if (!controlledDeviceId) return;
    if (devices.some((device) => device.deviceId === controlledDeviceId)) {
      setSelection({ kind: "device", id: controlledDeviceId });
    }
  }, [controlledDeviceId, devices]);
  useEffect(() => {
    if (selectedDevice && onSelectedDeviceChange) onSelectedDeviceChange(selectedDevice.deviceId);
  }, [onSelectedDeviceChange, selectedDevice?.deviceId]);
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
    const assignment = selectedDevice?.runtimeAssignment;
    const profile = deviceProfiles.find(
      (item) => item.profile.id === assignment?.device_profile_id,
    );
    const hardware = profile?.hardware_profiles.find(
      (item) => item.id === assignment?.hardware_profile_id,
    );
    setAssignmentDraft(
      profile && hardware && hardware.board_profile_id === selectedDevice?.boardProfileId
        ? {
            deviceProfileId: profile.profile.id,
            hardwareProfileId: hardware.id,
          }
        : { deviceProfileId: "", hardwareProfileId: "" },
    );
    pendingAssignment.current = null;
    pendingClearAssignment.current = false;
    setAssignmentConfirmation(null);
  }, [selectedDevice?.deviceId]);
  useEffect(() => {
    const assignment = selectedDevice?.runtimeAssignment;
    const saved = pendingAssignment.current;
    const saveConfirmed = saved &&
      assignment?.device_profile_id === saved.device_profile_id &&
      assignment.hardware_profile_id === saved.hardware_profile_id;
    if (saveConfirmed) {
      setAssignmentDraft({
        deviceProfileId: assignment.device_profile_id,
        hardwareProfileId: assignment.hardware_profile_id,
      });
      pendingAssignment.current = null;
    }
    if (pendingClearAssignment.current && selectedDevice && !assignment) {
      setAssignmentDraft({ deviceProfileId: "", hardwareProfileId: "" });
      pendingClearAssignment.current = false;
    }
  }, [selectedDevice?.runtimeAssignment]);
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
  const selectedDraftProfile = deviceProfiles.find(
    (item) => item.profile.id === assignmentDraft.deviceProfileId,
  );
  const compatibleHardware = selectedDraftProfile
    ? compatibleHardwareProfiles(
        selectedDraftProfile.hardware_profiles,
        selectedDevice?.boardProfileId ?? "",
      )
    : [];
  const selectedDraftHardware = compatibleHardware.find(
    (item) => item.id === assignmentDraft.hardwareProfileId,
  );
  const editingProfile = deviceProfiles.find((item) => item.profile.id === (assignmentDraft.deviceProfileId || selectedDevice?.runtimeAssignment?.device_profile_id));
  const sharedDeviceCount = editingProfile
    ? devices.filter((device) => device.runtimeAssignment?.device_profile_id === editingProfile.profile.id).length
    : 0;
  const updateEditingProfile = useCallback((next: DeviceProfile) => onChangeProfile?.(next), [onChangeProfile]);
  const handleWorkspaceSelection = useCallback((hardwareProfileId: string | null, deviceId: string | null) => {
    onHardwareSelectionChange?.(hardwareProfileId, deviceId);
  }, [onHardwareSelectionChange]);
  const handleBeginLearning = useCallback((hardwareProfileId: string, deviceId: string, pins: number[]) => {
    onBeginLearning?.(hardwareProfileId, deviceId, pins);
  }, [onBeginLearning]);
  const handleEndLearning = useCallback((deviceId: string) => {
    onEndLearning?.(deviceId);
  }, [onEndLearning]);
  const storedAssignmentProfile = deviceProfiles.find(
    (profile) =>
      profile.profile.id === selectedDevice?.runtimeAssignment?.device_profile_id,
  );
  const storedAssignmentHardware = storedAssignmentProfile?.hardware_profiles.find(
    (hardware) =>
      hardware.id === selectedDevice?.runtimeAssignment?.hardware_profile_id,
  );
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
  const saveAssignment = async () => {
    if (
      assignmentMutationInFlight.current ||
      !selectedDevice ||
      !selectedDraftProfile ||
      !selectedDraftHardware
    ) return;
    try {
      assignmentMutationInFlight.current = true;
      setAssignmentSaving(true);
      setError(null);
      pendingAssignment.current = {
        device_profile_id: selectedDraftProfile.profile.id,
        hardware_profile_id: selectedDraftHardware.id,
      };
      await onSaveRuntimeAssignment(selectedDevice.deviceId, {
        device_profile_id: selectedDraftProfile.profile.id,
        hardware_profile_id: selectedDraftHardware.id,
      });
      setAssignmentConfirmation(null);
    } catch (reason) {
      pendingAssignment.current = null;
      setError({
        owner: { kind: "device", id: selectedDevice.deviceId },
        message: errorMessage(reason),
      });
    } finally {
      assignmentMutationInFlight.current = false;
      setAssignmentSaving(false);
    }
  };
  const clearAssignment = async () => {
    if (assignmentMutationInFlight.current || !selectedDevice) return;
    try {
      assignmentMutationInFlight.current = true;
      setAssignmentSaving(true);
      setError(null);
      pendingClearAssignment.current = true;
      await onClearRuntimeAssignment(selectedDevice.deviceId);
      setAssignmentConfirmation(null);
    } catch (reason) {
      pendingClearAssignment.current = false;
      setError({
        owner: { kind: "device", id: selectedDevice.deviceId },
        message: errorMessage(reason),
      });
    } finally {
      assignmentMutationInFlight.current = false;
      setAssignmentSaving(false);
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
            <span>{t(language, "devices.assignment")}</span>
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
                    setSelection({ kind: "device", id: device.deviceId })
                  }
                >
                  <strong title={device.name}>{device.name}</strong>
                  <span title={boards.get(device.boardProfileId)?.displayName ?? device.boardProfileId}>
                    {boards.get(device.boardProfileId)?.displayName ??
                      device.boardProfileId}
                  </span>
                  <span title={status(device, language)}>{status(device, language)}</span>
                  <span title={assignmentLabel(device, deviceProfiles)}>{assignmentLabel(device, deviceProfiles)}</span>
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
                      setSelection({ kind: "candidate", id: candidate.key })
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
              label={t(language, "devices.serial")}
              value={selectedDevice.hardwareSerial}
            />
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
            <Detail
              label={t(language, "devices.assignment")}
              value={assignmentLabel(selectedDevice, deviceProfiles)}
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
            <section className="device-assignment" aria-label={t(language, "devices.assignment")}>
              <label>
                {t(language, "devices.useConfiguration")}
                <select
                  aria-label={t(language, "devices.useConfiguration")}
                  value={assignmentDraft.deviceProfileId}
                  disabled={assignmentSaving}
                  onChange={(event) => {
                    const deviceProfileId = event.target.value;
                    const profile = deviceProfiles.find((item) => item.profile.id === deviceProfileId);
                    const compatible = profile ? compatibleHardwareProfiles(profile.hardware_profiles, selectedDevice.boardProfileId) : [];
                    const currentHardware = profile?.hardware_profiles.find((item) => item.id === selectedDevice.runtimeAssignment?.hardware_profile_id);
                    setAssignmentDraft({ deviceProfileId, hardwareProfileId: currentHardware && currentHardware.board_profile_id === selectedDevice.boardProfileId ? currentHardware.id : compatible.length === 1 ? compatible[0].id : "" });
                  }}
                >
                  <option value="">{t(language, "model.select")}</option>
                  {deviceProfiles.map((profile) => <option key={profile.profile.id} value={profile.profile.id}>{profile.profile.name}</option>)}
                </select>
              </label>
              {!onChangeProfile && <label>
                {t(language, "model.label")}
                <select aria-label={t(language, "model.label")} value={assignmentDraft.deviceProfileId} disabled={assignmentSaving} onChange={(event) => {
                  const deviceProfileId = event.target.value;
                  const profile = deviceProfiles.find((item) => item.profile.id === deviceProfileId);
                  const hardware = profile ? compatibleHardwareProfiles(profile.hardware_profiles, selectedDevice.boardProfileId) : [];
                  setAssignmentDraft({ deviceProfileId, hardwareProfileId: hardware.length === 1 ? hardware[0].id : "" });
                }}>
                  <option value="">{t(language, "model.select")}</option>
                  {deviceProfiles.map((profile) => <option key={profile.profile.id} value={profile.profile.id}>{profile.profile.name}</option>)}
                </select>
              </label>}
              <label>
                {t(language, "hardware.title")}
                <select
                  aria-label={t(language, "hardware.title")}
                  value={assignmentDraft.hardwareProfileId}
                  disabled={!selectedDraftProfile || assignmentSaving}
                  onChange={(event) =>
                    setAssignmentDraft((current) => ({
                      ...current,
                      hardwareProfileId: event.target.value,
                    }))
                  }
                >
                  <option value="">{t(language, "model.select")}</option>
                  {compatibleHardware.map((hardware) => (
                    <option key={hardware.id} value={hardware.id}>
                      {hardware.name}
                    </option>
                  ))}
                </select>
              </label>
              {selectedDraftProfile && compatibleHardware.length === 0 && (
                <p className="field-error">{t(language, "devices.noCompatibleHardware")}</p>
              )}
              {editingProfile && compatibleHardware.length > 1 && !selectedDraftHardware && (
                <div className="required-hardware-resolver" role="alert">
                  <p>{t(language, "devices.requiredHardware")}</p>
                  <select aria-label={t(language, "devices.requiredHardware")} value={assignmentDraft.hardwareProfileId} onChange={(event) => setAssignmentDraft((current) => ({ ...current, hardwareProfileId: event.target.value }))}>
                    <option value="">{t(language, "model.select")}</option>
                    {compatibleHardware.map((hardware) => <option key={hardware.id} value={hardware.id}>{hardware.name}</option>)}
                  </select>
                  <button className="secondary-button" type="button" disabled={!assignmentDraft.hardwareProfileId} onClick={() => setAssignmentDraft((current) => ({ ...current }))}>{t(language, "devices.applyHardware")}</button>
                </div>
              )}
              <button
                className="primary-button"
                type="button"
                disabled={!selectedDraftHardware || assignmentSaving}
                onClick={() => setAssignmentConfirmation("save")}
              >
                {t(language, "devices.saveAssignment")}
              </button>
              <button
                className="danger-button"
                type="button"
                disabled={!selectedDevice.runtimeAssignment || assignmentSaving}
                onClick={() => setAssignmentConfirmation("clear")}
              >
                {t(language, "devices.clearAssignment")}
              </button>
            </section>
            {editingProfile && (
              <section className="device-workspace" aria-label={t(language, "devices.configurationSettings")}>
                {selectedDevice.connection === "offline" && <p className="form-hint">{t(language, "devices.offlineEditing")}</p>}
                <div className="device-workspace-toolbar">
                  <button className="secondary-button" type="button" aria-label={t(language, "devices.configurationSettings")} onClick={() => setSettingsOpen(true)}>{t(language, "devices.configurationSettings")}</button>
                  <div className="device-workspace-tabs" role="tablist" aria-label={t(language, "devices.detail")}>
                    <button type="button" role="tab" aria-selected={workspaceTab === "overview"} onClick={() => setWorkspaceTab("overview")}>{t(language, "devices.workspaceOverview")}</button>
                    <button type="button" role="tab" aria-selected={workspaceTab === "io"} onClick={() => setWorkspaceTab("io")}>{t(language, "devices.workspaceIo")}</button>
                    <button type="button" role="tab" aria-selected={workspaceTab === "layout"} onClick={() => setWorkspaceTab("layout")}>{t(language, "devices.workspaceLayout")}</button>
                  </div>
                </div>
                {(workspaceTab === "io" || workspaceTab === "layout") && sharedDeviceCount > 1 && (
                  <div className="shared-configuration-warning" role="status">
                    {t(language, "devices.sharedWarning", { name: editingProfile.profile.name, count: sharedDeviceCount })}
                    <button className="secondary-button" type="button" onClick={() => void onSaveSharedProfile?.(editingProfile)}>{t(language, "devices.saveShared")}</button>
                    <button className="secondary-button" type="button" onClick={() => void onDuplicateProfileForDevice?.({ deviceId: selectedDevice.deviceId, sourceProfile: editingProfile, name: `${editingProfile.profile.name} (${selectedDevice.name})` })}>{t(language, "devices.duplicateForDevice")}</button>
                  </div>
                )}
                {workspaceTab === "overview" && <p className="form-hint">{t(language, "devices.sharedWarning", { name: editingProfile.profile.name, count: sharedDeviceCount || 1 })}</p>}
                {workspaceTab === "io" && <div role="tabpanel" aria-label={t(language, "devices.workspaceIo")}>
                  <h3>{t(language, "hardware.title")}</h3>
                  <HardwareMapping language={language} layout={editingProfile.profile} hardwareProfiles={editingProfile.hardware_profiles} boardProfiles={boardProfiles} devices={devices} learning={selectedDevice.learning} initialHardwareProfileId={assignmentDraft.hardwareProfileId || selectedDevice.runtimeAssignment?.hardware_profile_id} initialDeviceId={selectedDevice.deviceId} selectedButtonId={selectedButtonId} onSelectButton={setSelectedButtonId} onChange={(hardwareProfiles) => updateEditingProfile({ ...editingProfile, hardware_profiles: hardwareProfiles })} onSelectionChange={handleWorkspaceSelection} onBeginLearning={handleBeginLearning} onEndLearning={handleEndLearning} />
                </div>}
                {workspaceTab === "layout" && <div role="tabpanel" aria-label={t(language, "devices.workspaceLayout")}><LayoutEditor language={language} layout={editingProfile.profile} onChange={(layout) => updateEditingProfile({ ...editingProfile, profile: layout })} /></div>}
              </section>
            )}
            <details className="device-technical-details">
              <summary>{t(language, "setup.technicalDetails")}</summary>
              <dl>
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
                          <td>{log.deviceProfileId}</td>
                          <td>{log.hardwareProfileId}</td>
                          <td>{log.message}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </>
            )}
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
      {assignmentConfirmation && selectedDevice && (
        <ConfirmDialog
          title={t(
            language,
            assignmentConfirmation === "save"
              ? "devices.saveAssignment"
              : "devices.clearAssignment",
          )}
          body={t(language, "devices.assignmentConfirm", {
            device: selectedDevice.name,
            deviceProfile:
              (assignmentConfirmation === "save"
                ? selectedDraftProfile?.profile.name
                : selectedDevice.assignment === "invalid_assignment"
                  ? undefined
                  : storedAssignmentProfile?.profile.name) ??
              selectedDevice.runtimeAssignment?.device_profile_id ??
              "-",
            hardwareProfile:
              (assignmentConfirmation === "save"
                ? selectedDraftHardware?.name
                : selectedDevice.assignment === "invalid_assignment"
                  ? undefined
                  : storedAssignmentHardware?.name) ??
              selectedDevice.runtimeAssignment?.hardware_profile_id ??
              "-",
          })}
          confirmLabel={t(language, "common.confirm")}
          cancelLabel={t(language, "common.cancel")}
          danger={assignmentConfirmation === "clear"}
          pending={assignmentSaving}
          onCancel={() => setAssignmentConfirmation(null)}
          onConfirm={() =>
            void (assignmentConfirmation === "save"
              ? saveAssignment()
              : clearAssignment())
          }
        />
      )}
      {editingProfile && <ConfigurationSettingsDialog open={settingsOpen} language={language} profile={editingProfile} sharedDeviceCount={sharedDeviceCount} onCancel={() => setSettingsOpen(false)} onSave={(settings: TriggerSettings) => { updateEditingProfile({ ...editingProfile, trigger_settings: settings }); setSettingsOpen(false); void onSaveSharedProfile?.({ ...editingProfile, trigger_settings: settings }); }} onDraftChange={(settings) => updateEditingProfile({ ...editingProfile, trigger_settings: settings })} onDuplicate={async (name) => { if (selectedDevice) await onDuplicateProfileForDevice?.({ deviceId: selectedDevice.deviceId, sourceProfile: { ...editingProfile }, name }); setSettingsOpen(false); }} />}
    </div>
  );
}
