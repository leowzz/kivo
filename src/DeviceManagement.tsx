import { Check, Pencil, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { ConfirmDialog } from "./ConfirmDialog";
import {
  compatibleHardwareProfiles,
  matchesDeviceFilter,
  primaryDeviceLabel,
  type DeviceFilter,
} from "./deviceStatus";
import { t } from "./i18n";
import type {
  BoardProfileSummary,
  CandidateStatus,
  DeviceProfile,
  DeviceStatus,
  HomeMetricsSnapshot,
  Language,
  RuntimeAssignment,
} from "./types";

type Selection = { kind: "device" | "candidate"; id: string };
type Row = { selection: Selection };
type AssignmentDraft = { deviceProfileId: string; hardwareProfileId: string };
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
function Detail({ label, value }: { label: string; value: string }) {
  return (
    <div className="device-detail-field">
      <span>{label}</span>
      <output>{value}</output>
    </div>
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
}: DeviceManagementProps) {
  const [filter, setFilter] = useState<DeviceFilter>("all");
  const [query, setQuery] = useState("");
  const [selection, setSelection] = useState<Selection | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [name, setName] = useState("");
  const [confirmId, setConfirmId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [forgetting, setForgetting] = useState(false);
  const [assignmentDraft, setAssignmentDraft] = useState<AssignmentDraft>({
    deviceProfileId: "",
    hardwareProfileId: "",
  });
  const [assignmentSaving, setAssignmentSaving] = useState(false);
  const [assignmentConfirmation, setAssignmentConfirmation] = useState<
    "save" | "clear" | null
  >(null);
  const previous = useRef<Row[]>([]);
  const pendingAssignment = useRef<RuntimeAssignment | null>(null);
  const pendingClearAssignment = useRef(false);
  const assignmentMutationInFlight = useRef(false);
  const boards = useMemo(
    () => new Map(boardProfiles.map((board) => [board.id, board])),
    [boardProfiles],
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
  const selectedDevice =
    selection?.kind === "device"
      ? (devices.find((device) => device.deviceId === selection.id) ?? null)
      : null;
  const selectedCandidate =
    selection?.kind === "candidate"
      ? (candidates.find((candidate) => candidate.key === selection.id) ?? null)
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
      setError(String(reason));
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
      setError(String(reason));
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
  const storedAssignmentProfile = deviceProfiles.find(
    (profile) =>
      profile.profile.id === selectedDevice?.runtimeAssignment?.device_profile_id,
  );
  const storedAssignmentHardware = storedAssignmentProfile?.hardware_profiles.find(
    (hardware) =>
      hardware.id === selectedDevice?.runtimeAssignment?.hardware_profile_id,
  );
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
      setError(String(reason));
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
      setError(String(reason));
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
            <span>{t(language, "devices.port")}</span>
          </div>
          <ul>
            {visibleDevices.map((device) => (
              <li key={device.deviceId}>
                <button
                  className={`device-row ${selection?.kind === "device" && selection.id === device.deviceId ? "is-selected" : ""}`}
                  type="button"
                  aria-pressed={
                    selection?.kind === "device" &&
                    selection.id === device.deviceId
                  }
                  onClick={() =>
                    setSelection({ kind: "device", id: device.deviceId })
                  }
                >
                  <strong>{device.name}</strong>
                  <span>
                    {boards.get(device.boardProfileId)?.displayName ??
                      device.boardProfileId}
                  </span>
                  <span>{status(device, language)}</span>
                  <span>{assignmentLabel(device, deviceProfiles)}</span>
                  <span>{device.port ?? "-"}</span>
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
                    className={`device-row candidate-row ${selection?.kind === "candidate" && selection.id === candidate.key ? "is-selected" : ""}`}
                    type="button"
                    aria-pressed={
                      selection?.kind === "candidate" &&
                      selection.id === candidate.key
                    }
                    onClick={() =>
                      setSelection({ kind: "candidate", id: candidate.key })
                    }
                  >
                    <strong>{candidate.rawSerial ?? candidate.key}</strong>
                    <span>
                      {boards.get(candidate.boardProfileId)?.displayName ??
                        candidate.boardProfileId}
                    </span>
                    <span>{t(language, "devices.filter.attention")}</span>
                    <span>-</span>
                    <span>{candidate.port ?? "-"}</span>
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
            <Detail
              label={t(language, "devices.serial")}
              value={selectedCandidate.rawSerial ?? "-"}
            />
            <Detail
              label={t(language, "devices.board")}
              value={
                boards.get(selectedCandidate.boardProfileId)?.displayName ??
                selectedCandidate.boardProfileId
              }
            />
            <Detail
              label={t(language, "devices.controller")}
              value={selectedCandidate.controllerFamilyId}
            />
            <Detail
              label={t(language, "devices.mode")}
              value={selectedCandidate.mode}
            />
            <Detail
              label={t(language, "devices.port")}
              value={selectedCandidate.port ?? "-"}
            />
            <Detail
              label={t(language, "devices.error")}
              value={selectedCandidate.latestError ?? "-"}
            />
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
                  onClick={() => void rename()}
                >
                  <Check size={16} />
                </button>
                <button
                  className="icon-button"
                  type="button"
                  aria-label={t(language, "common.cancel")}
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
              label={t(language, "devices.id")}
              value={selectedDevice.deviceId}
            />
            <Detail
              label={t(language, "devices.controller")}
              value={selectedDevice.controllerFamilyId}
            />
            <Detail
              label={t(language, "devices.board")}
              value={
                boards.get(selectedDevice.boardProfileId)?.displayName ??
                selectedDevice.boardProfileId
              }
            />
            <Detail
              label={t(language, "devices.mode")}
              value={selectedDevice.mode ?? "-"}
            />
            <Detail
              label={t(language, "devices.port")}
              value={selectedDevice.port ?? "-"}
            />
            <Detail
              label={t(language, "devices.firmware")}
              value={selectedDevice.firmwareBuildId ?? "-"}
            />
            <Detail
              label={t(language, "devices.pins")}
              value={selectedDevice.capabilities.join(", ") || "-"}
            />
            <Detail
              label={t(language, "devices.assignment")}
              value={assignmentLabel(selectedDevice, deviceProfiles)}
            />
            <section className="device-assignment" aria-label={t(language, "devices.assignment")}>
              <label>
                {t(language, "model.label")}
                <select
                  aria-label={t(language, "model.label")}
                  value={assignmentDraft.deviceProfileId}
                  disabled={assignmentSaving}
                  onChange={(event) => {
                    const deviceProfileId = event.target.value;
                    const profile = deviceProfiles.find(
                      (item) => item.profile.id === deviceProfileId,
                    );
                    const hardware = profile
                      ? compatibleHardwareProfiles(
                          profile.hardware_profiles,
                          selectedDevice.boardProfileId,
                        )
                      : [];
                    setAssignmentDraft({
                      deviceProfileId,
                      hardwareProfileId:
                        hardware.length === 1 ? hardware[0].id : "",
                    });
                  }}
                >
                  <option value="">{t(language, "model.select")}</option>
                  {deviceProfiles.map((profile) => (
                    <option key={profile.profile.id} value={profile.profile.id}>
                      {profile.profile.name}
                    </option>
                  ))}
                </select>
              </label>
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
              <button
                type="button"
                disabled={!selectedDraftHardware || assignmentSaving}
                onClick={() => setAssignmentConfirmation("save")}
              >
                {t(language, "devices.saveAssignment")}
              </button>
              <button
                type="button"
                disabled={!selectedDevice.runtimeAssignment || assignmentSaving}
                onClick={() => setAssignmentConfirmation("clear")}
              >
                {t(language, "devices.clearAssignment")}
              </button>
            </section>
            <Detail
              label={t(language, "devices.error")}
              value={selectedDevice.latestError?.detail ?? "-"}
            />
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
                        <tr key={`${log.timestampMs}:${log.deviceId}:${log.message}`}>
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
            {error && (
              <p className="field-error" role="alert">
                {error}
              </p>
            )}
            <button
              className="icon-button is-danger device-forget"
              type="button"
              aria-label={t(language, "devices.forget")}
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
    </div>
  );
}
