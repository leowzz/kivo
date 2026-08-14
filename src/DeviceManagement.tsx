import { Check, Pencil, RefreshCw, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ActionEditor } from "./ActionEditor";
import { ConfigurationSettingsDialog } from "./ConfigurationSettingsDialog";
import { HardwareMapping } from "./HardwareMapping";
import { Keypad } from "./Keypad";
import { LayoutEditor } from "./LayoutEditor";
import {
  candidateDisplayLabel,
  compatibleHardwareProfiles,
  primaryDeviceLabel,
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
  TriggerActions,
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
  onSaveRuntimeAssignment(
    deviceId: string,
    assignment: RuntimeAssignment,
  ): void | Promise<void>;
  onMetricsChange(deviceId: string | null): void;
  onOpenSetup(targetId: string | null): void;
  onRetryCandidate(deviceId: string): void | Promise<void>;
  selectedDeviceId?: string | null;
  onSelectedDeviceChange?(deviceId: string | null): void;
  onChangeProfile?(profile: DeviceProfile): void;
  onChangeActions?(profile: DeviceProfile): void;
  onSaveSharedProfile?(profile: DeviceProfile): void | Promise<void>;
  onDuplicateProfileForDevice?(request: { deviceId: string; sourceProfile: DeviceProfile; name: string }): Promise<void>;
  onHardwareSelectionChange?(hardwareProfileId: string | null, deviceId: string | null): void;
  onBeginLearning?(hardwareProfileId: string, deviceId: string, pins: number[]): void;
  onEndLearning?(deviceId: string): void;
  selectedButtonId: string | null;
  onSelectedButtonChange(buttonId: string | null): void;
  pressedButtonIds: Set<string>;
}

