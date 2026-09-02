import { Check, Keyboard, Pencil, Plus, RefreshCw, Settings2, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { ActionEditor } from "./ActionEditor";
import { ConfigurationSettingsDialog } from "./ConfigurationSettingsDialog";
import { HardwareMapping } from "./HardwareMapping";
import { Keypad } from "./Keypad";
import { LayoutEditor } from "./LayoutEditor";
import { reconcileProfileLayout } from "./profileEditing";
import {
  candidateDisplayLabel,
  compatibleHardwareProfiles,
  deviceSummary,
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
  Language,
  ProductConfigurationProfile,
  RuntimeAssignment,
  TriggerActions,
  TriggerSettings,
} from "./types";

type Selection = { kind: "device" | "candidate"; id: string };
type Row = { selection: Selection };
type AssignmentDraft = { deviceProfileId: string; hardwareProfileId: string };
type OperationError = { owner: Selection; message: string };
type AdvancedTab = "layout" | "io";
export type DeviceExecutionFeedback = {
  buttonId: string | null;
  status: "success" | "error";
  detail: string | null;
};
const ADVANCED_TABS: readonly AdvancedTab[] = ["layout", "io"];
const STARTER_PROFILE_IDS = new Set([
  "daily-shortcuts",
  "creator-workspace",
  "phone-numeric-terminal",
]);

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
    title: "setup.candidate.validating.title",
    body: "setup.candidate.validating.body",
  },
  firmware_not_responding: {
    title: "setup.candidate.firmware_not_responding.title",
    body: "setup.candidate.firmware_not_responding.body",
  },
  firmware_incompatible: {
    title: "setup.candidate.firmware_incompatible.title",
    body: "setup.candidate.firmware_incompatible.body",
  },
  bootloader: {
    title: "setup.candidate.bootloader.title",
    body: "setup.candidate.bootloader.body",
  },
  port_unavailable: {
    title: "setup.candidate.port_unavailable.title",
    body: "setup.candidate.port_unavailable.body",
  },
  invalid_identity: {
    title: "setup.candidate.invalid_identity.title",
    body: "setup.candidate.invalid_identity.body",
  },
  duplicate_identity: {
    title: "setup.candidate.duplicate_identity.title",
    body: "setup.candidate.duplicate_identity.body",
  },
  unknown: {
    title: "setup.candidate.unknown.title",
    body: "setup.candidate.unknown.body",
  },
};

