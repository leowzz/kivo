import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save as saveFile } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ArchiveRestore,
  DatabaseBackup,
  FileInput,
  Keyboard,
  Plus,
  Settings2,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { RedoCircle } from "reicon-react/icons/RedoCircle";
import { UndoCircle } from "reicon-react/icons/UndoCircle";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import brandIcon from "../src-tauri/icons/128x128.png";
import { ConfirmDialog } from "./ConfirmDialog";
import { CreateDeviceProfileForm } from "./CreateDeviceProfileForm";
import {
  DeviceManagement,
  type DeviceExecutionFeedback,
} from "./DeviceManagement";
import {
  DeviceSetupWizard,
  type DeviceSetupVerification,
} from "./DeviceSetupWizard";
import { hardwareProfilesAreValid } from "./HardwareMapping";
import { deviceSummary } from "./deviceStatus";
import { reconcileSetupSession, setupPresence } from "./deviceSetupSession";
import { t } from "./i18n";
import {
  projectImportedProfiles,
  summarizeProfiles,
  type ProfileContentSummary,
} from "./profileSummary";
import { DEFAULT_DOUBLE_PRESS_MS, DEFAULT_LONG_PRESS_MS } from "./types";
import type {
  AppSnapshot,
  BackupPreview,
  ButtonAction,
  CreateDeviceProfileRequest,
  DeviceProfile,
  HardwareProfile,
  ImportPreview,
  InputSource,
  Language,
  LearningTarget,
  PhysicalInput,
  ProductConfigurationProfile,
  RuntimeAssignment,
  RuntimeEvent,
  StartupFailure,
  TriggerActions,
  UsageView,
} from "./types";
import { SerializedSaveQueue, useAutosave } from "./useAutosave";
import { UsageSettingsPanel, type UsageSettingsPatch } from "./UsageSettingsPanel";
import {
  useProductConfigHistory,
  useProfileHistory,
  type ProductConfigHistoryEntry,
} from "./useProfileHistory";

type View = "devices" | "data";
type Confirmation =
  | { kind: "import"; path: string; preview: ImportPreview }
  | { kind: "restore"; path: string; preview: BackupPreview }
  | { kind: "delete"; profile: DeviceProfile }
  | { kind: "forget"; device: AppSnapshot["devices"][number] };

type RegistryState = Pick<
  AppSnapshot,
  "deviceProfiles" | "productConfigurations" | "editorProfile" | "boardProfiles" | "devices" | "candidates"
>;
type PressedOwner = {
  deviceProfileId: string;
  hardwareProfileId: string;
  buttonIds: Set<string>;
};
type RuntimeFeedbackIdentity = Omit<PressedOwner, "buttonIds">;
type RuntimeFeedbackTarget = RuntimeFeedbackIdentity & {
  profile: DeviceProfile;
};
type HardwareEditorTarget = {
  deviceProfileId: string;
  hardwareProfileId: string;
  deviceId: string | null;
};
type ProfileAutosaveTarget = {
  profiles: DeviceProfile[];
};
type PersistedProfileSave = {
  serialized: string;
  snapshot: AppSnapshot;
};
type HistoryTarget =
  | { kind: "product"; deviceId: string; configurationId: string }
  | { kind: "profile"; profileId: string }
  | null;

const PREVIEW_MODE = import.meta.env.DEV && new URLSearchParams(window.location.search).has("preview");
const REGISTRY_REFRESH_MS = 1_500;

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error && "code" in error) return String(error.code);
  return String(error);
}

function defaultBackupFilename(now = new Date()) {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `kivo-backup-${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}.yaml`;
}

