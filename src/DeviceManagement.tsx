import { Activity, Check, Keyboard, Pencil, Plus, RefreshCw, Settings2, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
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
type WorkspaceTab = "buttons" | "test" | "activity";
type AdvancedTab = "layout" | "io";
type InterfaceMode = "product" | "maker";
export type DeviceExecutionFeedback = {
  buttonId: string | null;
  status: "success" | "error";
  detail: string | null;
};
type ActionFailureInsight = {
  buttonId: string | null;
  actionKind: string | null;
  detail: string | null;
  count: number;
  timestampMs: number;
};

const WORKSPACE_TABS: readonly WorkspaceTab[] = ["buttons", "test", "activity"];
const ADVANCED_TABS: readonly AdvancedTab[] = ["layout", "io"];
const ACTION_KIND_MESSAGES: Record<string, MessageKey> = {
  paste: "behavior.summary.paste",
  hotkey: "behavior.summary.hotkey",
  delay: "behavior.summary.delay",
  media: "behavior.summary.media",
  open: "behavior.summary.open",
};
const ACTION_FAILURE_CODES = new Set([
  "action_step_failed",
  "action_timeout",
  "action_ack_timeout",
  "action_cancelled",
]);
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

function formatSnapshotDate(language: Language, timestamp: number | undefined) {
  if (!timestamp) return null;
  try {
    return new Intl.DateTimeFormat(language, {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(timestamp));
  } catch {
    return null;
  }
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
  onCopyProductConfig?(sourceDeviceId: string, targetDeviceId: string): Promise<void>;
  onForgetDevice?(deviceId: string): void;
  onMetricsChange(deviceId: string | null): void;
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
  language,
  devices,
  candidates,
  boardProfiles,
  deviceProfiles,
  metrics,
  onRename,
  onSaveRuntimeAssignment,
  onCopyProductConfig,
  onForgetDevice,
  onMetricsChange,
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
  const [workspaceTab, setWorkspaceTab] = useState<WorkspaceTab>("buttons");
  const [advancedTab, setAdvancedTab] = useState<AdvancedTab>("io");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [interfaceMode, setInterfaceMode] = useState<InterfaceMode>("product");
  const [localSelectedButtonId, setLocalSelectedButtonId] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [copySourceDeviceId, setCopySourceDeviceId] = useState("");
  const [copyingProductConfig, setCopyingProductConfig] = useState(false);
  const [pendingTestDeviceId, setPendingTestDeviceId] = useState<string | null>(null);
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
  const visibleDevices = devices;
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
  const selectedMetrics =
    selectedDevice && metrics?.deviceId === selectedDevice.deviceId
      ? metrics.snapshot
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
    onMetricsChange(selectedDevice?.deviceId ?? null);
  }, [onMetricsChange, selectedDevice?.deviceId]);
  useEffect(() => {
    setRenaming(false);
    setName(selectedDevice?.name ?? "");
    setLocalSelectedButtonId(null);
    setWorkspaceTab("buttons");
    setAdvancedTab("io");
    setAdvancedOpen(false);
  }, [selectedDevice?.deviceId]);
  useEffect(() => {
    if (!learningDeviceId) return;
    setInterfaceMode("maker");
    setAdvancedTab("io");
    setAdvancedOpen(true);
    setWorkspaceTab("buttons");
  }, [learningDeviceId]);
  useEffect(() => {
    if (!pendingTestDeviceId) return;
    const pendingDevice = devices.find(({ deviceId }) => deviceId === pendingTestDeviceId);
    if (pendingDevice?.learning) return;
    setPendingTestDeviceId(null);
    setAdvancedOpen(false);
    setWorkspaceTab("test");
  }, [devices, pendingTestDeviceId]);
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
  const productCopySources = useMemo(
    () => selectedDevice?.productVersionId
      ? devices.filter((device) =>
          device.deviceId !== selectedDevice.deviceId &&
          device.productVersionId === selectedDevice.productVersionId &&
          device.productConfig,
        )
      : [],
    [devices, selectedDevice?.deviceId, selectedDevice?.productVersionId],
  );
  useEffect(() => {
    setCopySourceDeviceId((current) =>
      productCopySources.some((device) => device.deviceId === current)
        ? current
        : (productCopySources[0]?.deviceId ?? ""),
    );
  }, [productCopySources]);
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
    ? devices.filter((device) => device.runtimeAssignment?.device_profile_id === editingProfile.profile.id).length
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
  const handleFinishLearning = useCallback((deviceId: string) => {
    setPendingTestDeviceId(deviceId);
  }, []);
  const selectWorkspaceTab = useCallback((tab: WorkspaceTab) => {
    if (learningDeviceId && tab === "test") return;
    setWorkspaceTab(tab);
  }, [learningDeviceId]);
  const handleWorkspaceTabKeyDown = useCallback((event: KeyboardEvent<HTMLButtonElement>) => {
    const currentIndex = WORKSPACE_TABS.indexOf(workspaceTab);
    let nextIndex = -1;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      nextIndex = (currentIndex + 1) % WORKSPACE_TABS.length;
    } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      nextIndex = (currentIndex - 1 + WORKSPACE_TABS.length) % WORKSPACE_TABS.length;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = WORKSPACE_TABS.length - 1;
    }
    if (nextIndex < 0) return;
    event.preventDefault();
    selectWorkspaceTab(WORKSPACE_TABS[nextIndex]);
  }, [selectWorkspaceTab, workspaceTab]);
  const selectAdvancedTab = useCallback((tab: AdvancedTab) => {
    if (learningDeviceId && tab !== "io") return;
    setInterfaceMode("maker");
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
  const pressedButtonLabels = buttons
    .filter((button) => pressedButtonIds.has(button.id))
    .map((button) => button.label);
  const selectedExecutionFeedback = executionFeedback && (
    !executionFeedback.buttonId || executionFeedback.buttonId === effectiveSelectedButtonId
  )
    ? executionFeedback
    : null;
  const failedButtonIds = new Set(
    executionFeedback?.status === "error" && executionFeedback.buttonId
      ? [executionFeedback.buttonId]
      : [],
  );
  const usageByButton = useMemo(() => {
    const usage = new Map<string, number>();
    if (selectedMetrics?.heatmap.length) {
      for (const entry of selectedMetrics.heatmap) {
        usage.set(entry.buttonId, (usage.get(entry.buttonId) ?? 0) + entry.presses);
      }
    } else {
      for (const log of selectedMetrics?.logs ?? []) {
        if (log.kind !== "button" || !log.buttonId) continue;
        usage.set(log.buttonId, (usage.get(log.buttonId) ?? 0) + 1);
      }
    }
    return usage;
  }, [selectedMetrics]);
  const popularButtons = useMemo(
    () => [...usageByButton.entries()]
      .sort(([leftId, leftCount], [rightId, rightCount]) =>
        rightCount - leftCount || leftId.localeCompare(rightId),
      )
      .slice(0, 5),
    [usageByButton],
  );
  const unusedButtons = useMemo(
    () => buttons.filter((button) => !usageByButton.has(button.id)),
    [buttons, usageByButton],
  );
  const activityDays = useMemo(() => {
    const byDay = new Map<string, { total: number; buttons: Map<string, number> }>();
    for (const entry of selectedMetrics?.heatmap ?? []) {
      const day = byDay.get(entry.day) ?? { total: 0, buttons: new Map<string, number>() };
      day.total += entry.presses;
      day.buttons.set(entry.buttonId, (day.buttons.get(entry.buttonId) ?? 0) + entry.presses);
      byDay.set(entry.day, day);
    }
    return [...byDay.entries()].sort(([left], [right]) => left.localeCompare(right));
  }, [selectedMetrics]);
  const maxActivityDay = Math.max(1, ...activityDays.map(([, day]) => day.total));
  const latestActionFailure = useMemo(() => {
    const runtimeError = selectedDevice?.latestError;
    const runtimeFailure = runtimeError && ACTION_FAILURE_CODES.has(runtimeError.code)
      ? runtimeError
      : null;
    const feedbackFailure = executionFeedback?.status === "error"
      ? executionFeedback
      : null;
    if (!runtimeFailure && !feedbackFailure) return null;
    const buttonId = runtimeFailure?.params.button ?? feedbackFailure?.buttonId ?? null;
    const step = runtimeFailure?.params.step ? Number(runtimeFailure.params.step) : NaN;
    const action = buttonId && Number.isInteger(step) && step > 0
      ? editingProfile?.actions[buttonId]?.press[step - 1]
      : undefined;
    return {
      buttonId,
      detail: runtimeFailure?.detail ?? feedbackFailure?.detail ?? null,
      actionKind: runtimeFailure?.params.actionKind ?? action?.type ?? null,
    };
  }, [editingProfile, executionFeedback, selectedDevice?.latestError]);
  const persistedActionFailures = useMemo<ActionFailureInsight[]>(() => {
    const grouped = new Map<string, ActionFailureInsight>();
    for (const log of selectedMetrics?.logs ?? []) {
      if (log.kind !== "action_failed") continue;
      const buttonId = log.buttonId ?? null;
      const actionKind = log.actionKind ?? null;
      if (!buttonId && !actionKind) continue;
      const key = `${buttonId ?? ""}\u0000${actionKind ?? ""}`;
      const current = grouped.get(key);
      if (!current) {
        grouped.set(key, {
          buttonId,
          actionKind,
          detail: log.detail ?? null,
          count: 1,
          timestampMs: log.timestampMs,
        });
        continue;
      }
      current.count += 1;
      if (log.timestampMs >= current.timestampMs) {
        current.detail = log.detail ?? null;
        current.timestampMs = log.timestampMs;
      }
    }
    return [...grouped.values()]
      .sort((left, right) => right.count - left.count || right.timestampMs - left.timestampMs)
      .slice(0, 5);
  }, [selectedMetrics]);
  const actionFailureInsights = useMemo<ActionFailureInsight[]>(() => {
    if (!latestActionFailure) return persistedActionFailures;
    const key = `${latestActionFailure.buttonId ?? ""}\u0000${latestActionFailure.actionKind ?? ""}`;
    const alreadyPersisted = persistedActionFailures.some((failure) =>
      `${failure.buttonId ?? ""}\u0000${failure.actionKind ?? ""}` === key,
    );
    if (alreadyPersisted) return persistedActionFailures;
    return [
      {
        ...latestActionFailure,
        count: 1,
        timestampMs: Date.now(),
      },
      ...persistedActionFailures,
    ].slice(0, 5);
  }, [latestActionFailure, persistedActionFailures]);
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
  const copyProductConfig = async () => {
    if (!selectedDevice || !copySourceDeviceId || !onCopyProductConfig) return;
    setCopyingProductConfig(true);
    setError(null);
    try {
      await onCopyProductConfig(copySourceDeviceId, selectedDevice.deviceId);
    } catch (reason) {
      setError({
        owner: { kind: "device", id: selectedDevice.deviceId },
        message: errorMessage(reason),
      });
    } finally {
      setCopyingProductConfig(false);
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
    <div className={`device-management${client ? " is-client" : ""}${editingProfile && (client || workspaceTab === "buttons") ? " is-workspace" : ""}`}>
      <section
        className="device-list-region"
        aria-label={t(language, "devices.list")}
      >
        <header className="device-list-header">
          <h2>{t(language, "nav.devices")}</h2>
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
              {!client && !renaming && (
                <div
                  className="device-mode-switch"
                  role="group"
                  aria-label={t(language, "devices.interfaceMode")}
                >
                  <button
                    type="button"
                    aria-pressed={interfaceMode === "product"}
                    disabled={Boolean(learningDeviceId)}
                    onClick={() => {
                      setInterfaceMode("product");
                      setAdvancedOpen(false);
                    }}
                  >
                    {t(language, "devices.productMode")}
                  </button>
                  <button
                    type="button"
                    aria-pressed={interfaceMode === "maker"}
                    onClick={() => setInterfaceMode("maker")}
                  >
                    {t(language, "devices.makerMode")}
                  </button>
                </div>
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
            {editingProfile && !client && (
              <div className="device-workspace-toolbar is-primary">
                <div className="device-workspace-tabs" role="tablist" aria-label={t(language, "devices.workspaceNavigation")}>
                  <button
                    id="device-workspace-tab-buttons"
                    type="button"
                    role="tab"
                    aria-controls="device-workspace-panel-buttons"
                    aria-selected={workspaceTab === "buttons"}
                    tabIndex={workspaceTab === "buttons" ? 0 : -1}
                    onClick={() => selectWorkspaceTab("buttons")}
                    onKeyDown={handleWorkspaceTabKeyDown}
                  >
                    {t(language, "devices.workspaceButtons")}
                  </button>
                  <button
                    id="device-workspace-tab-test"
                    type="button"
                    role="tab"
                    aria-controls="device-workspace-panel-test"
                    aria-selected={workspaceTab === "test"}
                    tabIndex={workspaceTab === "test" ? 0 : -1}
                    disabled={Boolean(learningDeviceId)}
                    onClick={() => selectWorkspaceTab("test")}
                    onKeyDown={handleWorkspaceTabKeyDown}
                  >
                    {t(language, "devices.workspaceTest")}
                  </button>
                  <button
                    id="device-workspace-tab-activity"
                    type="button"
                    role="tab"
                    aria-controls="device-workspace-panel-activity"
                    aria-selected={workspaceTab === "activity"}
                    tabIndex={workspaceTab === "activity" ? 0 : -1}
                    onClick={() => selectWorkspaceTab("activity")}
                    onKeyDown={handleWorkspaceTabKeyDown}
                  >
                    <Activity size={14} aria-hidden="true" />
                    {t(language, "devices.workspaceActivity")}
                  </button>
                </div>
              </div>
            )}
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
            {editingProfile && (client || workspaceTab === "buttons") && (
              <div id="device-workspace-panel-buttons" className="keypad-stage device-keypad-stage" role="tabpanel" aria-labelledby="device-workspace-tab-buttons">
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
            {!client && editingProfile && workspaceTab === "test" && (
              <div id="device-workspace-panel-test" className="device-test-panel" role="tabpanel" aria-labelledby="device-workspace-tab-test">
                <div className="device-test-heading">
                  <div>
                    <h3>{t(language, "devices.testTitle")}</h3>
                    <p>{t(language, "devices.testHint")}</p>
                  </div>
                  <div className="device-test-actions">
                    <button className="secondary-button" type="button" onClick={() => selectAdvancedTab("io")}>
                      <RefreshCw size={14} aria-hidden="true" />
                      {t(language, "devices.testRelearn")}
                    </button>
                    <span className="device-test-live"><i aria-hidden="true" />{t(language, "devices.testLive")}</span>
                  </div>
                </div>
                <div
                  className={`device-test-status${selectedExecutionFeedback?.status === "error" ? " is-error" : ""}`}
                  role={selectedExecutionFeedback?.status === "error" ? "alert" : "status"}
                  aria-live="polite"
                >
                  {pressedButtonLabels.length > 0
                    ? t(language, "devices.testPressed", { buttons: pressedButtonLabels.join(", ") })
                    : selectedExecutionFeedback?.status === "success"
                      ? t(language, "devices.actionSent")
                      : selectedExecutionFeedback?.status === "error"
                        ? `${t(language, "devices.actionFailed")}${selectedExecutionFeedback.detail ? `: ${selectedExecutionFeedback.detail}` : ""}`
                        : t(language, "devices.testWaiting")}
                </div>
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
              </div>
            )}
            {!client && (!editingProfile || workspaceTab === "activity") && (
              <div
                id={editingProfile ? "device-workspace-panel-activity" : undefined}
                className="device-activity-panel"
                role={editingProfile ? "tabpanel" : undefined}
                aria-labelledby={editingProfile ? "device-workspace-tab-activity" : undefined}
              >
                <div className="device-activity-heading">
                  <div>
                    <h3>{t(language, "devices.activity")}</h3>
                    <p>{t(language, "devices.activityHint")}</p>
                  </div>
                </div>
                {selectedMetrics ? (
                  <>
                    <section className="device-metrics" aria-label={t(language, "devices.metricsSummary")}>
                      <div><span>{t(language, "home.todayPresses")}</span><strong>{selectedMetrics.todayPresses}</strong></div>
                      <div><span>{t(language, "home.totalPresses")}</span><strong>{selectedMetrics.totalPresses}</strong></div>
                      <div><span>{t(language, "home.activeButtons")}</span><strong>{selectedMetrics.activeButtonCount}</strong></div>
                      <div><span>{t(language, "home.topButton")}</span><strong>{buttons.find((button) => button.id === selectedMetrics.topButton?.buttonId)?.label ?? selectedMetrics.topButton?.buttonId ?? "-"}</strong></div>
                    </section>
                    <section className="device-activity-insight" aria-label={t(language, "devices.activityHeatmap")}>
                      <h4>{t(language, "devices.activityHeatmap")}</h4>
                      {activityDays.length > 0 ? (
                        <div className="heatmap" aria-label={t(language, "devices.activityHeatmap")}>
                          {activityDays.map(([day, summary]) => {
                            const level = Math.min(4, Math.max(1, Math.ceil((summary.total / maxActivityDay) * 4)));
                            const labels = [...summary.buttons.entries()]
                              .sort(([, left], [, right]) => right - left)
                              .slice(0, 3)
                              .map(([buttonId]) => buttons.find((button) => button.id === buttonId)?.label ?? buttonId)
                              .join("、");
                            return (
                              <div className={`heat-cell heat-${level}`} key={day}>
                                <span>{day}</span>
                                <strong>{summary.total}</strong>
                                <small>{labels}</small>
                              </div>
                            );
                          })}
                        </div>
                      ) : <p className="panel-empty">{t(language, "activity.empty")}</p>}
                    </section>
                    <section className="device-activity-insight" aria-label={t(language, "devices.popularButtons")}>
                      <h4>{t(language, "devices.popularButtons")}</h4>
                      {popularButtons.length > 0 ? (
                        <ul>
                          {popularButtons.map(([buttonId, count]) => (
                            <li key={buttonId}>
                              <button className="secondary-button" type="button" onClick={() => {
                                selectButton(buttonId);
                                setWorkspaceTab("buttons");
                              }}>
                                {buttons.find((button) => button.id === buttonId)?.label ?? buttonId}
                              </button>
                              <span>{count}</span>
                            </li>
                          ))}
                        </ul>
                      ) : <p className="panel-empty">{t(language, "activity.empty")}</p>}
                    </section>
                    {editingProfile && (
                      <section className="device-activity-insight" aria-label={t(language, "devices.unusedButtons")}>
                        <h4>{t(language, "devices.unusedButtons")}</h4>
                        {unusedButtons.length > 0 ? (
                          <ul>
                            {unusedButtons.map((button) => (
                              <li key={button.id}>
                                <button className="secondary-button" type="button" onClick={() => {
                                  selectButton(button.id);
                                  setWorkspaceTab("buttons");
                                }}>
                                  {button.label}
                                </button>
                                <span>{button.id}</span>
                              </li>
                            ))}
                          </ul>
                        ) : <p className="panel-empty">{t(language, "devices.noUnusedButtons")}</p>}
                      </section>
                    )}
                    {actionFailureInsights.length > 0 && (
                      <section className="device-activity-insight" aria-label={t(language, "devices.failedActions")} role="alert">
                        <h4>{t(language, "devices.failedActions")}</h4>
                        <ul>
                          {actionFailureInsights.map((failure) => {
                            const button = buttons.find((candidate) => candidate.id === failure.buttonId);
                            return (
                              <li key={`${failure.buttonId ?? "unknown"}:${failure.actionKind ?? "unknown"}`}>
                                {button ? (
                                  <button className="secondary-button" type="button" onClick={() => {
                                    selectButton(button.id);
                                    setWorkspaceTab("buttons");
                                  }}>
                                    <span className="sr-only">{t(language, "devices.editButton")}: </span>
                                    {button.label}
                                  </button>
                                ) : <span>{failure.buttonId ?? "-"}</span>}
                                <span>
                                  {" · "}
                                  {failure.actionKind && ACTION_KIND_MESSAGES[failure.actionKind]
                                    ? t(language, ACTION_KIND_MESSAGES[failure.actionKind])
                                    : t(language, "devices.actionKindUnknown")}
                                  {failure.count > 1 && ` · ${t(language, "devices.failureCount", { count: failure.count })}`}
                                  {failure.detail ? `: ${failure.detail}` : ""}
                                </span>
                              </li>
                            );
                          })}
                        </ul>
                      </section>
                    )}
                    <div className="device-activity-wrap">
                      <table className="device-activity" aria-label={t(language, "devices.activity")}>
                        <tbody>
                          {selectedMetrics.logs.map((log) => (
                            <tr key={`${log.timestampMs}:${log.deviceId}:${log.deviceProfileId}:${log.hardwareProfileId}:${log.message}`}>
                              <td><time>{new Date(log.timestampMs).toLocaleTimeString()}</time></td>
                              <td>{log.deviceName}</td>
                              <td>{log.deviceProfileId}</td>
                              <td>{log.hardwareProfileId}</td>
                              <td>{log.actionKind ? `${log.message}${log.detail ? `: ${log.detail}` : ""}` : log.message}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </>
                ) : <p className="panel-empty">{t(language, "activity.empty")}</p>}
              </div>
            )}
            {!client && interfaceMode === "maker" && (
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
                  {editingProfile && isProductDevice && (
                    <section className="device-assignment" aria-label={t(language, "devices.copyProductConfig")}>
                      {editingProfile.snapshot_metadata?.created_at && (
                        <p>
                          {t(language, "data.createdAt", {
                            time: formatSnapshotDate(language, editingProfile.snapshot_metadata.created_at) ?? "",
                          })}
                        </p>
                      )}
                      {editingProfile.snapshot_metadata?.source_device_name && (
                        <p>
                          {t(language, "data.sourceDevice", {
                            name: editingProfile.snapshot_metadata.source_device_name,
                          })}
                        </p>
                      )}
                      <label>
                        {t(language, "devices.copyProductConfig")}
                        <select
                          aria-label={t(language, "devices.copySource")}
                          value={copySourceDeviceId}
                          disabled={Boolean(learningDeviceId) || copyingProductConfig || productCopySources.length === 0}
                          onChange={(event) => setCopySourceDeviceId(event.target.value)}
                        >
                          {productCopySources.length === 0 ? (
                            <option value="">{t(language, "devices.copySourceEmpty")}</option>
                          ) : productCopySources.map((device) => (
                            <option key={device.deviceId} value={device.deviceId}>{device.name}</option>
                          ))}
                        </select>
                      </label>
                      <button
                        className="secondary-button"
                        type="button"
                        disabled={Boolean(learningDeviceId) || !copySourceDeviceId || copyingProductConfig || !onCopyProductConfig}
                        onClick={() => void copyProductConfig()}
                      >
                        {t(language, copyingProductConfig ? "devices.copyingProductConfig" : "devices.copyProductConfigAction")}
                      </button>
                    </section>
                  )}
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
                      <HardwareMapping language={language} layout={editingProfile.profile} hardwareProfiles={editingProfile.hardware_profiles} boardProfiles={boardProfiles} devices={devices} learning={selectedDevice.learning} initialHardwareProfileId={assignmentDraft.hardwareProfileId || selectedDevice.runtimeAssignment?.hardware_profile_id} initialDeviceId={selectedDevice.deviceId} selectedButtonId={effectiveSelectedButtonId} onSelectButton={selectButton} onChange={(hardwareProfiles) => updateEditingProfile({ ...editingProfile, hardware_profiles: hardwareProfiles })} onSelectionChange={handleWorkspaceSelection} onBeginLearning={handleBeginLearning} onEndLearning={handleEndLearning} onFinishLearning={handleFinishLearning} />
                    </div>
                  )}
                  {editingProfile && !isProductDevice && advancedTab === "layout" && (
                    <div id="device-advanced-panel-layout" role="tabpanel" aria-labelledby="device-advanced-tab-layout">
                      <LayoutEditor language={language} layout={editingProfile.profile} onChange={(layout) => updateEditingProfile({ ...editingProfile, profile: layout })} />
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
      {editingProfile && (client || workspaceTab === "buttons") && (
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
      {!client && editingProfile && <ConfigurationSettingsDialog open={settingsOpen} language={language} profile={editingProfile} sharedDeviceCount={sharedDeviceCount} allowDuplicate={!isProductDevice} onCancel={() => setSettingsOpen(false)} onSave={(settings: TriggerSettings) => { updateEditingProfile({ ...editingProfile, trigger_settings: settings }); setSettingsOpen(false); if (!isProductDevice) void onSaveSharedProfile?.({ ...editingProfile, trigger_settings: settings }); }} onDraftChange={isProductDevice ? undefined : (settings) => updateEditingProfile({ ...editingProfile, trigger_settings: settings })} onDuplicate={async (name) => { if (selectedDevice) await onDuplicateProfileForDevice?.({ deviceId: selectedDevice.deviceId, sourceProfile: { ...editingProfile }, name }); setSettingsOpen(false); }} />}
    </div>
  );
}