interface DeviceManagementProps {
  client?: boolean;
  studioMode?: boolean;
  language: Language;
  devices: DeviceStatus[];
  candidates: CandidateStatus[];
  boardProfiles: BoardProfileSummary[];
  deviceProfiles: DeviceProfile[];
  productConfigurations?: ProductConfigurationProfile[];
  onRename(deviceId: string, name: string): void | Promise<void>;
  onSaveRuntimeAssignment(
    deviceId: string,
    assignment: RuntimeAssignment,
  ): void | Promise<void>;
  onSelectProductConfiguration?(deviceId: string, configurationId: string): Promise<void>;
  onCreateProductConfiguration?(request: { deviceId: string; name: string; copyCurrent: boolean }): Promise<void>;
  onForgetDevice?(deviceId: string): void;
  onOpenSetup(targetId: string | null): void;
  onCreateFromTemplate?(profileId: string): void;
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
  executionFeedback?: DeviceExecutionFeedback | null;
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
  client = false,
  studioMode = false,
  language,
  devices,
  candidates,
  boardProfiles,
  deviceProfiles,
  productConfigurations = [],
  onRename,
  onSaveRuntimeAssignment,
  onSelectProductConfiguration,
  onCreateProductConfiguration,
  onForgetDevice,
  onOpenSetup,
  onCreateFromTemplate,
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
  executionFeedback = null,
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
  const [advancedTab, setAdvancedTab] = useState<AdvancedTab>("io");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [localSelectedButtonId, setLocalSelectedButtonId] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [productConfigurationCreating, setProductConfigurationCreating] = useState(false);
  const [productConfigurationName, setProductConfigurationName] = useState("");
  const [copyCurrentProductConfiguration, setCopyCurrentProductConfiguration] = useState(true);
  const [productConfigurationSaving, setProductConfigurationSaving] = useState(false);
  const previous = useRef<Row[]>([]);
  const candidateRetryInFlight = useRef(false);
  const pendingAssignment = useRef<RuntimeAssignment | null>(null);
  const assignmentMutationInFlight = useRef(false);
  const deviceRowElements = useRef<Record<string, HTMLButtonElement | null>>({});
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
  const visibleDevices = useMemo(
    () => [
      ...devices.filter((device) => device.connection === "online"),
      ...devices.filter((device) => device.connection === "offline"),
    ],
    [devices],
  );
  const visibleCandidates = candidates;
  const summary = deviceSummary(devices);
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
  const learningDeviceId = devices.find((device) => device.learning)?.deviceId ?? null;
  const requestedSelection: Selection | null = learningDeviceId
    ? { kind: "device", id: learningDeviceId }
    : controlledDeviceId
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
    if (learningDeviceId && (next.kind !== "device" || next.id !== learningDeviceId)) return;
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
  const handleKeypadEscape = useCallback(() => {
    const deviceId = selectedDevice?.deviceId ?? activeDeviceId;
    const target = deviceId
      ? deviceRowElements.current[deviceId]
      : rows.find((row) => row.selection.kind === "device")
        ? deviceRowElements.current[rows.find((row) => row.selection.kind === "device")!.selection.id]
        : null;
    target?.focus();
  }, [activeDeviceId, rows, selectedDevice?.deviceId]);
  useEffect(() => {
    setRenaming(false);
    setName(selectedDevice?.name ?? "");
    setLocalSelectedButtonId(null);
    setAdvancedTab("io");
    setAdvancedOpen(false);
    setProductConfigurationCreating(false);
  }, [selectedDevice?.deviceId]);
  useEffect(() => {
    if (!learningDeviceId) return;
    setAdvancedTab("io");
    setAdvancedOpen(true);
  }, [learningDeviceId]);
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
  const productEditingProfile: DeviceProfile | undefined = selectedDevice?.productDefinition && selectedDevice.productConfig
    ? {
        schema_version: 3,
        profile: selectedDevice.productDefinition.layout,
        snapshot_metadata: selectedDevice.productConfig.snapshot_metadata,
        trigger_settings: selectedDevice.productConfig.trigger_settings,
        hardware_profiles: [selectedDevice.productDefinition.hardware_profile],
        actions: selectedDevice.productConfig.actions,
      }
    : undefined;
  const editingProfile = productEditingProfile ?? deviceProfiles.find((item) => item.profile.id === (assignmentDraft.deviceProfileId || selectedDevice?.runtimeAssignment?.device_profile_id));
  const isProductDevice = Boolean(productEditingProfile);
  const compatibleProductConfigurations = useMemo(
    () => selectedDevice?.productVersionId
      ? productConfigurations.filter((configuration) =>
          configuration.product_version_id === selectedDevice.productVersionId
        )
      : [],
    [productConfigurations, selectedDevice?.productVersionId],
  );
  const buttons = useMemo(
    () => editingProfile?.profile.groups.flatMap((group) => group.buttons) ?? [],
    [editingProfile?.profile.groups],
  );
  const starterProfiles = useMemo(
    () => deviceProfiles.filter((profile) => STARTER_PROFILE_IDS.has(profile.profile.id)),
    [deviceProfiles],
  );
  const emptyLayoutTemplates = useMemo(
    () => buttons.length === 0 && selectedDevice
      ? starterProfiles
          .filter((profile) =>
            profile.profile.id !== editingProfile?.profile.id &&
            profile.hardware_profiles.some((hardware) =>
              hardware.board_profile_id === selectedDevice.boardProfileId,
            ),
          )
          .map((profile) => ({ id: profile.profile.id, name: profile.profile.name }))
      : [],
    [buttons.length, editingProfile?.profile.id, selectedDevice, starterProfiles],
  );
  const effectiveSelectedButtonId = localSelectedButtonId ?? selectedButtonId;
  const selectButton = useCallback((buttonId: string | null) => {
    setLocalSelectedButtonId(buttonId);
    onSelectedButtonChange(buttonId);
  }, [onSelectedButtonChange]);
  const selectedButton = buttons.find((button) => button.id === effectiveSelectedButtonId) ?? null;
  const selectedActions: TriggerActions = editingProfile && effectiveSelectedButtonId
    ? editingProfile.actions[effectiveSelectedButtonId] ?? {
        press: [],
        release: [],
        long_press: [],
        double_press: [],
      }
    : { press: [], release: [], long_press: [], double_press: [] };
  const sharedDeviceCount = editingProfile
    ? isProductDevice
      ? devices.filter((device) =>
          device.productConfigurationId === selectedDevice?.productConfigurationId
        ).length
      : devices.filter((device) => device.runtimeAssignment?.device_profile_id === editingProfile.profile.id).length
    : 0;
  const updateEditingProfile = useCallback(
    (next: DeviceProfile) => isProductDevice
      ? (onChangeActions ?? onChangeProfile)?.(next)
      : onChangeProfile?.(next),
    [isProductDevice, onChangeActions, onChangeProfile],
  );
  const updateActionProfile = useCallback(
    (next: DeviceProfile) => (onChangeActions ?? onChangeProfile)?.(next),
    [onChangeActions, onChangeProfile],
  );
  useEffect(() => {
    if (!buttons.some((button) => button.id === effectiveSelectedButtonId)) {
      selectButton(buttons[0]?.id ?? null);
    }
  }, [buttons, effectiveSelectedButtonId, selectButton]);
  useEffect(() => {
    if (localSelectedButtonId && localSelectedButtonId === selectedButtonId) {
      setLocalSelectedButtonId(null);
    }
  }, [localSelectedButtonId, selectedButtonId]);
  const handleWorkspaceSelection = useCallback((hardwareProfileId: string | null, deviceId: string | null) => {
    onHardwareSelectionChange?.(hardwareProfileId, deviceId);
  }, [onHardwareSelectionChange]);
  const handleBeginLearning = useCallback((hardwareProfileId: string, deviceId: string, pins: number[]) => {
    onBeginLearning?.(hardwareProfileId, deviceId, pins);
  }, [onBeginLearning]);
  const handleEndLearning = useCallback((deviceId: string) => {
    onEndLearning?.(deviceId);
  }, [onEndLearning]);
  const selectAdvancedTab = useCallback((tab: AdvancedTab) => {
    if (learningDeviceId && tab !== "io") return;
    setAdvancedOpen(true);
    setAdvancedTab(tab);
  }, [learningDeviceId]);
  const handleAdvancedTabKeyDown = useCallback((event: KeyboardEvent<HTMLButtonElement>) => {
    const currentIndex = ADVANCED_TABS.indexOf(advancedTab);
    let nextIndex = -1;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      nextIndex = (currentIndex + 1) % ADVANCED_TABS.length;
    } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      nextIndex = (currentIndex - 1 + ADVANCED_TABS.length) % ADVANCED_TABS.length;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = ADVANCED_TABS.length - 1;
    }
    if (nextIndex < 0) return;
    event.preventDefault();
    selectAdvancedTab(ADVANCED_TABS[nextIndex]);
  }, [advancedTab, selectAdvancedTab]);
  const selectedCandidateMessages = selectedCandidate
    ? candidateMessages[selectedCandidate.issue]
    : null;
  const failedButtonIds = new Set(
    executionFeedback?.status === "error" && executionFeedback.buttonId
      ? [executionFeedback.buttonId]
      : [],
  );
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
  const selectProductConfiguration = async (configurationId: string) => {
    if (!selectedDevice || !onSelectProductConfiguration) return;
    setError(null);
    try {
      await onSelectProductConfiguration(selectedDevice.deviceId, configurationId);
    } catch (reason) {
      setError({
        owner: { kind: "device", id: selectedDevice.deviceId },
        message: errorMessage(reason),
      });
    }
  };
  const createProductConfiguration = async () => {
    if (!selectedDevice || !productConfigurationName.trim() || !onCreateProductConfiguration) return;
    setProductConfigurationSaving(true);
    setError(null);
    try {
      await onCreateProductConfiguration({
        deviceId: selectedDevice.deviceId,
        name: productConfigurationName.trim(),
        copyCurrent: copyCurrentProductConfiguration,
      });
      setProductConfigurationCreating(false);
      setProductConfigurationName("");
    } catch (reason) {
      setError({
        owner: { kind: "device", id: selectedDevice.deviceId },
        message: errorMessage(reason),
      });
    } finally {
      setProductConfigurationSaving(false);
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
  const selectTitleConfiguration = (value: string) => {
    if (value === "__create__") {
      if (selectedDevice?.productVersionId) {
        setProductConfigurationName("");
        setCopyCurrentProductConfiguration(true);
        setProductConfigurationCreating(true);
      } else {
        onCreateFromTemplate?.(editingProfile?.profile.id ?? "");
      }
      return;
    }
    if (selectedDevice?.productVersionId) {
      void selectProductConfiguration(value);
    } else {
      void selectAssignment(value);
    }
  };
  return (
    <div className={`device-management${client ? " is-client" : ""}${studioMode ? " is-studio-mode" : ""}${!client && !studioMode ? " is-main-mode" : ""}${editingProfile ? " is-workspace" : ""}`}>
      <section
        className="device-list-region"
        aria-label={t(language, "devices.list")}
      >
        <header className="device-list-header">
          <div className="device-list-heading">
            <h2>{t(language, "nav.devices")}</h2>
            {!client && !studioMode && (
              <span>{t(language, "devices.deckSummary", {
                total: visibleDevices.length,
                attention: summary.attention + visibleCandidates.length,
              })}</span>
            )}
          </div>
          {!client && (
            <button
              className="primary-button"
              type="button"
              onClick={() => onOpenSetup(null)}
            >
              <Plus size={16} />
              {t(language, "setup.addKeyboard")}
            </button>
          )}
        </header>
        <div className="connected-device-list">
          <ul>
            {visibleDevices.map((device) => (
              <li key={device.deviceId}>
                <div className="device-row-wrap">
                  <button
                    ref={(element) => {
                      deviceRowElements.current[device.deviceId] = element;
                    }}
                    className={`device-row ${activeSelection?.kind === "device" && activeSelection.id === device.deviceId ? "is-selected" : ""}`}
                    type="button"
                    disabled={Boolean(learningDeviceId && learningDeviceId !== device.deviceId)}
                    aria-pressed={
                      activeSelection?.kind === "device" &&
                      activeSelection.id === device.deviceId
                    }
                    onClick={() =>
                      selectRow({ kind: "device", id: device.deviceId })
                    }
                  >
                    <strong title={device.name}>
                      {device.name}
                    </strong>
                    <span className="device-row-meta">
                      <span className="device-board-label" title={boards.get(device.boardProfileId)?.displayName ?? device.boardProfileId}>
                        {boards.get(device.boardProfileId)?.displayName ?? device.boardProfileId}
                      </span>
                      <span aria-hidden="true">·</span>
                      <span className="device-identifier" title={device.hardwareSerial}>{device.hardwareSerial}</span>
                      <span className={`device-connection-state ${device.connection === "online" ? "" : "is-offline"}`}><i aria-hidden="true" />{device.connection === "online" ? t(language, "devices.connected") : t(language, "device.offline")}</span>
                      <span aria-hidden="true">·</span>
                      <span className={`device-availability ${device.runtime === "ready" ? "is-available" : ""}`}>
                        {device.runtime === "ready" ? t(language, "devices.available") : status(device, language)}
                      </span>
                    </span>
                  </button>
                  {!client && onForgetDevice && device.connection === "offline" && (
                    <button
                      className="icon-button is-danger device-row-action"
                      type="button"
                      aria-label={`${t(language, "devices.forget")} ${device.name}`}
                      title={t(language, "devices.forget")}
                      disabled={Boolean(learningDeviceId)}
                      onClick={(event) => {
                        event.stopPropagation();
                        onForgetDevice(device.deviceId);
                      }}
                    >
                      <Trash2 size={15} aria-hidden="true" />
                    </button>
                  )}
                </div>
              </li>
            ))}
            {visibleCandidates.map((candidate) => (
                <li key={candidate.key}>
                  <button
                    className={`device-row candidate-row ${activeSelection?.kind === "candidate" && activeSelection.id === candidate.key ? "is-selected" : ""}`}
                    type="button"
                    disabled={Boolean(learningDeviceId)}
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
          <div className="panel-empty device-empty-state">
            <span>{t(language, "devices.select")}</span>
            {!client && rows.length === 0 && starterProfiles.length > 0 && onCreateFromTemplate ? (
              <section className="device-empty-templates" aria-labelledby="device-empty-templates-title">
                <h3 id="device-empty-templates-title">{t(language, "behavior.emptyTemplates")}</h3>
                <p>{t(language, "devices.emptyTemplatesHint")}</p>
                <div className="setup-template-grid">
                  {starterProfiles.map((profile) => (
                    <button
                      className="setup-template-card"
                      type="button"
                      key={profile.profile.id}
                      onClick={() => onCreateFromTemplate(profile.profile.id)}
                    >
                      <span className="setup-template-icon" aria-hidden="true"><Keyboard size={18} /></span>
                      <span className="setup-template-copy">
                        <strong>{profile.profile.name}</strong>
                        <small>{t(language, "devices.createFromTemplate")}</small>
                      </span>
                    </button>
                  ))}
                </div>
              </section>
            ) : null}
          </div>
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
              {!client && (
                <button
                  className="primary-button"
                  type="button"
                  onClick={() => onOpenSetup(candidateSetupId(selectedCandidate))}
                >
                  {t(language, "setup.continue")}
                </button>
              )}
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
              <div className="device-detail-heading-copy">
                {!client && !studioMode && <span>{t(language, "nav.layout")}</span>}
                <div className="device-detail-name-row">
                  <h2>
                    {renaming ? t(language, "devices.rename") : selectedDevice.name}
                  </h2>
                  {!client && !renaming && (
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
              </div>
              {!client && !studioMode && (
                <label className="device-configuration-selector">
                  <span>{t(language, "devices.useConfiguration")}</span>
                  <select
                    aria-label={t(language, "devices.useConfiguration")}
                    value={selectedDevice.productVersionId
                      ? selectedDevice.productConfigurationId ?? ""
                      : assignmentDraft.deviceProfileId}
                    disabled={Boolean(learningDeviceId) || assignmentSaving || productConfigurationSaving}
                    onChange={(event) => selectTitleConfiguration(event.target.value)}
                  >
                    {!selectedDevice.productVersionId && <option value="">{t(language, "model.select")}</option>}
                    {(selectedDevice.productVersionId
                      ? compatibleProductConfigurations.map((configuration) => ({ id: configuration.id, name: configuration.name }))
                      : deviceProfiles
                          .filter((profile) => compatibleHardwareProfiles(
                            profile.hardware_profiles,
                            selectedDevice.boardProfileId,
                          ).length > 0)
                          .map((profile) => ({ id: profile.profile.id, name: profile.profile.name })))
                      .map((configuration) => (
                        <option key={configuration.id} value={configuration.id}>{configuration.name}</option>
                      ))}
                    <option value="__create__">+ {t(language, "profile.create")}</option>
                  </select>
                  {sharedDeviceCount > 1 && selectedDevice.productVersionId && (
                    <small>{t(language, "devices.sharedByDevices", { count: sharedDeviceCount })}</small>
                  )}
                </label>
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
            {studioMode && (
              <div className="device-context-summary">
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
                  value={selectedDevice.productVersionId ?? assignmentLabel(selectedDevice, deviceProfiles)}
                />
              </div>
            )}
            {selectedDevice.connection === "online" &&
              selectedDevice.mode === "runtime" &&
              selectedDevice.identity === "valid" &&
              selectedDevice.assignment === "unassigned" &&
              !client && (
                <button
                  className="primary-button setup-command"
                  type="button"
                  onClick={() => onOpenSetup(selectedDevice.deviceId)}
                >
                  {t(language, "setup.continue")}
                </button>
              )}
            {editingProfile && !isProductDevice && sharedDeviceCount > 1 && (
              <div className="shared-configuration-warning" role="status">
                {t(language, "devices.sharedWarning", { name: editingProfile.profile.name, count: sharedDeviceCount })}
                      <button className="secondary-button" type="button" disabled={Boolean(learningDeviceId)} onClick={() => void onSaveSharedProfile?.(editingProfile)}>{t(language, "devices.saveShared")}</button>
                      <button className="secondary-button" type="button" disabled={Boolean(learningDeviceId)} onClick={() => void onDuplicateProfileForDevice?.({ deviceId: selectedDevice.deviceId, sourceProfile: editingProfile, name: `${editingProfile.profile.name} (${selectedDevice.name})` })}>{t(language, "devices.duplicateForDevice")}</button>
              </div>
            )}
            {editingProfile && (
              <div className="keypad-stage device-keypad-stage">
                <Keypad
                  layout={editingProfile.profile}
                  actions={editingProfile.actions}
                  selectedButtonId={effectiveSelectedButtonId}
                  pressedButtonIds={pressedButtonIds}
                  failedButtonIds={failedButtonIds}
                  failureLabel={t(language, "devices.actionFailed")}
                  actionCountLabel={(count) => t(language, "model.actionCount", { count })}
                  onSelect={selectButton}
                  onEscape={handleKeypadEscape}
                />
              </div>
            )}
            {!client && studioMode && (
              <details
                className="device-advanced-disclosure"
                open={advancedOpen}
                onToggle={(event) => {
                  if (learningDeviceId && !event.currentTarget.open) {
                    setAdvancedOpen(true);
                    return;
                  }
                  setAdvancedOpen(event.currentTarget.open);
                }}
              >
                <summary>
                  <Settings2 size={15} aria-hidden="true" />
                  <span>{t(language, "devices.advancedDisclosure")}</span>
                </summary>
                <div className="device-advanced-body">
                  {!isProductDevice && (
                    <section className="device-assignment" aria-label={t(language, "devices.assignment")}>
                      <label>
                        {t(language, "devices.useConfiguration")}
                        <select
                          aria-label={t(language, "devices.useConfiguration")}
                          value={assignmentDraft.deviceProfileId}
                          disabled={Boolean(learningDeviceId) || assignmentSaving}
                          onChange={(event) => void selectAssignment(event.target.value)}
                        >
                          <option value="">{t(language, "model.select")}</option>
                          {deviceProfiles.map((profile) => <option key={profile.profile.id} value={profile.profile.id}>{profile.profile.name}</option>)}
                        </select>
                      </label>
                      {!onChangeProfile && <label>
                        {t(language, "model.label")}
                        <select aria-label={t(language, "model.label")} value={assignmentDraft.deviceProfileId} disabled={Boolean(learningDeviceId) || assignmentSaving} onChange={(event) => void selectAssignment(event.target.value)}>
                          <option value="">{t(language, "model.select")}</option>
                          {deviceProfiles.map((profile) => <option key={profile.profile.id} value={profile.profile.id}>{profile.profile.name}</option>)}
                        </select>
                      </label>}
                      {selectedDraftProfile && compatibleHardware.length === 0 && (
                        <p className="field-error">{t(language, "devices.incompatibleProfile")}</p>
                      )}
                    </section>
                  )}
                  {editingProfile && <div className="device-advanced-toolbar">
                    {!isProductDevice && (
                      <div className="device-workspace-tabs device-advanced-tabs" role="tablist" aria-label={t(language, "devices.advancedDisclosure")}>
                        <button
                          id="device-advanced-tab-layout"
                          type="button"
                          role="tab"
                          aria-controls="device-advanced-panel-layout"
                          aria-selected={advancedTab === "layout"}
                          tabIndex={advancedTab === "layout" ? 0 : -1}
                          disabled={Boolean(learningDeviceId)}
                          onClick={() => selectAdvancedTab("layout")}
                          onKeyDown={handleAdvancedTabKeyDown}
                        >
                          {t(language, "devices.workspaceLayout")}
                        </button>
                        <button
                          id="device-advanced-tab-io"
                          type="button"
                          role="tab"
                          aria-controls="device-advanced-panel-io"
                          aria-selected={advancedTab === "io"}
                          tabIndex={advancedTab === "io" ? 0 : -1}
                          onClick={() => selectAdvancedTab("io")}
                          onKeyDown={handleAdvancedTabKeyDown}
                        >
                          {t(language, "devices.workspaceIo")}
                        </button>
                      </div>
                    )}
                    <button className="secondary-button" type="button" aria-label={t(language, "devices.configurationSettings")} disabled={Boolean(learningDeviceId)} onClick={() => setSettingsOpen(true)}>
                      {t(language, "devices.configurationSettings")}
                    </button>
                  </div>
                  }
                  {selectedDevice.connection === "offline" && <p className="form-hint">{t(language, "devices.offlineEditing")}</p>}
                  {editingProfile && !isProductDevice && advancedTab === "io" && (
                    <div id="device-advanced-panel-io" role="tabpanel" aria-labelledby="device-advanced-tab-io">
                      <h3>{t(language, "hardware.title")}</h3>
                      <HardwareMapping language={language} layout={editingProfile.profile} hardwareProfiles={editingProfile.hardware_profiles} boardProfiles={boardProfiles} devices={devices} learning={selectedDevice.learning} initialHardwareProfileId={assignmentDraft.hardwareProfileId || selectedDevice.runtimeAssignment?.hardware_profile_id} initialDeviceId={selectedDevice.deviceId} selectedButtonId={effectiveSelectedButtonId} onSelectButton={selectButton} onChange={(hardwareProfiles) => updateEditingProfile({ ...editingProfile, hardware_profiles: hardwareProfiles })} onSelectionChange={handleWorkspaceSelection} onBeginLearning={handleBeginLearning} onEndLearning={handleEndLearning} />
                    </div>
                  )}
                  {editingProfile && !isProductDevice && advancedTab === "layout" && (
                    <div id="device-advanced-panel-layout" role="tabpanel" aria-labelledby="device-advanced-tab-layout">
                      <LayoutEditor language={language} layout={editingProfile.profile} onChange={(layout) => updateEditingProfile(reconcileProfileLayout(editingProfile, layout))} />
                    </div>
                  )}
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
                        label={t(language, "devices.runtimeProfileId")}
                        value={selectedDevice.runtimeAssignment?.device_profile_id ?? "-"}
                      />
                      <TechnicalDetail
                        label={t(language, "devices.runtimeHardwareId")}
                        value={selectedDevice.runtimeAssignment?.hardware_profile_id ?? "-"}
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
                </div>
              </details>
            )}
            {selectedError && (
              <p className="field-error" role="alert">
                {selectedError}
              </p>
            )}
          </>
        )}
      </aside>
      {editingProfile && (
        <ActionEditor
          language={language}
          button={selectedButton}
          actions={selectedActions}
          canRename={!client && !isProductDevice}
          emptyTemplates={emptyLayoutTemplates}
          onUseTemplate={selectedDevice && onDuplicateProfileForDevice
            ? (profileId) => {
                const sourceProfile = deviceProfiles.find((profile) => profile.profile.id === profileId);
                if (!sourceProfile) return;
                void onDuplicateProfileForDevice({
                  deviceId: selectedDevice.deviceId,
                  sourceProfile,
                  name: `${sourceProfile.profile.name} (${selectedDevice.name})`,
                });
              }
            : undefined}
          onChange={(actions) => {
            if (!effectiveSelectedButtonId) return;
            updateActionProfile({
              ...editingProfile,
              actions: {
                ...editingProfile.actions,
                [effectiveSelectedButtonId]: actions,
              },
            });
          }}
          onRename={(buttonId, label) => {
            if (isProductDevice) return;
            updateActionProfile({
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
            });
          }}
        />
      )}
      {!client && studioMode && editingProfile && <ConfigurationSettingsDialog open={settingsOpen} language={language} profile={editingProfile} sharedDeviceCount={sharedDeviceCount} allowDuplicate={!isProductDevice} onCancel={() => setSettingsOpen(false)} onSave={(settings: TriggerSettings) => { updateEditingProfile({ ...editingProfile, trigger_settings: settings }); setSettingsOpen(false); if (!isProductDevice) void onSaveSharedProfile?.({ ...editingProfile, trigger_settings: settings }); }} onDraftChange={isProductDevice ? undefined : (settings) => updateEditingProfile({ ...editingProfile, trigger_settings: settings })} onDuplicate={async (name) => { if (selectedDevice) await onDuplicateProfileForDevice?.({ deviceId: selectedDevice.deviceId, sourceProfile: { ...editingProfile }, name }); setSettingsOpen(false); }} />}
      {productConfigurationCreating && selectedDevice && (
        <div className="modal-backdrop" role="presentation">
          <section
            className="product-configuration-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="product-configuration-create-title"
          >
            <header>
              <h2 id="product-configuration-create-title">{t(language, "productConfiguration.create")}</h2>
              <button
                className="icon-button"
                type="button"
                aria-label={t(language, "common.close")}
                title={t(language, "common.close")}
                disabled={productConfigurationSaving}
                onClick={() => setProductConfigurationCreating(false)}
              >
                <X size={16} />
              </button>
            </header>
            <form onSubmit={(event) => { event.preventDefault(); void createProductConfiguration(); }}>
              <label>
                {t(language, "productConfiguration.name")}
                <input
                  autoFocus
                  value={productConfigurationName}
                  disabled={productConfigurationSaving}
                  onChange={(event) => setProductConfigurationName(event.target.value)}
                />
              </label>
              <label className="product-configuration-copy-option">
                <input
                  type="checkbox"
                  checked={copyCurrentProductConfiguration}
                  disabled={productConfigurationSaving}
                  onChange={(event) => setCopyCurrentProductConfiguration(event.target.checked)}
                />
                <span>{t(language, "productConfiguration.copyCurrent")}</span>
              </label>
              <p>{t(language, "productConfiguration.scope")}</p>
              <footer>
                <button type="button" disabled={productConfigurationSaving} onClick={() => setProductConfigurationCreating(false)}>
                  {t(language, "common.cancel")}
                </button>
                <button className="primary-button" type="submit" disabled={productConfigurationSaving || !productConfigurationName.trim()}>
                  {t(language, productConfigurationSaving ? "save.saving" : "profile.createAction")}
                </button>
              </footer>
            </form>
          </section>
        </div>
      )}
    </div>
  );
}