function assignmentLabel(device: DeviceStatus, profiles: DeviceProfile[]) {
  if (!device.runtimeAssignment) return "-";
  const profile = profiles.find(
    (item) => item.profile.id === device.runtimeAssignment?.device_profile_id,
  );
  if (device.assignment === "invalid_assignment") {
    return device.runtimeAssignment.device_profile_id;
  }
  return profile?.profile.name ?? device.runtimeAssignment.device_profile_id;
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
  onSaveRuntimeAssignment,
  onMetricsChange,
  onOpenSetup,
  onRetryCandidate,
  selectedDeviceId: controlledDeviceId,
  onSelectedDeviceChange,
  onChangeProfile,
  onChangeActions,
  onSaveSharedProfile,
  onDuplicateProfileForDevice,
  onHardwareSelectionChange,
  onBeginLearning,
  onEndLearning,
  selectedButtonId,
  onSelectedButtonChange,
  pressedButtonIds,
}: DeviceManagementProps) {
  const [selection, setSelection] = useState<Selection | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [name, setName] = useState("");
  const [error, setError] = useState<OperationError | null>(null);
  const [candidateRetrying, setCandidateRetrying] = useState(false);
  const [assignmentDraft, setAssignmentDraft] = useState<AssignmentDraft>({
    deviceProfileId: "",
    hardwareProfileId: "",
  });
  const [assignmentSaving, setAssignmentSaving] = useState(false);
  const [workspaceTab, setWorkspaceTab] = useState<"buttons" | "overview" | "io" | "layout">("buttons");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const previous = useRef<Row[]>([]);
  const candidateRetryInFlight = useRef(false);
  const pendingAssignment = useRef<RuntimeAssignment | null>(null);
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
  const visibleDevices = devices.filter((device) => device.connection === "online");
  const visibleCandidates = candidates;
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
  const activeDeviceId =
    activeSelection?.kind === "device" ? activeSelection.id : null;
  useEffect(() => {
    if (activeDeviceId !== (controlledDeviceId ?? null)) {
      onSelectedDeviceChange?.(activeDeviceId);
    }
  }, [activeDeviceId, controlledDeviceId, onSelectedDeviceChange]);
  const selectRow = (next: Selection) => {
    setSelection(next);
    onSelectedDeviceChange?.(next.kind === "device" ? next.id : null);
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
  useEffect(() => {
    onMetricsChange(selectedDevice?.deviceId ?? null);
  }, [onMetricsChange, selectedDevice?.deviceId]);
  useEffect(() => {
    setRenaming(false);
    setName(selectedDevice?.name ?? "");
    setWorkspaceTab("buttons");
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
  }, [selectedDevice?.runtimeAssignment]);
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
  const selectedDraftProfile = deviceProfiles.find(
    (item) => item.profile.id === assignmentDraft.deviceProfileId,
  );
  const compatibleHardware = selectedDraftProfile
    ? compatibleHardwareProfiles(
        selectedDraftProfile.hardware_profiles,
        selectedDevice?.boardProfileId ?? "",
      )
    : [];
  const editingProfile = deviceProfiles.find((item) => item.profile.id === (assignmentDraft.deviceProfileId || selectedDevice?.runtimeAssignment?.device_profile_id));
  const buttons = editingProfile?.profile.groups.flatMap((group) => group.buttons) ?? [];
  const selectedButton = buttons.find((button) => button.id === selectedButtonId) ?? null;
  const selectedActions: TriggerActions = editingProfile && selectedButtonId
    ? editingProfile.actions[selectedButtonId] ?? {
        press: [],
        release: [],
        long_press: [],
        double_press: [],
      }
    : { press: [], release: [], long_press: [], double_press: [] };
  const sharedDeviceCount = editingProfile
    ? devices.filter((device) => device.runtimeAssignment?.device_profile_id === editingProfile.profile.id).length
    : 0;
  const updateEditingProfile = useCallback((next: DeviceProfile) => onChangeProfile?.(next), [onChangeProfile]);
  const updateActionProfile = useCallback(
    (next: DeviceProfile) => (onChangeActions ?? onChangeProfile)?.(next),
    [onChangeActions, onChangeProfile],
  );
  useEffect(() => {
    if (!buttons.some((button) => button.id === selectedButtonId)) {
      onSelectedButtonChange(buttons[0]?.id ?? null);
    }
  }, [buttons, onSelectedButtonChange, selectedButtonId]);
  useEffect(() => {
    if (editingProfile && buttons.length === 0 && workspaceTab === "buttons") {
      setWorkspaceTab("overview");
    }
  }, [buttons.length, editingProfile, workspaceTab]);
  const handleWorkspaceSelection = useCallback((hardwareProfileId: string | null, deviceId: string | null) => {
    onHardwareSelectionChange?.(hardwareProfileId, deviceId);
  }, [onHardwareSelectionChange]);
  const handleBeginLearning = useCallback((hardwareProfileId: string, deviceId: string, pins: number[]) => {
    onBeginLearning?.(hardwareProfileId, deviceId, pins);
  }, [onBeginLearning]);
  const handleEndLearning = useCallback((deviceId: string) => {
    onEndLearning?.(deviceId);
  }, [onEndLearning]);
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
  const selectAssignment = async (deviceProfileId: string) => {
    if (!deviceProfileId) return;
    const profile = deviceProfiles.find(
      (item) => item.profile.id === deviceProfileId,
    );
    const compatible = profile
      ? compatibleHardwareProfiles(
          profile.hardware_profiles,
          selectedDevice?.boardProfileId ?? "",
        )
      : [];
    const currentHardware = profile?.hardware_profiles.find(
      (item) =>
        item.id === selectedDevice?.runtimeAssignment?.hardware_profile_id &&
        item.board_profile_id === selectedDevice.boardProfileId,
    );
    const nextDraft = {
      deviceProfileId,
      hardwareProfileId: currentHardware?.id ?? compatible[0]?.id ?? "",
    };
    setAssignmentDraft(nextDraft);

    if (
      assignmentMutationInFlight.current ||
      !selectedDevice ||
      !profile ||
      !nextDraft.hardwareProfileId ||
      (selectedDevice.runtimeAssignment?.device_profile_id === deviceProfileId &&
        selectedDevice.runtimeAssignment.hardware_profile_id ===
          nextDraft.hardwareProfileId)
    ) return;
    const assignment = {
      device_profile_id: deviceProfileId,
      hardware_profile_id: nextDraft.hardwareProfileId,
    };
    try {
      assignmentMutationInFlight.current = true;
      setAssignmentSaving(true);
      setError(null);
      pendingAssignment.current = assignment;
      await onSaveRuntimeAssignment(selectedDevice.deviceId, assignment);
    } catch (reason) {
      pendingAssignment.current = null;
      const currentProfile = deviceProfiles.find(
        (item) =>
          item.profile.id ===
          selectedDevice.runtimeAssignment?.device_profile_id,
      );
      const currentHardwareProfile = currentProfile?.hardware_profiles.find(
        (item) =>
          item.id === selectedDevice.runtimeAssignment?.hardware_profile_id &&
          item.board_profile_id === selectedDevice.boardProfileId,
      );
      setAssignmentDraft(
        currentProfile && currentHardwareProfile
          ? {
              deviceProfileId: currentProfile.profile.id,
              hardwareProfileId: currentHardwareProfile.id,
            }
          : { deviceProfileId: "", hardwareProfileId: "" },
      );
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
    <div className={`device-management ${editingProfile && workspaceTab === "buttons" ? "is-workspace" : ""}`}>
      <section
        className="device-list-region"
        aria-label={t(language, "devices.list")}
      >
        <header className="device-list-header">
          <h2>{t(language, "nav.devices")}</h2>
        </header>
        <div className="connected-device-list">
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
                  <strong title={boards.get(device.boardProfileId)?.displayName ?? device.boardProfileId}>
                    {boards.get(device.boardProfileId)?.displayName ??
                      device.boardProfileId}
                  </strong>
                  <span className="device-row-meta">
                    <span className="device-identifier" title={device.hardwareSerial}>{device.hardwareSerial}</span>
                    <span className="device-connection-state"><i aria-hidden="true" />{t(language, "devices.connected")}</span>
                    <span aria-hidden="true">·</span>
                    <span className={`device-availability ${device.runtime === "ready" ? "is-available" : ""}`}>
                      {device.runtime === "ready" ? t(language, "devices.available") : status(device, language)}
                    </span>
                  </span>
                </button>
              </li>
            ))}
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
                    <strong title={boards.get(candidate.boardProfileId)?.displayName ?? candidate.boardProfileId}>
                      {boards.get(candidate.boardProfileId)?.displayName ??
                        candidate.boardProfileId}
                    </strong>
                    <span className="device-row-meta">
                      <span className="device-identifier" title={candidate.rawSerial ?? candidateLabels.get(candidate.key)}>{candidateLabels.get(candidate.key)}</span>
                      <span className="device-connection-state"><i aria-hidden="true" />{t(language, "devices.connected")}</span>
                      <span aria-hidden="true">·</span>
                      <span className="device-availability is-attention">{t(language, "devices.needsSetup")}</span>
                    </span>
                  </button>
                </li>
              ))}
          </ul>
          {visibleDevices.length === 0 && visibleCandidates.length === 0 && (
            <p className="device-list-empty">{t(language, "devices.noConnected")}</p>
          )}
        </div>
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
            {editingProfile && (
              <div className="device-workspace-toolbar is-primary">
                <div className="device-workspace-tabs" role="tablist" aria-label={t(language, "devices.detail")}>
                  <button type="button" role="tab" aria-selected={workspaceTab === "buttons"} onClick={() => setWorkspaceTab("buttons")}>{t(language, "devices.workspaceButtons")}</button>
                  <button type="button" role="tab" aria-selected={workspaceTab === "overview"} onClick={() => setWorkspaceTab("overview")}>{t(language, "devices.workspaceOverview")}</button>
                  <button type="button" role="tab" aria-selected={workspaceTab === "layout"} onClick={() => setWorkspaceTab("layout")}>{t(language, "devices.workspaceLayout")}</button>
                  <button type="button" role="tab" aria-selected={workspaceTab === "io"} onClick={() => setWorkspaceTab("io")}>{t(language, "devices.workspaceIo")}</button>
                </div>
                <button className="secondary-button" type="button" aria-label={t(language, "devices.configurationSettings")} onClick={() => setSettingsOpen(true)}>{t(language, "devices.configurationSettings")}</button>
              </div>
            )}
            {(!editingProfile || workspaceTab === "overview") && <>
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
                    onChange={(event) => void selectAssignment(event.target.value)}
                  >
                    <option value="">{t(language, "model.select")}</option>
                    {deviceProfiles.map((profile) => <option key={profile.profile.id} value={profile.profile.id}>{profile.profile.name}</option>)}
                  </select>
                </label>
                {!onChangeProfile && <label>
                  {t(language, "model.label")}
                  <select aria-label={t(language, "model.label")} value={assignmentDraft.deviceProfileId} disabled={assignmentSaving} onChange={(event) => void selectAssignment(event.target.value)}>
                    <option value="">{t(language, "model.select")}</option>
                    {deviceProfiles.map((profile) => <option key={profile.profile.id} value={profile.profile.id}>{profile.profile.name}</option>)}
                  </select>
                </label>}
                {selectedDraftProfile && compatibleHardware.length === 0 && (
                  <p className="field-error">{t(language, "devices.incompatibleProfile")}</p>
                )}
              </section>
            </>}
            {editingProfile && workspaceTab === "buttons" && (
              <div className="keypad-stage device-keypad-stage" role="tabpanel" aria-label={t(language, "devices.workspaceButtons")}>
                <Keypad
                  layout={editingProfile.profile}
                  actions={editingProfile.actions}
                  selectedButtonId={selectedButtonId}
                  pressedButtonIds={pressedButtonIds}
                  actionCountLabel={(count) => t(language, "model.actionCount", { count })}
                  onSelect={onSelectedButtonChange}
                />
              </div>
            )}
            {editingProfile && (workspaceTab === "layout" || workspaceTab === "io") && (
              <section className="device-workspace" aria-label={t(language, "devices.configurationSettings")}>
                {selectedDevice.connection === "offline" && <p className="form-hint">{t(language, "devices.offlineEditing")}</p>}
                {(workspaceTab === "io" || workspaceTab === "layout") && sharedDeviceCount > 1 && (
                  <div className="shared-configuration-warning" role="status">
                    {t(language, "devices.sharedWarning", { name: editingProfile.profile.name, count: sharedDeviceCount })}
                    <button className="secondary-button" type="button" onClick={() => void onSaveSharedProfile?.(editingProfile)}>{t(language, "devices.saveShared")}</button>
                    <button className="secondary-button" type="button" onClick={() => void onDuplicateProfileForDevice?.({ deviceId: selectedDevice.deviceId, sourceProfile: editingProfile, name: `${editingProfile.profile.name} (${selectedDevice.name})` })}>{t(language, "devices.duplicateForDevice")}</button>
                  </div>
                )}
                {workspaceTab === "io" && <div role="tabpanel" aria-label={t(language, "devices.workspaceIo")}>
                  <h3>{t(language, "hardware.title")}</h3>
                  <HardwareMapping language={language} layout={editingProfile.profile} hardwareProfiles={editingProfile.hardware_profiles} boardProfiles={boardProfiles} devices={devices} learning={selectedDevice.learning} initialHardwareProfileId={assignmentDraft.hardwareProfileId || selectedDevice.runtimeAssignment?.hardware_profile_id} initialDeviceId={selectedDevice.deviceId} selectedButtonId={selectedButtonId} onSelectButton={onSelectedButtonChange} onChange={(hardwareProfiles) => updateEditingProfile({ ...editingProfile, hardware_profiles: hardwareProfiles })} onSelectionChange={handleWorkspaceSelection} onBeginLearning={handleBeginLearning} onEndLearning={handleEndLearning} />
                </div>}
                {workspaceTab === "layout" && <div role="tabpanel" aria-label={t(language, "devices.workspaceLayout")}><LayoutEditor language={language} layout={editingProfile.profile} onChange={(layout) => updateEditingProfile({ ...editingProfile, profile: layout })} /></div>}
              </section>
            )}
            {(!editingProfile || workspaceTab === "overview") && (
              <div
                role={editingProfile ? "tabpanel" : undefined}
                aria-label={editingProfile ? t(language, "devices.workspaceOverview") : undefined}
              >
                {editingProfile && <p className="form-hint">{t(language, "devices.sharedWarning", { name: editingProfile.profile.name, count: sharedDeviceCount || 1 })}</p>}
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
              </div>
            )}
            {selectedError && (
              <p className="field-error" role="alert">
                {selectedError}
              </p>
            )}
          </>
        )}
      </aside>
      {editingProfile && workspaceTab === "buttons" && (
        <ActionEditor
          language={language}
          button={selectedButton}
          actions={selectedActions}
          onChange={(actions) => {
            if (!selectedButtonId) return;
            updateActionProfile({
              ...editingProfile,
              actions: {
                ...editingProfile.actions,
                [selectedButtonId]: actions,
              },
            });
          }}
          onRename={(buttonId, label) => updateActionProfile({
            ...editingProfile,
            profile: {
              ...editingProfile.profile,
              groups: editingProfile.profile.groups.map((group) => ({
                ...group,
                buttons: group.buttons.map((button) =>
                  button.id === buttonId ? { ...button, label } : button
                ),
              })),
            },
          })}
        />
      )}
      {editingProfile && <ConfigurationSettingsDialog open={settingsOpen} language={language} profile={editingProfile} sharedDeviceCount={sharedDeviceCount} onCancel={() => setSettingsOpen(false)} onSave={(settings: TriggerSettings) => { updateEditingProfile({ ...editingProfile, trigger_settings: settings }); setSettingsOpen(false); void onSaveSharedProfile?.({ ...editingProfile, trigger_settings: settings }); }} onDraftChange={(settings) => updateEditingProfile({ ...editingProfile, trigger_settings: settings })} onDuplicate={async (name) => { if (selectedDevice) await onDuplicateProfileForDevice?.({ deviceId: selectedDevice.deviceId, sourceProfile: { ...editingProfile }, name }); setSettingsOpen(false); }} />}
    </div>
  );
}