function productConfigHistoryEntries(
  configurations: readonly ProductConfigurationProfile[],
): ProductConfigHistoryEntry[] {
  return configurations.map((config) => ({
    configurationId: config.id,
    config,
  }));
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

function isValidDraft(
  profile: DeviceProfile | undefined,
  boardProfiles: AppSnapshot["boardProfiles"],
) {
  return Boolean(profile &&
    Number.isInteger(profile.trigger_settings.long_press_ms) &&
    profile.trigger_settings.long_press_ms >= 100 &&
    profile.trigger_settings.long_press_ms <= 5_000 &&
    Number.isInteger(profile.trigger_settings.double_press_ms) &&
    profile.trigger_settings.double_press_ms >= 100 &&
    profile.trigger_settings.double_press_ms <= 1_000 &&
    Object.values(profile.actions).every((groups) =>
    [groups.press, groups.release, groups.long_press, groups.double_press].every((actions) => actions.every((action: ButtonAction) => {
      switch (action.type) {
        case "paste": return action.text.length > 0;
        case "hotkey": return action.keys.length > 0;
        case "delay": return Number.isInteger(action.duration_ms) && action.duration_ms >= 1 && action.duration_ms <= 60_000;
        case "media": return action.command.length > 0;
        case "open": return action.target.trim().length > 0 && action.target.length <= 2_048 && !action.target.includes("\0");
      }
    }))) && hardwareProfilesAreValid(profile.hardware_profiles, boardProfiles, profile.profile));
}

function emptyTriggerActions(): TriggerActions {
  return { press: [], release: [], long_press: [], double_press: [] };
}

function normalizeDeviceProfile(profile: DeviceProfile): DeviceProfile {
  const actions = Object.fromEntries(
    Object.entries(profile.actions ?? {}).map(([buttonId, groups]) => [
      buttonId,
      {
        ...emptyTriggerActions(),
        ...(Array.isArray(groups) ? { press: groups } : groups ?? {}),
      },
    ]),
  );
  return {
    ...profile,
    trigger_settings: {
      long_press_ms: profile.trigger_settings?.long_press_ms ?? DEFAULT_LONG_PRESS_MS,
      double_press_ms: profile.trigger_settings?.double_press_ms ?? DEFAULT_DOUBLE_PRESS_MS,
    },
    actions,
  };
}

function resolveButton(hardware: HardwareProfile | undefined, input: PhysicalInput) {
  if (!hardware) return null;
  let runtimeSource = 0;
  for (const source of hardware.inputs) {
    if (source.type === "direct") {
      if (Object.keys(source.keys).length === 0) continue;
      if (input.type === "direct") {
        const match = Object.entries(source.keys).find(([, gpio]) => gpio === input.gpio);
        if (match) return match[0];
      }
      runtimeSource += 1;
    } else if (source.type === "contact_matrix") {
      if (source.keys && Object.keys(source.keys).length === 0) continue;
      if (input.type === "contact" && input.source === runtimeSource) {
        const pair = [Math.min(input.pin_a, input.pin_b), Math.max(input.pin_a, input.pin_b)];
        const match = Object.entries(source.keys).find(([, pins]) => pins[0] === pair[0] && pins[1] === pair[1]);
        if (match) return match[0];
      }
      runtimeSource += 1;
    } else {
      runtimeSource += 1;
    }
  }
  return null;
}

function learnInput(profile: DeviceProfile, hardwareProfileId: string, buttonId: string, input: PhysicalInput): DeviceProfile {
  const hardware = profile.hardware_profiles.find((item) => item.id === hardwareProfileId);
  if (!hardware) return profile;
  const inputs = hardware.inputs.map((source): InputSource => {
    if (source.type === "feature_switch") return source;
    return {
      ...source,
      keys: Object.fromEntries(Object.entries(source.keys).filter(([id]) => id !== buttonId)),
    };
  });

  if (input.type === "direct") {
    let index = inputs.findIndex((source) => source.type === "direct");
    if (index < 0) {
      inputs.push({ type: "direct", id: "direct", keys: {} });
      index = inputs.length - 1;
    }
    const source = inputs[index];
    if (source.type === "direct") {
      source.keys = Object.fromEntries(Object.entries(source.keys).filter(([, gpio]) => gpio !== input.gpio));
      source.keys[buttonId] = input.gpio;
    }
  } else {
    let index = inputs.findIndex((source) => source.type === "contact_matrix");
    if (index < 0) {
      inputs.push({ type: "contact_matrix", id: "matrix", pins: [], keys: {} });
      index = inputs.length - 1;
    }
    const source = inputs[index];
    if (source.type === "contact_matrix") {
      const pair: [number, number] = [Math.min(input.pin_a, input.pin_b), Math.max(input.pin_a, input.pin_b)];
      source.pins = [...new Set([...source.pins, ...pair])].sort((left, right) => left - right);
      source.keys = Object.fromEntries(Object.entries(source.keys).filter(([, pins]) =>
        pins[0] !== pair[0] || pins[1] !== pair[1]
      ));
      source.keys[buttonId] = pair;
    }
  }

  return {
    ...profile,
    hardware_profiles: profile.hardware_profiles.map((item) =>
      item.id === hardwareProfileId ? { ...item, inputs } : item
    ),
  };
}

function pressedButtons(owners: Map<string, PressedOwner>) {
  return new Set([...owners.values()].flatMap((owner) => [...owner.buttonIds]));
}

function productRuntimeFeedbackTarget(
  device: AppSnapshot["devices"][number],
): RuntimeFeedbackTarget | null {
  const definition = device.productDefinition;
  const config = device.productConfig;
  const productVersionId = device.productVersionId;
  if (!definition || !config || !productVersionId ||
    definition.product.product_version_id !== productVersionId ||
    config.product_version_id !== productVersionId) {
    return null;
  }
  return {
    deviceProfileId: productVersionId,
    hardwareProfileId: definition.hardware_profile.id,
    profile: {
      schema_version: 3,
      profile: definition.layout,
      snapshot_metadata: config.snapshot_metadata,
      trigger_settings: config.trigger_settings,
      hardware_profiles: [definition.hardware_profile],
      actions: config.actions,
    },
  };
}

function runtimeFeedbackIdentity(
  device: AppSnapshot["devices"][number],
): RuntimeFeedbackIdentity | null {
  const productTarget = productRuntimeFeedbackTarget(device);
  if (productTarget) {
    return {
      deviceProfileId: productTarget.deviceProfileId,
      hardwareProfileId: productTarget.hardwareProfileId,
    };
  }
  if (device.productVersionId || !device.runtimeAssignment) return null;
  return {
    deviceProfileId: device.runtimeAssignment.device_profile_id,
    hardwareProfileId: device.runtimeAssignment.hardware_profile_id,
  };
}

function learningTargetsMatch(left: LearningTarget, right: LearningTarget) {
  return left.deviceId === right.deviceId &&
    left.deviceProfileId === right.deviceProfileId &&
    left.hardwareProfileId === right.hardwareProfileId &&
    left.editingRevision === right.editingRevision &&
    left.firmwareRevision === right.firmwareRevision &&
    left.pins.length === right.pins.length &&
    left.pins.every((pin, index) => pin === right.pins[index]);
}

export default function App({
  embedded = false,
  client = false,
}: {
  embedded?: boolean;
  client?: boolean;
}) {
  const queue = useRef(new SerializedSaveQueue()).current;
  const [registry, setRegistry] = useState<RegistryState>({
    deviceProfiles: [],
    productConfigurations: [],
    editorProfile: null,
    boardProfiles: [],
    devices: [],
    candidates: [],
  });
  const [savedDeviceProfiles, setSavedDeviceProfiles] = useState<Record<string, string>>({});
  const [language, setLanguage] = useState<Language>("zh-CN");
  const [view, setView] = useState<View>("devices");
  const [homeMetrics, setHomeMetrics] = useState<AppSnapshot["homeMetrics"]>(null);
  const [usage, setUsage] = useState<UsageView | null>(null);
  const [selectedButtonId, setSelectedButtonId] = useState<string | null>(null);
  const [selectedManagedDeviceId, setSelectedManagedDeviceId] = useState<string | null>(null);
  const [hardwareEditorTarget, setHardwareEditorTarget] = useState<HardwareEditorTarget | null>(null);
  const [capturedDraftProfileIds, setCapturedDraftProfileIds] = useState<Set<string>>(() => new Set());
  const [pendingSharedDraftProfileIds, setPendingSharedDraftProfileIds] = useState<Set<string>>(() => new Set());
  const [tentativeLearningCounts, setTentativeLearningCounts] = useState<Map<string, number>>(() => new Map());
  const [pressedButtonIds, setPressedButtonIds] = useState<Set<string>>(() => new Set());
  const [executionFeedbackByDevice, setExecutionFeedbackByDevice] = useState<
    Record<string, DeviceExecutionFeedback>
  >({});
  const [loaded, setLoaded] = useState(false);
  const [startupFailure, setStartupFailure] = useState<StartupFailure | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [setupOpen, setSetupOpen] = useState(false);
  const [setupTargetId, setSetupTargetId] = useState<string | null>(null);
  const [setupVerification, setSetupVerification] =
    useState<DeviceSetupVerification>({ status: "waiting" });
  const [profileCreatorOpen, setProfileCreatorOpen] = useState(false);
  const [profileCreatorSourceId, setProfileCreatorSourceId] = useState<string | null>(null);
  const profileHistory = useProfileHistory([], { maxSnapshots: 50 });
  const productConfigHistory = useProductConfigHistory([], { maxSnapshots: 50 });
  const pressedOwnersRef = useRef<Map<string, PressedOwner>>(new Map());
  const mountedRef = useRef(true);
  const registryEpochRef = useRef(0);
  const refreshInFlightRef = useRef(false);
  const refreshPendingRef = useRef(false);
  const refreshFullSnapshotPendingRef = useRef(false);
  const fullSnapshotRequiredRef = useRef(true);
  const refreshPromiseRef = useRef<Promise<void> | null>(null);
  const loadErrorMessageRef = useRef<string | null>(null);
  const homeMetricsRef = useRef(homeMetrics);
  const hardwareEditorTargetRef = useRef<HardwareEditorTarget | null>(null);
  const learningEditingRevisionRef = useRef(0);
  const profileDraftsRef = useRef<Map<string, DeviceProfile>>(new Map());
  const autosaveTargetRef = useRef<ProfileAutosaveTarget>({ profiles: [] });
  const persistedProfileSavesRef = useRef<Map<string, PersistedProfileSave>>(new Map());
  const setupSeenRef = useRef<Set<string>>(new Set());
  const setupOpenRef = useRef(false);
  const setupTargetIdRef = useRef<string | null>(null);
  const setupVerificationTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const setupVerificationButtonRef = useRef<{
    buttonId: string;
    buttonLabel: string;
  } | null>(null);

  const { deviceProfiles, productConfigurations, editorProfile, boardProfiles, devices, candidates } = registry;
  const profileById = useCallback((profileId: string | null) => {
    if (!profileId) return undefined;
    return profileDraftsRef.current.get(profileId) ??
      registry.deviceProfiles.find((profile) => profile.profile.id === profileId);
  }, [registry.deviceProfiles]);
  const editorProfileConfig = useMemo(
    () => profileById(editorProfile),
    [editorProfile, profileById],
  );
  const selectedLearningDevice = hardwareEditorTarget?.deviceProfileId === editorProfile
    ? devices.find((device) => device.deviceId === hardwareEditorTarget.deviceId)
    : undefined;
  const editorLearningActive = devices.some((device) =>
    device.learning?.deviceProfileId === editorProfileConfig?.profile.id
  );
  const summary = deviceSummary(devices);
  const attentionCount = summary.attention + candidates.length;
  const currentSetupPresence = useMemo(
    () => setupPresence(devices, candidates),
    [devices, candidates],
  );

  useEffect(() => {
    if (client || !loaded) return;
    const next = reconcileSetupSession(
      setupSeenRef.current,
      currentSetupPresence,
    );
    setupSeenRef.current = next.seen;
    if (!setupOpen && next.autoTargetId) {
      setSetupTargetId(next.autoTargetId);
      setSetupOpen(true);
    }
  }, [client, currentSetupPresence, loaded, setupOpen]);

  const openSetup = useCallback((targetId: string | null = null) => {
    if (targetId) setupSeenRef.current.add(targetId);
    if (setupVerificationTimerRef.current) {
      clearTimeout(setupVerificationTimerRef.current);
      setupVerificationTimerRef.current = null;
    }
    setSetupVerification({ status: "waiting" });
    setupVerificationButtonRef.current = null;
    setSetupTargetId(targetId);
    setSetupOpen(true);
  }, []);

  const beginSetupVerification = useCallback(() => {
    if (setupVerificationTimerRef.current) {
      clearTimeout(setupVerificationTimerRef.current);
    }
    setSetupVerification({ status: "waiting" });
    setupVerificationButtonRef.current = null;
    setupVerificationTimerRef.current = setTimeout(() => {
      setupVerificationTimerRef.current = null;
      if (mountedRef.current) setSetupVerification({ status: "timeout" });
    }, 15_000);
  }, []);

  const closeSetup = useCallback(() => {
    if (setupVerificationTimerRef.current) {
      clearTimeout(setupVerificationTimerRef.current);
      setupVerificationTimerRef.current = null;
    }
    setSetupOpen(false);
  }, []);

  const replaceRegistrySnapshot = useCallback((snapshot: AppSnapshot, preserveProductDrafts = false) => {
    registryEpochRef.current += 1;
    const visibleProductConfigurations = preserveProductDrafts
      ? snapshot.productConfigurations.map((config) =>
          productConfigHistory.get(config.id) ?? config
        )
      : snapshot.productConfigurations;
    const visibleProductConfigurationsById = new Map(
      visibleProductConfigurations.map((config) => [config.id, config]),
    );
    const visibleDevices = preserveProductDrafts
      ? snapshot.devices.map((device) => {
          const config = device.productConfigurationId
            ? visibleProductConfigurationsById.get(device.productConfigurationId)
            : undefined;
          return config ? { ...device, productConfig: config } : device;
        })
      : snapshot.devices;
    productConfigHistory.sync(productConfigHistoryEntries(visibleProductConfigurations));
    setRegistry((current) => ({
      ...current,
      productConfigurations: visibleProductConfigurations,
      boardProfiles: snapshot.boardProfiles,
      devices: visibleDevices,
      candidates: snapshot.candidates,
    }));
    setUsage(snapshot.usage ?? null);
    const currentDevices = new Map(snapshot.devices.map((device) => [device.deviceId, device]));
    const nextOwners = new Map(pressedOwnersRef.current);
    for (const [deviceId, owner] of nextOwners) {
      const currentDevice = currentDevices.get(deviceId);
      const currentIdentity = currentDevice
        ? runtimeFeedbackIdentity(currentDevice)
        : null;
      if (currentDevice?.connection !== "online" ||
        currentIdentity?.deviceProfileId !== owner.deviceProfileId ||
        currentIdentity.hardwareProfileId !== owner.hardwareProfileId) {
        nextOwners.delete(deviceId);
      }
    }
    pressedOwnersRef.current = nextOwners;
    setPressedButtonIds(pressedButtons(nextOwners));
  }, [productConfigHistory.get, productConfigHistory.sync]);

  const applySnapshot = useCallback((
    snapshot: AppSnapshot,
    preserveDrafts = false,
    preserveHistory = preserveDrafts,
  ) => {
    registryEpochRef.current += 1;
    const serverProfiles = snapshot.deviceProfiles.map(normalizeDeviceProfile);
    const visibleProductConfigurations = preserveHistory
      ? snapshot.productConfigurations.map((config) =>
          productConfigHistory.get(config.id) ?? config
        )
      : snapshot.productConfigurations;
    const visibleProductConfigurationsById = new Map(
      visibleProductConfigurations.map((config) => [config.id, config]),
    );
    const visibleDevices = preserveHistory
      ? snapshot.devices.map((device) => {
          const config = device.productConfigurationId
            ? visibleProductConfigurationsById.get(device.productConfigurationId)
            : undefined;
          return config ? { ...device, productConfig: config } : device;
        })
      : snapshot.devices;
    const snapshotProfileIds = new Set(serverProfiles.map((profile) => profile.profile.id));
    if (preserveDrafts) {
      for (const profileId of profileDraftsRef.current.keys()) {
        if (!snapshotProfileIds.has(profileId)) profileDraftsRef.current.delete(profileId);
      }
      for (const profileId of persistedProfileSavesRef.current.keys()) {
        if (!snapshotProfileIds.has(profileId)) persistedProfileSavesRef.current.delete(profileId);
      }
      setCapturedDraftProfileIds((current) => {
        const next = new Set([...current].filter((profileId) => snapshotProfileIds.has(profileId)));
        return next.size === current.size ? current : next;
      });
      setPendingSharedDraftProfileIds((current) => {
        const next = new Set([...current].filter((profileId) => snapshotProfileIds.has(profileId)));
        return next.size === current.size ? current : next;
      });
    } else {
      profileDraftsRef.current.clear();
      persistedProfileSavesRef.current.clear();
      setCapturedDraftProfileIds(new Set());
      setPendingSharedDraftProfileIds(new Set());
    }
    const visibleProfiles = serverProfiles.map((profile) =>
        preserveDrafts ? profileDraftsRef.current.get(profile.profile.id) ?? profile : profile
      );
    if (preserveHistory) {
      profileHistory.sync(visibleProfiles);
      productConfigHistory.sync(productConfigHistoryEntries(visibleProductConfigurations));
    } else {
      profileHistory.reset(visibleProfiles);
      productConfigHistory.reset(productConfigHistoryEntries(snapshot.productConfigurations));
    }
    setRegistry({
      deviceProfiles: visibleProfiles,
      productConfigurations: visibleProductConfigurations,
      editorProfile: snapshot.editorProfile,
      boardProfiles: snapshot.boardProfiles,
      devices: visibleDevices,
      candidates: snapshot.candidates,
    });
    const defaultDeviceId =
      visibleDevices.find((device) => device.connection === "online")?.deviceId ??
      visibleDevices[0]?.deviceId ??
      null;
    setSelectedManagedDeviceId((current) =>
      current && visibleDevices.some((device) => device.deviceId === current)
        ? current
        : defaultDeviceId,
    );
    setSavedDeviceProfiles(Object.fromEntries(serverProfiles.map((profile) =>
      [profile.profile.id, JSON.stringify(profile)]
    )));
    setLanguage(snapshot.language);
    setHomeMetrics(snapshot.homeMetrics);
    setUsage(snapshot.usage ?? null);
    pressedOwnersRef.current = new Map();
    setPressedButtonIds(new Set());
  }, [productConfigHistory.get, productConfigHistory.reset, productConfigHistory.sync, profileHistory.reset, profileHistory.sync]);

  const refreshRegistry = useCallback(function refreshRegistryTask(
    queueIfBusy = false,
    fullSnapshot = false,
  ): Promise<void> {
    if (PREVIEW_MODE) return Promise.resolve();
    if (refreshInFlightRef.current) {
      if (queueIfBusy) {
        refreshPendingRef.current = true;
        refreshFullSnapshotPendingRef.current ||= fullSnapshot;
      }
      return refreshPromiseRef.current ?? Promise.resolve();
    }
    refreshInFlightRef.current = true;
    const requestFullSnapshot = fullSnapshot || fullSnapshotRequiredRef.current;
    const requestEpoch = registryEpochRef.current;
    const request = (async () => {
      try {
        const snapshot = await invoke<AppSnapshot>("get_snapshot");
        if (mountedRef.current && requestEpoch === registryEpochRef.current) {
          if (requestFullSnapshot) {
            applySnapshot(snapshot);
            fullSnapshotRequiredRef.current = false;
            const loadErrorMessage = loadErrorMessageRef.current;
            loadErrorMessageRef.current = null;
            if (loadErrorMessage) {
              setError((current) => current === loadErrorMessage ? null : current);
            }
          }
          else replaceRegistrySnapshot(snapshot);
        }
      } catch (refreshError) {
        if (mountedRef.current && requestFullSnapshot) {
          fullSnapshotRequiredRef.current = true;
          const message = `${t("zh-CN", "error.load")}: ${errorMessage(refreshError)}`;
          loadErrorMessageRef.current = message;
          setError(message);
        }
      } finally {
        refreshInFlightRef.current = false;
        refreshPromiseRef.current = null;
        if (mountedRef.current && refreshPendingRef.current) {
          refreshPendingRef.current = false;
          const pendingFullSnapshot = refreshFullSnapshotPendingRef.current;
          refreshFullSnapshotPendingRef.current = false;
          void refreshRegistryTask(false, pendingFullSnapshot);
        }
      }
    })();
    refreshPromiseRef.current = request;
    return request;
  }, [applySnapshot, replaceRegistrySnapshot]);

  const saveProfiles = useCallback(async (profiles: DeviceProfile[]) => {
    if (profiles.length === 0) return;
    let savedSnapshot: AppSnapshot | null = null;
    if (!PREVIEW_MODE) {
      for (const profile of profiles) {
        const serialized = JSON.stringify(profile);
        const persisted = persistedProfileSavesRef.current.get(profile.profile.id);
        if (persisted?.serialized === serialized) {
          savedSnapshot = persisted.snapshot;
          continue;
        }
        savedSnapshot = await invoke<AppSnapshot>("save_device_profile", { profile });
        persistedProfileSavesRef.current.set(profile.profile.id, {
          serialized,
          snapshot: savedSnapshot,
        });
      }
    }
    if (savedSnapshot && mountedRef.current) replaceRegistrySnapshot(savedSnapshot);
    const serializedProfiles = new Map(profiles.map((profile) =>
      [profile.profile.id, JSON.stringify(profile)]
    ));
    const settledProfileIds = new Set<string>();
    for (const [profileId, serialized] of serializedProfiles) {
      const currentDraft = profileDraftsRef.current.get(profileId);
      if (!currentDraft || JSON.stringify(currentDraft) === serialized) {
        profileDraftsRef.current.delete(profileId);
        settledProfileIds.add(profileId);
      }
    }
    setPendingSharedDraftProfileIds((current) => {
      const next = new Set([...current].filter((profileId) => !settledProfileIds.has(profileId)));
      return next.size === current.size ? current : next;
    });
    setCapturedDraftProfileIds((current) => {
      const next = new Set([...current].filter((profileId) => !settledProfileIds.has(profileId)));
      return next.size === current.size ? current : next;
    });
    setSavedDeviceProfiles((current) => Object.fromEntries([
      ...Object.entries(current),
      ...serializedProfiles,
    ]));
    for (const [profileId, serialized] of serializedProfiles) {
      if (persistedProfileSavesRef.current.get(profileId)?.serialized === serialized) {
        persistedProfileSavesRef.current.delete(profileId);
      }
    }
  }, [replaceRegistrySnapshot]);

  const saveEditorProfile = useCallback(async (profile: DeviceProfile | undefined) => {
    if (profile) await saveProfiles([profile]);
  }, [saveProfiles]);

  const renameManagedDevice = useCallback(async (deviceId: string, name: string) => {
    try {
      const snapshot = await invoke<AppSnapshot>("rename_device", { deviceId, name });
      if (mountedRef.current) replaceRegistrySnapshot(snapshot);
    } catch (operationError) {
      setError(`${t(language, "error.save")}: ${errorMessage(operationError)}`);
      throw operationError;
    }
  }, [language, replaceRegistrySnapshot]);

  const saveManagedRuntimeAssignment = useCallback(async (
    deviceId: string,
    assignment: RuntimeAssignment,
  ) => {
    try {
      const snapshot = await invoke<AppSnapshot>("save_runtime_assignment", {
        deviceId,
        assignment,
      });
      if (mountedRef.current) replaceRegistrySnapshot(snapshot);
    } catch (operationError) {
      setError(`${t(language, "error.save")}: ${errorMessage(operationError)}`);
      throw operationError;
    }
  }, [language, replaceRegistrySnapshot]);

  const requestForgetManagedDevice = useCallback((deviceId: string) => {
    const device = devices.find((item) => item.deviceId === deviceId);
    if (!device || device.connection !== "offline") return;
    setConfirmation({ kind: "forget", device });
  }, [devices]);

  const selectManagedProductConfiguration = useCallback(async (
    deviceId: string,
    configurationId: string,
  ) => {
    try {
      const snapshot = await queue.enqueue(() => invoke<AppSnapshot>("select_product_configuration", {
        deviceId,
        configurationId,
      }));
      if (mountedRef.current) applySnapshot(snapshot, true);
    } catch (operationError) {
      setError(`${t(language, "error.save")}: ${errorMessage(operationError)}`);
      throw operationError;
    }
  }, [applySnapshot, language, queue]);

  const createManagedProductConfiguration = useCallback(async (request: {
    deviceId: string;
    name: string;
    copyCurrent: boolean;
  }) => {
    try {
      const snapshot = await queue.enqueue(() => invoke<AppSnapshot>("create_product_configuration", {
        request: {
          device_id: request.deviceId,
          name: request.name,
          copy_current: request.copyCurrent,
        },
      }));
      if (mountedRef.current) applySnapshot(snapshot);
    } catch (operationError) {
      setError(`${t(language, "error.save")}: ${errorMessage(operationError)}`);
      throw operationError;
    }
  }, [applySnapshot, language, queue]);

  const handleHardwareEditorSelection = useCallback((
    hardwareProfileId: string | null,
    deviceId: string | null,
  ) => {
    const managedDevice = deviceId ? devices.find((device) => device.deviceId === deviceId) : undefined;
    const deviceProfileId = managedDevice?.runtimeAssignment?.device_profile_id ?? editorProfileConfig?.profile.id;
    const next = hardwareProfileId && deviceProfileId
      ? { deviceProfileId, hardwareProfileId, deviceId }
      : null;
    setHardwareEditorTarget((current) =>
      current?.deviceProfileId === next?.deviceProfileId &&
      current?.hardwareProfileId === next?.hardwareProfileId &&
      current?.deviceId === next?.deviceId
        ? current
        : next
    );
  }, [devices, editorProfileConfig?.profile.id]);

  const dirtyProfiles = useMemo(() => deviceProfiles.filter((profile) =>
    savedDeviceProfiles[profile.profile.id] !== JSON.stringify(profile)
  ), [deviceProfiles, savedDeviceProfiles]);
  const autosaveProfiles = useMemo(() => dirtyProfiles.filter((profile) => {
    const profileId = profile.profile.id;
    const isCurrentlyShared = devices.filter(
      (device) => device.runtimeAssignment?.device_profile_id === profileId,
    ).length > 1;
    return !capturedDraftProfileIds.has(profileId) &&
      (!pendingSharedDraftProfileIds.has(profileId) || !isCurrentlyShared) &&
      !tentativeLearningCounts.has(profileId) &&
      !devices.some((device) => device.learning?.deviceProfileId === profileId) &&
      isValidDraft(profile, boardProfiles);
  }), [
    boardProfiles,
    capturedDraftProfileIds,
    devices,
    dirtyProfiles,
    pendingSharedDraftProfileIds,
    tentativeLearningCounts,
  ]);
  const autosaveTarget = useMemo<ProfileAutosaveTarget>(() => {
    if (autosaveProfiles.length > 0) {
      const target = { profiles: autosaveProfiles };
      autosaveTargetRef.current = target;
      return target;
    }
    if (dirtyProfiles.length === 0) return autosaveTargetRef.current;
    return { profiles: [] };
  }, [autosaveProfiles, dirtyProfiles.length]);
  const autosave = useAutosave<ProfileAutosaveTarget>({
    value: autosaveTarget,
    valid: autosaveProfiles.length > 0,
    save: (target) => saveProfiles(target.profiles),
    queue,
  });

  const navigate = useCallback((nextView: View) => {
    void autosave.flush()
      .then(() => {
        if (mountedRef.current) setView(nextView);
      })
      .catch((navigationError) => {
        if (mountedRef.current) setError(`${t(language, "error.save")}: ${errorMessage(navigationError)}`);
      });
  }, [autosave.flush, language]);

  const retrySetupCandidate = useCallback(
    async (deviceId: string) => {
      const snapshot = await invoke<AppSnapshot>("retry_candidate", { deviceId });
      if (mountedRef.current) applySnapshot(snapshot, true);
    },
    [applySnapshot],
  );

  const createDeviceProfile = useCallback(
    async (request: CreateDeviceProfileRequest) => {
      await autosave.flush();
      if (PREVIEW_MODE) {
        const { createPreviewDeviceProfile } = await import("./preview");
        const snapshot = createPreviewDeviceProfile(
          { ...registry, language, homeMetrics },
          request,
        );
        if (mountedRef.current) applySnapshot(snapshot, true);
        return snapshot;
      }
      const snapshot = await invoke<AppSnapshot>("create_device_profile", {
        request,
      });
      if (mountedRef.current) applySnapshot(snapshot, true);
      return snapshot;
    },
    [applySnapshot, autosave, homeMetrics, language, registry],
  );

  const prepareSetupProfile = useCallback(async (
    deviceId: string,
    deviceName: string,
    sourceProfileId: string | null,
    preferredHardwareProfileId: string | null,
  ): Promise<RuntimeAssignment> => {
    const targetDevice = registry.devices.find((device) => device.deviceId === deviceId);
    if (!targetDevice) throw new Error("unknown_device");

    const compatibleSources = registry.deviceProfiles.filter((profile) =>
      profile.hardware_profiles.some(
        (hardware) => hardware.board_profile_id === targetDevice.boardProfileId,
      ),
    );
    const requestedSource = sourceProfileId
      ? compatibleSources.find((profile) => profile.profile.id === sourceProfileId)
      : null;
    if (sourceProfileId && !requestedSource) throw new Error("hardware_resolution_required");

    // A blank onboarding choice still needs a physical layout to verify. Reuse a
    // compatible layout, clear its actions, and keep the source profile untouched.
    const source = requestedSource ?? compatibleSources.find(
      (profile) => profile.profile.groups.some((group) => group.buttons.length > 0),
    ) ?? null;
    const createdSnapshot = await createDeviceProfile(
      source
        ? {
            kind: "clone",
            name: t(language, "setup.dedicatedProfileName", { name: deviceName }),
            source_profile_id: source.profile.id,
          }
        : {
            kind: "blank",
            name: t(language, "setup.dedicatedProfileName", { name: deviceName }),
            board_profile_id: targetDevice.boardProfileId,
          },
    );
    const created = createdSnapshot.deviceProfiles.find(
      (profile) => profile.profile.id === createdSnapshot.editorProfile,
    );
    if (!created) throw new Error("unknown_profile");

    const compatibleHardware = created.hardware_profiles.filter(
      (hardware) => hardware.board_profile_id === targetDevice.boardProfileId,
    );
    const hardware =
      compatibleHardware.find(
        (candidate) => candidate.id === preferredHardwareProfileId,
      ) ??
      compatibleHardware[0];
    if (!hardware) throw new Error("hardware_resolution_required");

    const firstButton = created.profile.groups.flatMap((group) => group.buttons)[0];
    const hasActions = Object.values(created.actions).some((triggers) =>
      Object.values(triggers).some((actions) => actions.length > 0),
    );
    const prepared: DeviceProfile = {
      ...created,
      actions: requestedSource
        ? created.actions
        : firstButton
          ? {
              [firstButton.id]: {
                ...emptyTriggerActions(),
                press: [{ type: "paste", text: t(language, "setup.sampleText") }],
              },
            }
          : {},
    };
    if (!hasActions && requestedSource && firstButton) {
      prepared.actions = {
        ...prepared.actions,
        [firstButton.id]: {
          ...emptyTriggerActions(),
          press: [{ type: "paste", text: t(language, "setup.sampleText") }],
        },
      };
    }

    if (JSON.stringify(prepared) !== JSON.stringify(created)) {
      if (PREVIEW_MODE) {
        profileDraftsRef.current.set(prepared.profile.id, prepared);
        applySnapshot({
          ...createdSnapshot,
          deviceProfiles: createdSnapshot.deviceProfiles.map((profile) =>
            profile.profile.id === prepared.profile.id ? prepared : profile
          ),
        }, true);
      } else {
        const savedSnapshot = await invoke<AppSnapshot>("save_device_profile", {
          profile: prepared,
        });
        if (mountedRef.current) applySnapshot(savedSnapshot, true);
      }
    }

    return {
      device_profile_id: prepared.profile.id,
      hardware_profile_id: hardware.id,
    };
  }, [applySnapshot, createDeviceProfile, language, registry.deviceProfiles, registry.devices]);

  const profileRef = useRef(editorProfileConfig);
  const profilesRef = useRef(deviceProfiles);
  const devicesRef = useRef(devices);
  const selectedRef = useRef(selectedButtonId);
  const viewRef = useRef(view);
  profileRef.current = editorProfileConfig;
  homeMetricsRef.current = homeMetrics;
  profilesRef.current = deviceProfiles;
  devicesRef.current = devices;
  hardwareEditorTargetRef.current = hardwareEditorTarget;
  selectedRef.current = selectedButtonId;
  viewRef.current = view;
  setupOpenRef.current = setupOpen;
  setupTargetIdRef.current = setupTargetId;

  useEffect(() => {
    mountedRef.current = true;
    let active = true;
    let unlisten: (() => void) | undefined;
    let refreshTimer: ReturnType<typeof setInterval> | undefined;
    void (async () => {
      try {
        if (PREVIEW_MODE) {
          applySnapshot((await import("./preview")).previewSnapshot);
          return;
        }
        const startup = await invoke<StartupFailure | null>("get_startup_failure")
          .catch(() => null);
        if (!active) return;
        if (startup?.code) {
          setStartupFailure(startup);
          return;
        }
        refreshTimer = setInterval(() => {
          void refreshRegistry();
        }, REGISTRY_REFRESH_MS);
        const registeredUnlisten = await listen<RuntimeEvent>("runtime-event", ({ payload }) => {
          if (!active) return;
          if (payload.homeUpdate && payload.deviceProfileId === profileRef.current?.profile.id) {
            setHomeMetrics(payload.homeUpdate);
          }
          if (payload.input && payload.pressed !== null) {
            const currentEditorTarget = hardwareEditorTargetRef.current;
            const activeLearningTarget = devicesRef.current.find(
              ({ deviceId }) => deviceId === currentEditorTarget?.deviceId,
            )?.learning;
            const currentProfile = activeLearningTarget
              ? profileDraftsRef.current.get(activeLearningTarget.deviceProfileId) ?? profilesRef.current.find((profile) => profile.profile.id === activeLearningTarget.deviceProfileId)
              : profileRef.current;
            if (payload.code === "learning_input" && payload.pressed && payload.learningTarget
              && selectedRef.current && viewRef.current === "devices" && currentProfile && currentEditorTarget?.deviceId
              && activeLearningTarget && learningTargetsMatch(payload.learningTarget, activeLearningTarget)
              && payload.deviceId === activeLearningTarget.deviceId
              && activeLearningTarget.deviceId === currentEditorTarget.deviceId
              && activeLearningTarget.deviceProfileId === currentProfile.profile.id
              && activeLearningTarget.deviceProfileId === currentEditorTarget.deviceProfileId
              && activeLearningTarget.hardwareProfileId === currentEditorTarget.hardwareProfileId) {
              const learned = learnInput(
                currentProfile,
                activeLearningTarget.hardwareProfileId,
                selectedRef.current,
                payload.input,
              );
              // A runtime-event refresh can be in flight. Reject its older
              // snapshot so it cannot overwrite this unsaved learning step.
              registryEpochRef.current += 1;
              profileHistory.record(learned);
              profileDraftsRef.current.set(learned.profile.id, learned);
              setCapturedDraftProfileIds((current) => new Set(current).add(learned.profile.id));
              setRegistry((current) => ({
                ...current,
                deviceProfiles: current.deviceProfiles.map((profile) =>
                  profile.profile.id === learned.profile.id ? learned : profile
                ),
              }));
            }
            const emittingDevice = devicesRef.current.find((device) => device.deviceId === payload.deviceId);
            const productTarget = emittingDevice
              ? productRuntimeFeedbackTarget(emittingDevice)
              : null;
            const eventIdentity = emittingDevice
              ? runtimeFeedbackIdentity(emittingDevice)
              : null;
            const eventProfile = productTarget?.profile ?? (eventIdentity
              ? profileDraftsRef.current.get(eventIdentity.deviceProfileId) ?? profilesRef.current.find((profile) => profile.profile.id === eventIdentity.deviceProfileId)
              : undefined);
            if (payload.code === "input_state" && eventProfile
              && payload.hardwareProfileId
              && eventIdentity?.deviceProfileId === payload.deviceProfileId
              && eventIdentity.hardwareProfileId === payload.hardwareProfileId) {
              const eventHardware = eventProfile.hardware_profiles.find((hardware) =>
                hardware.id === payload.hardwareProfileId
              );
              const buttonId = resolveButton(eventHardware, payload.input);
              if (buttonId) {
                const nextOwners = new Map(pressedOwnersRef.current);
                const currentOwner = nextOwners.get(payload.deviceId);
                const owner = currentOwner?.deviceProfileId === payload.deviceProfileId
                  && currentOwner.hardwareProfileId === payload.hardwareProfileId
                  ? currentOwner
                  : {
                    deviceProfileId: payload.deviceProfileId,
                    hardwareProfileId: payload.hardwareProfileId,
                    buttonIds: new Set<string>(),
                  };
                const buttonIds = new Set(owner.buttonIds);
                if (payload.pressed) buttonIds.add(buttonId);
                else buttonIds.delete(buttonId);
                if (buttonIds.size > 0) nextOwners.set(payload.deviceId, { ...owner, buttonIds });
                else nextOwners.delete(payload.deviceId);
                pressedOwnersRef.current = nextOwners;
                setPressedButtonIds(pressedButtons(nextOwners));
                if (payload.pressed) {
                  setExecutionFeedbackByDevice((current) => {
                    if (!current[payload.deviceId]) return current;
                    const next = { ...current };
                    delete next[payload.deviceId];
                    return next;
                  });
                }
                if (
                  payload.pressed &&
                  setupOpenRef.current &&
                  setupTargetIdRef.current === payload.deviceId
                ) {
                  const label = eventProfile.profile.groups
                    .flatMap((group) => group.buttons)
                    .find((button) => button.id === buttonId)?.label ?? buttonId;
                  setupVerificationButtonRef.current = {
                    buttonId,
                    buttonLabel: label,
                  };
                  const actions = eventProfile.actions[buttonId];
                  const actionCount = actions
                    ? actions.press.length + actions.release.length +
                      actions.long_press.length + actions.double_press.length
                    : 0;
                  if (actionCount === 0 && setupVerificationTimerRef.current) {
                    clearTimeout(setupVerificationTimerRef.current);
                    setupVerificationTimerRef.current = null;
                  }
                  setSetupVerification({
                    status: actionCount > 0 ? "waiting" : "success",
                    buttonId,
                    buttonLabel: label,
                  });
                }
              }
            }
          }
          if (
            payload.code === "action_step_completed" &&
            setupOpenRef.current &&
            setupTargetIdRef.current === payload.deviceId &&
            setupVerificationButtonRef.current &&
            (!payload.params.button ||
              payload.params.button === setupVerificationButtonRef.current.buttonId)
          ) {
            if (setupVerificationTimerRef.current) {
              clearTimeout(setupVerificationTimerRef.current);
              setupVerificationTimerRef.current = null;
            }
            setSetupVerification({
              status: "success",
              ...setupVerificationButtonRef.current,
            });
          }
          if (payload.code === "action_step_completed") {
            setExecutionFeedbackByDevice((current) => ({
              ...current,
              [payload.deviceId]: {
                buttonId: payload.params.button ?? null,
                status: "success",
                detail: null,
              },
            }));
          }
          if (payload.level === "error" && payload.params.button) {
            setExecutionFeedbackByDevice((current) => ({
              ...current,
              [payload.deviceId]: {
                buttonId: payload.params.button,
                status: "error",
                detail: payload.detail ?? payload.code,
              },
            }));
          }
          if (
            payload.level === "error" &&
            setupOpenRef.current &&
            setupTargetIdRef.current === payload.deviceId
          ) {
            if (setupVerificationTimerRef.current) {
              clearTimeout(setupVerificationTimerRef.current);
              setupVerificationTimerRef.current = null;
            }
            setSetupVerification({
              status: "error",
              detail: payload.detail ?? payload.code,
            });
          }
          void refreshRegistry(true);
        });
        if (!active) {
          registeredUnlisten();
          return;
        }
        unlisten = registeredUnlisten;
        await refreshRegistry(true, true);
      } catch (loadError) {
        if (active) {
          const message = `${t("zh-CN", "error.load")}: ${errorMessage(loadError)}`;
          loadErrorMessageRef.current = message;
          setError(message);
        }
      } finally {
        if (active) setLoaded(true);
      }
    })();
    return () => {
      active = false;
      mountedRef.current = false;
      refreshPendingRef.current = false;
      refreshFullSnapshotPendingRef.current = false;
      if (refreshTimer) clearInterval(refreshTimer);
      if (setupVerificationTimerRef.current) clearTimeout(setupVerificationTimerRef.current);
      unlisten?.();
    };
  }, [applySnapshot, profileHistory.record, refreshRegistry]);

  const updateProfile = useCallback((profileId: string, update: (profile: DeviceProfile) => DeviceProfile) => {
    if (!editorLearningActive || profileId !== editorProfileConfig?.profile.id) {
      setCapturedDraftProfileIds((current) => {
        if (!current.has(profileId)) return current;
        const next = new Set(current);
        next.delete(profileId);
        return next;
      });
    }
    const currentProfile = profileById(profileId);
    if (!currentProfile) return;
    const updatedProfile = update(currentProfile);
    registryEpochRef.current += 1;
    profileHistory.record(updatedProfile);
    profileDraftsRef.current.set(profileId, updatedProfile);
    setRegistry((current) => ({
      ...current,
      deviceProfiles: current.deviceProfiles.map((profile) =>
        profile.profile.id === profileId ? updatedProfile : profile
      ),
    }));
  }, [editorLearningActive, editorProfileConfig?.profile.id, profileById, profileHistory.record]);

  const setSharedDraftPending = useCallback((profileId: string, pending: boolean) => {
    setPendingSharedDraftProfileIds((current) => {
      const next = new Set(current);
      if (pending) next.add(profileId);
      else next.delete(profileId);
      return next;
    });
  }, []);

  const changeManagedProfile = useCallback((profile: DeviceProfile) => {
    updateProfile(profile.profile.id, () => profile);
    const sharedDeviceCount = devices.filter(
      (device) => device.runtimeAssignment?.device_profile_id === profile.profile.id,
    ).length;
    setSharedDraftPending(profile.profile.id, sharedDeviceCount > 1);
  }, [devices, setSharedDraftPending, updateProfile]);

  const saveProductConfig = useCallback((
    deviceId: string,
    config: ProductConfigurationProfile,
    recordHistory = true,
  ) => {
    const existingMetadata = devices.find((device) => device.deviceId === deviceId)?.productConfig?.snapshot_metadata;
    const nextConfig = config.snapshot_metadata === undefined
      ? { ...config, snapshot_metadata: existingMetadata }
      : config;
    if (recordHistory) productConfigHistory.record(nextConfig.id, nextConfig);
    setRegistry((current) => ({
      ...current,
      productConfigurations: current.productConfigurations.map((configuration) =>
        configuration.id === nextConfig.id ? nextConfig : configuration
      ),
      devices: current.devices.map((device) =>
        device.productConfigurationId === nextConfig.id
          ? { ...device, productConfig: nextConfig }
          : device,
      ),
    }));
    void queue.enqueue(() => invoke<AppSnapshot>("save_product_configuration", {
      deviceId,
      config: nextConfig,
    })).then((snapshot) => {
      if (mountedRef.current) replaceRegistrySnapshot(snapshot, true);
    }).catch((operationError) => {
      if (mountedRef.current) setError(`${t(language, "error.save")}: ${errorMessage(operationError)}`);
    });
  }, [devices, language, productConfigHistory.record, queue, replaceRegistrySnapshot]);

  const changeManagedActions = useCallback((profile: DeviceProfile) => {
    const productDevice = devices.find(
      (device) => device.deviceId === selectedManagedDeviceId && device.productVersionId,
    );
    if (productDevice?.productVersionId) {
      const current = productDevice.productConfig;
      if (!current) return;
      const config: ProductConfigurationProfile = {
        ...current,
        trigger_settings: profile.trigger_settings,
        actions: profile.actions,
      };
      saveProductConfig(productDevice.deviceId, config);
      return;
    }
    updateProfile(profile.profile.id, () => profile);
  }, [devices, saveProductConfig, selectedManagedDeviceId, updateProfile]);

  const saveManagedSharedProfile = useCallback(async (profile: DeviceProfile) => {
    try {
      await autosave.flush();
      await saveEditorProfile(profile);
    } catch (operationError) {
      if (mountedRef.current) {
        setError(`${t(language, "error.save")}: ${errorMessage(operationError)}`);
      }
    }
  }, [autosave.flush, language, saveEditorProfile]);

  const duplicateManagedProfileForDevice = useCallback(async ({ deviceId, sourceProfile: profile, name }: { deviceId: string; sourceProfile: DeviceProfile; name: string }) => {
    await autosave.flush();
    if (PREVIEW_MODE) {
      const { createPreviewDeviceProfile } = await import("./preview");
      const preview = createPreviewDeviceProfile(
        { ...registry, language, homeMetrics },
        { kind: "clone", name, source_profile_id: profile.profile.id },
      );
      const cloned = preview.deviceProfiles.find((item) => item.profile.name === name) ?? preview.deviceProfiles[preview.deviceProfiles.length - 1];
      const selectedDevice = preview.devices.find((device) => device.deviceId === deviceId);
      const hardware = cloned?.hardware_profiles.find((item) => item.board_profile_id === selectedDevice?.boardProfileId);
      const nextSnapshot = cloned && selectedDevice && hardware
        ? { ...preview, editorProfile: registry.editorProfile, devices: preview.devices.map((device) => device.deviceId === deviceId ? { ...device, assignment: "valid" as const, runtimeAssignment: { device_profile_id: cloned.profile.id, hardware_profile_id: hardware.id } } : device) }
        : { ...preview, editorProfile: registry.editorProfile };
      if (mountedRef.current) applySnapshot(nextSnapshot, true);
      return;
    }
    const snapshot = await invoke<AppSnapshot>("duplicate_profile_for_device", {
      request: { device_id: deviceId, source_profile: profile, name },
    });
    profileDraftsRef.current.delete(profile.profile.id);
    setPendingSharedDraftProfileIds((current) => {
      const next = new Set(current);
      next.delete(profile.profile.id);
      return next;
    });
    if (mountedRef.current) applySnapshot(snapshot, true);
  }, [applySnapshot, autosave.flush, homeMetrics, language, registry]);

  const saveSettings = async (nextEditorProfile: string | null, nextLanguage: Language) => {
    await autosave.flush();
    if (PREVIEW_MODE) {
      setRegistry((current) => ({ ...current, editorProfile: nextEditorProfile }));
      setLanguage(nextLanguage);
      return;
    }
    const snapshot = await queue.enqueue(() => invoke<AppSnapshot>("save_settings", {
      settings: { schema_version: 4, editor_profile: nextEditorProfile, language: nextLanguage },
    }));
    applySnapshot(snapshot, true);
  };

  const saveUsageSettings = useCallback(async (settings: UsageSettingsPatch) => {
    if (PREVIEW_MODE) return;
    try {
      const snapshot = await invoke<AppSnapshot>("save_usage_settings", { settings });
      if (mountedRef.current) applySnapshot(snapshot, true);
    } catch (operationError) {
      setError(`${t(language, "error.save")}: ${errorMessage(operationError)}`);
      throw operationError;
    }
  }, [applySnapshot, language]);

  const completeDeviceSetup = async (
    deviceId: string,
    name: string,
    assignment: RuntimeAssignment,
  ) => {
    await autosave.flush();
    const completed = PREVIEW_MODE
      ? {
          deviceProfiles: deviceProfiles.some((profile) => profile.profile.id === assignment.device_profile_id)
            ? deviceProfiles
            : [
                ...deviceProfiles,
                ...(profileDraftsRef.current.get(assignment.device_profile_id)
                  ? [profileDraftsRef.current.get(assignment.device_profile_id)!]
                  : []),
              ],
          productConfigurations,
          editorProfile: registry.editorProfile,
          boardProfiles,
          devices: devices.map((device) => device.deviceId === deviceId
            ? {
                ...device,
                name,
                assignment: "valid" as const,
                runtime: "configuring" as const,
                runtimeAssignment: assignment,
              }
            : device),
          candidates,
          language,
          homeMetrics,
        }
      : await invoke<AppSnapshot>("complete_device_setup", {
          deviceId,
          name,
          assignment,
        });
    if (!mountedRef.current) return;
    applySnapshot(completed, true);
    if (completed.editorProfile !== assignment.device_profile_id) {
      try {
        await saveSettings(assignment.device_profile_id, language);
      } catch (settingsError) {
        setRegistry((current) => ({
          ...current,
          editorProfile: assignment.device_profile_id,
        }));
        setError(
          `${t(language, "error.save")}: ${errorMessage(settingsError)}`,
        );
      }
    }
    setSelectedManagedDeviceId(deviceId);
    setHardwareEditorTarget({
      deviceId,
      deviceProfileId: assignment.device_profile_id,
      hardwareProfileId: assignment.hardware_profile_id,
    });
    beginSetupVerification();
    setView("devices");
  };

  const run = async (label: string, task: () => Promise<void>) => {
    setError(null);
    try {
      await task();
    } catch (operationError) {
      setError(`${label}: ${errorMessage(operationError)}`);
    }
  };

  const beginManagedLearning = useCallback((hardwareProfileId: string, deviceId: string, pins: number[]) => {
    const selectedDevice = devices.find((device) => device.deviceId === deviceId);
    const profileId = selectedDevice?.runtimeAssignment?.device_profile_id ?? editorProfileConfig?.profile.id;
    const profile = profileId ? profileById(profileId) : undefined;
    const hardware = profile?.hardware_profiles.find(({ id }) => id === hardwareProfileId);
    if (!profile || !hardware || !selectedDevice || selectedDevice.connection !== "online" || selectedDevice.mode !== "runtime" || selectedDevice.identity !== "valid" || selectedDevice.boardProfileId !== hardware.board_profile_id) return;
    const editingRevision = ++learningEditingRevisionRef.current;
    setHardwareEditorTarget({ deviceId, deviceProfileId: profile.profile.id, hardwareProfileId });
    setTentativeLearningCounts((current) => {
      const next = new Map(current);
      next.set(profile.profile.id, (next.get(profile.profile.id) ?? 0) + 1);
      return next;
    });
    void (async () => {
      try {
        const snapshot = await invoke<AppSnapshot>("begin_learning", {
          deviceId,
          deviceProfileId: profile.profile.id,
          hardwareProfileId,
          editingRevision,
          pins,
        });
        if (mountedRef.current) replaceRegistrySnapshot(snapshot);
      } catch (operationError) {
        if (mountedRef.current) setError(`${t(language, "error.learning")}: ${errorMessage(operationError)}`);
      } finally {
        setTentativeLearningCounts((current) => {
          const next = new Map(current);
          const remaining = (next.get(profile.profile.id) ?? 1) - 1;
          if (remaining > 0) next.set(profile.profile.id, remaining);
          else next.delete(profile.profile.id);
          return next;
        });
      }
    })();
  }, [devices, editorProfileConfig?.profile.id, language, profileById, replaceRegistrySnapshot]);

  const endManagedLearning = useCallback((deviceId: string) => {
    void (async () => {
      try {
        const snapshot = await invoke<AppSnapshot>("end_learning", { deviceId });
        if (mountedRef.current) replaceRegistrySnapshot(snapshot);
      } catch (operationError) {
        if (mountedRef.current) setError(`${t(language, "error.learning")}: ${errorMessage(operationError)}`);
      }
    })();
  }, [language, replaceRegistrySnapshot]);

  const chooseImport = () => run(t(language, "error.import"), async () => {
    await autosave.flush();
    const path = await open({ multiple: false, filters: [{ name: "Kivo", extensions: ["yaml", "yml"] }] });
    if (!path) return;
    const preview = await invoke<ImportPreview>("preview_device_profile_import", { path });
    setConfirmation({ kind: "import", path, preview });
  });

  const chooseRestore = () => run(t(language, "error.restore"), async () => {
    await autosave.flush();
    const path = await open({ multiple: false, filters: [{ name: "Kivo", extensions: ["yaml", "yml"] }] });
    if (!path) return;
    const preview = await invoke<BackupPreview>("preview_backup", { path });
    setConfirmation({ kind: "restore", path, preview });
  });

  const exportBackup = () => run(t(language, "error.export"), async () => {
    await autosave.flush();
    const path = await saveFile({
      defaultPath: defaultBackupFilename(),
      filters: [{ name: "Kivo", extensions: ["yaml"] }],
    });
    if (path) await invoke("export_backup", { path });
  });

  const exportProfile = useCallback((profile: DeviceProfile) => run(t(language, "error.export"), async () => {
    await autosave.flush();
    const path = await saveFile({
      defaultPath: `${profile.profile.id}.yaml`,
      filters: [{ name: "Kivo", extensions: ["yaml"] }],
    });
    if (path) await invoke("export_device_profile", { id: profile.profile.id, path });
  }), [autosave.flush, language]);

  const openProfileCreator = useCallback((sourceProfileId: string | null = null) => {
    setProfileCreatorSourceId(sourceProfileId);
    setProfileCreatorOpen(true);
  }, []);

  const confirmOperation = () => {
    const current = confirmation;
    if (!current) return;
    setConfirmation(null);
    void run(
      t(language,
        current.kind === "restore"
          ? "error.restore"
          : current.kind === "delete"
            ? "error.delete"
            : current.kind === "forget"
              ? "error.forget"
              : "error.import",
      ),
      async () => {
        const snapshot = current.kind === "import"
          ? await invoke<AppSnapshot>("import_device_profile", { path: current.path })
          : current.kind === "restore"
            ? await invoke<AppSnapshot>("restore_backup", { path: current.path })
            : current.kind === "delete"
              ? await invoke<AppSnapshot>("delete_device_profile", { id: current.profile.profile.id })
              : await invoke<AppSnapshot>("forget_device", { deviceId: current.device.deviceId });
        applySnapshot(
          snapshot,
          current.kind === "forget",
          current.kind === "import" || current.kind === "delete" || current.kind === "forget",
        );
      },
    );
  };

  const workspacePressedButtonIds = useMemo(
    () => new Set(
      selectedManagedDeviceId
        ? pressedOwnersRef.current.get(selectedManagedDeviceId)?.buttonIds ?? []
        : [],
    ),
    [pressedButtonIds, selectedManagedDeviceId],
  );
  const historyProfileId = devices.find(
    (device) => device.deviceId === selectedManagedDeviceId,
  )?.runtimeAssignment?.device_profile_id ?? editorProfile;
  const selectedHistoryDevice = devices.find(
    (device) => device.deviceId === selectedManagedDeviceId,
  );
  const historyTarget: HistoryTarget = selectedHistoryDevice?.productVersionId && selectedHistoryDevice.productConfigurationId
    ? {
        kind: "product",
        deviceId: selectedHistoryDevice.deviceId,
        configurationId: selectedHistoryDevice.productConfigurationId,
      }
    : historyProfileId
      ? { kind: "profile", profileId: historyProfileId }
      : null;
  const selectedSummaryDevice = devices.find(
    (device) => device.deviceId === selectedManagedDeviceId,
  ) ?? devices.find((device) => device.connection === "online") ?? null;
  const selectedSummaryNeedsAttention = selectedSummaryDevice && (
    selectedSummaryDevice.connection === "offline" ||
    selectedSummaryDevice.assignment !== "valid" ||
    selectedSummaryDevice.runtime === "runtime_error"
  );
  const canUndoProfile = Boolean(
    historyTarget?.kind === "profile" && profileHistory.canUndo(historyTarget.profileId),
  );
  const canRedoProfile = Boolean(
    historyTarget?.kind === "profile" && profileHistory.canRedo(historyTarget.profileId),
  );
  const canUndoProduct = Boolean(
    historyTarget?.kind === "product" && productConfigHistory.canUndo(historyTarget.configurationId),
  );
  const canRedoProduct = Boolean(
    historyTarget?.kind === "product" && productConfigHistory.canRedo(historyTarget.configurationId),
  );
  const canUndoHistory = canUndoProfile || canUndoProduct;
  const canRedoHistory = canRedoProfile || canRedoProduct;

  const applyHistoryProfile = (profile: DeviceProfile | undefined) => {
    if (!profile) return;
    const profileId = profile.profile.id;
    profileDraftsRef.current.set(profileId, profile);
    setCapturedDraftProfileIds((current) => {
      if (!current.has(profileId)) return current;
      const next = new Set(current);
      next.delete(profileId);
      return next;
    });
    setSharedDraftPending(
      profileId,
      devices.filter(
        (device) => device.runtimeAssignment?.device_profile_id === profileId,
      ).length > 1,
    );
    setRegistry((current) => ({
      ...current,
      deviceProfiles: current.deviceProfiles.map((currentProfile) =>
        currentProfile.profile.id === profileId ? profile : currentProfile
      ),
    }));
  };
  const currentProfileSummary = summarizeProfiles(deviceProfiles);
  let changedProfileSummary: ProfileContentSummary | null = null;
  if (confirmation?.kind === "import") {
    changedProfileSummary = projectImportedProfiles(
      deviceProfiles,
      confirmation.preview,
    );
  } else if (
    confirmation?.kind === "restore" &&
    confirmation.preview.kind !== "product_devices"
  ) {
    changedProfileSummary = {
      profileCount: confirmation.preview.profileCount,
      buttonCount: confirmation.preview.buttonCount,
      hardwareBindingCount: confirmation.preview.hardwareBindingCount,
      actionCount: confirmation.preview.actionCount,
    };
  }
  const profileChangeSummary = changedProfileSummary
    ? t(language, "dialog.profileChangeSummary", {
        currentProfiles: currentProfileSummary.profileCount,
        nextProfiles: changedProfileSummary.profileCount,
        currentButtons: currentProfileSummary.buttonCount,
        nextButtons: changedProfileSummary.buttonCount,
        currentBindings: currentProfileSummary.hardwareBindingCount,
        nextBindings: changedProfileSummary.hardwareBindingCount,
        currentActions: currentProfileSummary.actionCount,
        nextActions: changedProfileSummary.actionCount,
      })
    : null;

  if (startupFailure) {
    const incompatible = startupFailure.code === "unsupported_profile_schema" ||
      startupFailure.code === "unsupported_settings_schema";
    return (
      <main className={`startup-failure-shell${embedded ? " is-embedded" : ""}`}>
        {!embedded ? (
          <header className="startup-failure-brand">
            <img src={brandIcon} alt="" />
            <span>Kivo</span>
          </header>
        ) : null}
        <section className="startup-failure-content" role="alert">
          <AlertTriangle size={28} aria-hidden="true" />
          <div>
            <h1>Kivo 无法启动</h1>
            {incompatible ? (
              <>
                <p>当前配置由较新版本创建。请更新 Kivo 后重试。</p>
                <p>现有配置未被修改。</p>
                <p lang="en">This configuration was created by a newer version of Kivo. Update Kivo and try again. Your configuration has not been changed.</p>
              </>
            ) : (
              <>
                <p>启动初始化失败。现有配置未被修改。</p>
                <p lang="en">Kivo could not start. Your configuration has not been changed.</p>
              </>
            )}
            <code>{startupFailure.code}</code>
          </div>
        </section>
      </main>
    );
  }

  return (
    <main className={`product-shell${embedded ? " is-embedded" : ""}${client ? " is-client" : ""}${!embedded && !client ? " is-main-app" : ""}`}>
      <header className={`topbar${embedded ? " is-embedded" : ""}${client ? " is-client" : ""}`}>
        {!embedded ? <div className="brand"><img src={brandIcon} alt="" /><h1>Kivo</h1></div> : null}
        <div className="device-summary" aria-label={t(language, "device.summary")}>
          <span className="summary-current-device">
            <Keyboard size={15} aria-hidden="true" />
            {selectedSummaryDevice?.name ?? t(language, "nav.devices")}
          </span>
          {selectedSummaryNeedsAttention ? (
            <span className="summary-attention">
              <i />
              {selectedSummaryDevice.connection === "offline"
                ? t(language, "device.offline")
                : t(language, "device.attention")}
            </span>
          ) : attentionCount > 0 ? (
            <span className="summary-attention">
              <i />{attentionCount} {t(language, "device.attention")}
            </span>
          ) : null}
        </div>
        {client && (
          <div className="client-transfer-actions" aria-label={t(language, "nav.data")}>
            <button type="button" title={t(language, "nav.backupConfig")} onClick={() => void exportBackup()}>
              <DatabaseBackup size={16} />{t(language, "nav.backupConfig")}
            </button>
            <button type="button" title={t(language, "nav.restoreConfig")} onClick={() => void chooseRestore()}>
              <ArchiveRestore size={16} />{t(language, "nav.restoreConfig")}
            </button>
          </div>
        )}
        {!client && <nav className="topbar-nav" aria-label={t(language, "nav.primary")}>
          <button
            className={view === "devices" ? "is-active" : ""}
            type="button"
            aria-pressed={view === "devices"}
            onClick={() => void navigate("devices")}
          >
            <Keyboard size={16} />{t(language, "nav.devices")}
          </button>
          <button
            className={view === "data" ? "is-active" : ""}
            type="button"
            aria-pressed={view === "data"}
            title={t(language, "nav.data")}
            onClick={() => void navigate("data")}
          >
            <Settings2 size={16} />{t(language, "nav.settings")}
          </button>
        </nav>}
        {!client && (
          <div className="edit-history-actions" aria-label={t(language, "common.editHistory")}>
            <button
              className="icon-button"
              type="button"
              disabled={!canUndoHistory}
              aria-label={t(language, "common.undo")}
              title={t(language, "common.undo")}
              onClick={() => {
                if (historyTarget?.kind === "product") {
                  const config = productConfigHistory.undo(historyTarget.configurationId);
                  if (config) saveProductConfig(historyTarget.deviceId, config, false);
                } else if (historyTarget?.kind === "profile") {
                  applyHistoryProfile(profileHistory.undo(historyTarget.profileId));
                }
              }}
            >
              <UndoCircle size={18} weight="Outline" aria-hidden="true" />
            </button>
            <button
              className="icon-button"
              type="button"
              disabled={!canRedoHistory}
              aria-label={t(language, "common.redo")}
              title={t(language, "common.redo")}
              onClick={() => {
                if (historyTarget?.kind === "product") {
                  const config = productConfigHistory.redo(historyTarget.configurationId);
                  if (config) saveProductConfig(historyTarget.deviceId, config, false);
                } else if (historyTarget?.kind === "profile") {
                  applyHistoryProfile(profileHistory.redo(historyTarget.profileId));
                }
              }}
            >
              <RedoCircle size={18} weight="Outline" aria-hidden="true" />
            </button>
          </div>
        )}
        <div className={`save-state is-${autosave.status}`} aria-live="polite">
          {autosave.status === "saving" && t(language, "save.saving")}
          {autosave.status === "saved" && t(language, "save.saved")}
          {autosave.status === "error" && (
            <><span>{t(language, "save.failed")}</span><button type="button" onClick={() => void autosave.retry()}>{t(language, "save.retry")}</button></>
          )}
        </div>
      </header>

      {error && (
        <div className="error-toast" role="alert">
          <span className="error-banner">{error}</span>
          <button className="icon-button" type="button" aria-label={t(language, "common.close")} title={t(language, "common.close")} onClick={() => {
            setError(null);
          }}><X size={15} /></button>
        </div>
      )}

      <div className="product-workspace is-unified">
        <section className="content-panel">
          {!client && view === "data" ? (
            <div className="data-page">
              <div className="content-heading">
                <div>
                  <h2>{t(language, "nav.data")}</h2>
                  <p className="content-subtitle">{t(language, "data.subtitle")}</p>
                </div>
                <button className="primary-button" type="button" onClick={() => openProfileCreator()}>
                  <Plus size={16} />{t(language, "profile.create")}
                </button>
              </div>
              <div className="data-page-body">
                <UsageSettingsPanel
                  language={language}
                  usage={usage}
                  onSave={saveUsageSettings}
                />
                <section className="profile-list" aria-label={t(language, "data.profileList")}>
                  {deviceProfiles.length === 0 && <p className="empty-workspace-copy">{t(language, "model.empty")}</p>}
                  {deviceProfiles.map((profile) => {
                    const usage = devices.filter((device) => device.runtimeAssignment?.device_profile_id === profile.profile.id).length;
                    return (
                      <article className="profile-row" key={profile.profile.id}>
                        <div className="profile-row-main">
                          <div className="profile-row-title">
                            <h3>{profile.profile.name}</h3>
                            {profile.profile.id === editorProfile && <span className="profile-badge">{t(language, "data.editorBadge")}</span>}
                          </div>
                          <p>{t(language, "data.usedBy", { count: usage })}</p>
                          {formatSnapshotDate(language, profile.snapshot_metadata?.created_at) && (
                            <p>{t(language, "data.createdAt", {
                              time: formatSnapshotDate(language, profile.snapshot_metadata?.created_at) ?? "",
                            })}</p>
                          )}
                          {profile.snapshot_metadata?.source_device_name && (
                            <p>{t(language, "data.sourceDevice", {
                              name: profile.snapshot_metadata.source_device_name,
                            })}</p>
                          )}
                          <code>{profile.profile.id}</code>
                        </div>
                        <div className="profile-row-actions">
                          <button type="button" aria-label={`${t(language, "data.exportProfile")} ${profile.profile.name}`} title={t(language, "data.exportProfile")} onClick={() => void exportProfile(profile)}>
                            <Upload size={15} />{t(language, "data.exportProfile")}
                          </button>
                          <button type="button" aria-label={`${t(language, "data.duplicateProfile")} ${profile.profile.name}`} title={t(language, "data.duplicateProfile")} onClick={() => openProfileCreator(profile.profile.id)}>
                            <Plus size={15} />{t(language, "data.duplicateProfile")}
                          </button>
                          <button className="is-danger" type="button" aria-label={`${t(language, "data.deleteProfile")} ${profile.profile.name}`} title={t(language, "data.deleteProfile")} onClick={() => setConfirmation({ kind: "delete", profile })}>
                            <Trash2 size={15} />{t(language, "data.deleteProfile")}
                          </button>
                        </div>
                      </article>
                    );
                  })}
                </section>
                <section className="data-card">
                  <h3>{t(language, "data.groupTransfer")}</h3>
                  <div className="data-menu">
                    <button type="button" onClick={() => void chooseImport()}><FileInput size={16} />{t(language, "nav.import")}</button>
                    <button type="button" onClick={() => void exportBackup()}><DatabaseBackup size={16} />{t(language, "nav.backup")}</button>
                    <button type="button" onClick={() => void chooseRestore()}><ArchiveRestore size={16} />{t(language, "nav.restore")}</button>
                  </div>
                </section>
              </div>
            </div>
          ) : (
            <DeviceManagement
              client={client}
              studioMode={embedded}
              language={language}
              devices={devices}
              candidates={candidates}
              boardProfiles={boardProfiles}
              deviceProfiles={deviceProfiles}
              productConfigurations={productConfigurations}
              onRename={renameManagedDevice}
              onSaveRuntimeAssignment={saveManagedRuntimeAssignment}
              onSelectProductConfiguration={selectManagedProductConfiguration}
              onCreateProductConfiguration={createManagedProductConfiguration}
              onForgetDevice={requestForgetManagedDevice}
              onOpenSetup={openSetup}
              onCreateFromTemplate={openProfileCreator}
              onRetryCandidate={retrySetupCandidate}
              selectedDeviceId={selectedManagedDeviceId}
              onSelectedDeviceChange={setSelectedManagedDeviceId}
              onChangeProfile={changeManagedProfile}
              onChangeActions={changeManagedActions}
              onSaveSharedProfile={saveManagedSharedProfile}
              onDuplicateProfileForDevice={duplicateManagedProfileForDevice}
              onHardwareSelectionChange={handleHardwareEditorSelection}
              onBeginLearning={beginManagedLearning}
              onEndLearning={endManagedLearning}
              selectedButtonId={selectedButtonId}
              onSelectedButtonChange={setSelectedButtonId}
              pressedButtonIds={workspacePressedButtonIds}
              executionFeedback={
                selectedManagedDeviceId
                  ? executionFeedbackByDevice[selectedManagedDeviceId] ?? null
                  : null
              }
            />
          )}
        </section>
      </div>

      {!client && profileCreatorOpen && (
        <div className="modal-backdrop" role="presentation">
          <section
            className="device-setup-dialog profile-create-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="profile-create-title"
          >
            <header className="device-setup-header">
              <h2 id="profile-create-title">{t(language, "profile.create")}</h2>
              <button
                className="icon-button"
                type="button"
                aria-label={t(language, "common.close")}
                title={t(language, "common.close")}
                onClick={() => {
                  setProfileCreatorOpen(false);
                  setProfileCreatorSourceId(null);
                }}
              >
                <X size={17} />
              </button>
            </header>
            <div className="device-setup-body">
              <CreateDeviceProfileForm
                language={language}
                boardProfiles={boardProfiles}
                deviceProfiles={deviceProfiles}
                initialSourceProfileId={profileCreatorSourceId ?? undefined}
                onCreate={async (request) => {
                  await createDeviceProfile(request);
                  setProfileCreatorOpen(false);
                  setProfileCreatorSourceId(null);
                  setView("data");
                }}
                onCancel={() => {
                  setProfileCreatorOpen(false);
                  setProfileCreatorSourceId(null);
                }}
              />
            </div>
          </section>
        </div>
      )}

      {!client && (
        <DeviceSetupWizard
          open={setupOpen}
          targetId={setupTargetId}
          language={language}
          devices={devices}
          candidates={candidates}
          boardProfiles={boardProfiles}
          deviceProfiles={deviceProfiles}
          onTargetChange={setSetupTargetId}
          onRetryCandidate={retrySetupCandidate}
          onCreateProfile={createDeviceProfile}
          onPrepareProfile={prepareSetupProfile}
          onComplete={completeDeviceSetup}
          verification={setupVerification}
          onVerificationRetry={() => beginSetupVerification()}
          onClose={closeSetup}
        />
      )}

      {confirmation && (
        <ConfirmDialog
          title={confirmation.kind === "restore"
            ? t(language, confirmation.preview.kind === "product_devices"
              ? "dialog.restoreProductTitle"
              : "dialog.restoreTitle")
            : confirmation.kind === "delete"
              ? t(language, "dialog.deleteTitle")
              : confirmation.kind === "forget"
                ? t(language, "devices.forget")
                : t(language, confirmation.preview.replacesExisting ? "dialog.replaceTitle" : "dialog.importTitle")}
          body={confirmation.kind === "restore"
            ? t(language, confirmation.preview.kind === "product_devices"
              ? "dialog.restoreProductBody"
              : "dialog.restoreBody")
            : confirmation.kind === "delete"
              ? t(language, "dialog.deleteBody", { name: confirmation.profile.profile.name })
              : confirmation.kind === "forget"
                ? t(language, "devices.forgetBody", { name: confirmation.device.name })
                : t(language, confirmation.preview.replacesExisting ? "dialog.replaceBody" : "dialog.importBody")}
          summary={confirmation.kind === "restore"
            ? confirmation.preview.kind === "product_devices"
              ? t(language, "dialog.productBackupSummary", {
                actions: confirmation.preview.actionCount,
                devices: confirmation.preview.deviceCount,
              })
              : (
                <>
                  <p>{profileChangeSummary}</p>
                  <p>{t(language, "dialog.backupSummary", {
                    models: confirmation.preview.profileCount,
                    buttons: confirmation.preview.buttonCount,
                    bindings: confirmation.preview.hardwareBindingCount,
                    actions: confirmation.preview.actionCount,
                    devices: confirmation.preview.deviceCount,
                    assignments: confirmation.preview.assignmentCount,
                    metricRows: confirmation.preview.metricRowCount,
                    activity: confirmation.preview.activityCount,
                  })}</p>
                </>
              )
            : confirmation.kind === "import"
              ? profileChangeSummary ?? ""
              : confirmation.kind === "delete"
                ? confirmation.profile.profile.name
                : confirmation.device.name}
          confirmLabel={t(language, "common.confirm")}
          cancelLabel={t(language, "common.cancel")}
          danger={confirmation.kind === "delete" || confirmation.kind === "forget" ||
            (confirmation.kind === "restore" && confirmation.preview.kind !== "product_devices") ||
            (confirmation.kind === "import" && confirmation.preview.replacesExisting)}
          onCancel={() => setConfirmation(null)}
          onConfirm={confirmOperation}
        />
      )}
    </main>
  );
}
