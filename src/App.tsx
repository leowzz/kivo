import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save as saveFile } from "@tauri-apps/plugin-dialog";
import {
  ArchiveRestore,
  Cable,
  DatabaseBackup,
  Download,
  FileInput,
  Home,
  Keyboard,
  LayoutGrid,
  Plus,
  Trash2,
  Upload,
  Usb,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import brandIcon from "../src-tauri/icons/128x128.png";
import { ActionEditor } from "./ActionEditor";
import { ConfirmDialog } from "./ConfirmDialog";
import { CreateDeviceProfileForm } from "./CreateDeviceProfileForm";
import { DeviceManagement } from "./DeviceManagement";
import { DeviceSetupWizard } from "./DeviceSetupWizard";
import { HardwareMapping, hardwareProfilesAreValid } from "./HardwareMapping";
import { HomeDashboard } from "./HomeDashboard";
import { Keypad } from "./Keypad";
import { LayoutEditor } from "./LayoutEditor";
import { deviceSummary } from "./deviceStatus";
import { reconcileSetupSession, setupPresence } from "./deviceSetupSession";
import { t } from "./i18n";
import { DEFAULT_DOUBLE_PRESS_MS, DEFAULT_LONG_PRESS_MS } from "./types";
import type {
  AppSnapshot,
  BackupPreview,
  ButtonAction,
  CreateDeviceProfileRequest,
  DeviceProfile,
  HardwareProfile,
  HomeMetricsSnapshot,
  ImportPreview,
  InputSource,
  Language,
  LearningTarget,
  PhysicalInput,
  RuntimeAssignment,
  RuntimeEvent,
  TriggerActions,
} from "./types";
import { SerializedSaveQueue, useAutosave } from "./useAutosave";

type View = "home" | "devices" | "behavior" | "hardware" | "layout" | "data";
type Confirmation =
  | { kind: "import"; path: string; preview: ImportPreview }
  | { kind: "restore"; path: string; preview: BackupPreview }
  | { kind: "delete"; profile: DeviceProfile };

type RegistryState = Pick<
  AppSnapshot,
  "deviceProfiles" | "editorProfile" | "boardProfiles" | "devices" | "candidates"
>;
type PressedOwner = {
  deviceProfileId: string;
  hardwareProfileId: string;
  buttonIds: Set<string>;
};
type HardwareEditorTarget = {
  deviceProfileId: string;
  hardwareProfileId: string;
  deviceId: string | null;
};
type ProfileAutosaveTarget = {
  profileId: string;
  profile: DeviceProfile;
} | null;

const PREVIEW_MODE = import.meta.env.DEV && new URLSearchParams(window.location.search).has("preview");
const REGISTRY_REFRESH_MS = 1_500;

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error && "code" in error) return String(error.code);
  return String(error);
}

function allButtons(profile: DeviceProfile | undefined) {
  return profile?.profile.groups.flatMap((group) => group.buttons) ?? [];
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
    }))) && hardwareProfilesAreValid(profile.hardware_profiles, boardProfiles));
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
    if (Object.keys(source.keys).length === 0) continue;
    if (source.type === "direct" && input.type === "direct") {
      const match = Object.entries(source.keys).find(([, gpio]) => gpio === input.gpio);
      if (match) return match[0];
    }
    if (source.type === "contact_matrix" && input.type === "contact" && input.source === runtimeSource) {
      const pair = [Math.min(input.pin_a, input.pin_b), Math.max(input.pin_a, input.pin_b)];
      const match = Object.entries(source.keys).find(([, pins]) => pins[0] === pair[0] && pins[1] === pair[1]);
      if (match) return match[0];
    }
    runtimeSource += 1;
  }
  return null;
}

function learnInput(profile: DeviceProfile, hardwareProfileId: string, buttonId: string, input: PhysicalInput): DeviceProfile {
  const hardware = profile.hardware_profiles.find((item) => item.id === hardwareProfileId);
  if (!hardware) return profile;
  const inputs = hardware.inputs.map((source): InputSource => ({
    ...source,
    keys: Object.fromEntries(Object.entries(source.keys).filter(([id]) => id !== buttonId)),
  })) as InputSource[];

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

function learningTargetsMatch(left: LearningTarget, right: LearningTarget) {
  return left.deviceId === right.deviceId &&
    left.deviceProfileId === right.deviceProfileId &&
    left.hardwareProfileId === right.hardwareProfileId &&
    left.editingRevision === right.editingRevision &&
    left.firmwareRevision === right.firmwareRevision &&
    left.pins.length === right.pins.length &&
    left.pins.every((pin, index) => pin === right.pins[index]);
}

export default function App() {
  const queue = useRef(new SerializedSaveQueue()).current;
  const [registry, setRegistry] = useState<RegistryState>({
    deviceProfiles: [],
    editorProfile: null,
    boardProfiles: [],
    devices: [],
    candidates: [],
  });
  const [savedDeviceProfiles, setSavedDeviceProfiles] = useState<Record<string, string>>({});
  const [language, setLanguage] = useState<Language>("zh-CN");
  const [view, setView] = useState<View>("home");
  const [homeMetrics, setHomeMetrics] = useState<AppSnapshot["homeMetrics"]>(null);
  const [deviceMetrics, setDeviceMetrics] = useState<{ deviceId: string; snapshot: HomeMetricsSnapshot } | null>(null);
  const [selectedButtonId, setSelectedButtonId] = useState<string | null>(null);
  const [hardwareEditorTarget, setHardwareEditorTarget] = useState<HardwareEditorTarget | null>(null);
  const [capturedDraftProfileIds, setCapturedDraftProfileIds] = useState<Set<string>>(() => new Set());
  const [manualSaveProfileIds, setManualSaveProfileIds] = useState<Set<string>>(() => new Set());
  const [tentativeLearningCounts, setTentativeLearningCounts] = useState<Map<string, number>>(() => new Map());
  const [pressedButtonIds, setPressedButtonIds] = useState<Set<string>>(() => new Set());
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [layoutEditorOpen, setLayoutEditorOpen] = useState(false);
  const [setupOpen, setSetupOpen] = useState(false);
  const [setupTargetId, setSetupTargetId] = useState<string | null>(null);
  const [profileCreatorOpen, setProfileCreatorOpen] = useState(false);
  const pressedOwnersRef = useRef<Map<string, PressedOwner>>(new Map());
  const mountedRef = useRef(true);
  const registryEpochRef = useRef(0);
  const refreshInFlightRef = useRef(false);
  const refreshPendingRef = useRef(false);
  const refreshFullSnapshotPendingRef = useRef(false);
  const fullSnapshotRequiredRef = useRef(true);
  const refreshPromiseRef = useRef<Promise<void> | null>(null);
  const loadErrorMessageRef = useRef<string | null>(null);
  const selectedManagedDeviceIdRef = useRef<string | null>(null);
  const managedMetricsGenerationRef = useRef(0);
  const hardwareEditorTargetRef = useRef<HardwareEditorTarget | null>(null);
  const learningEditingRevisionRef = useRef(0);
  const profileDraftsRef = useRef<Map<string, DeviceProfile>>(new Map());
  const setupSeenRef = useRef<Set<string>>(new Set());

  const { deviceProfiles, editorProfile, boardProfiles, devices, candidates } = registry;
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
  const dirty = Boolean(editorProfileConfig &&
    savedDeviceProfiles[editorProfileConfig.profile.id] !== JSON.stringify(editorProfileConfig));
  const summary = deviceSummary(devices);
  const attentionCount = summary.attention + candidates.length;
  const currentSetupPresence = useMemo(
    () => setupPresence(devices, candidates),
    [devices, candidates],
  );

  useEffect(() => {
    if (!loaded) return;
    const next = reconcileSetupSession(
      setupSeenRef.current,
      currentSetupPresence,
    );
    setupSeenRef.current = next.seen;
    if (!setupOpen && next.autoTargetId) {
      setSetupTargetId(next.autoTargetId);
      setSetupOpen(true);
    }
  }, [currentSetupPresence, loaded, setupOpen]);

  const openSetup = useCallback((targetId: string | null = null) => {
    if (targetId) setupSeenRef.current.add(targetId);
    setSetupTargetId(targetId);
    setSetupOpen(true);
  }, []);

  const replaceRegistrySnapshot = useCallback((snapshot: AppSnapshot) => {
    registryEpochRef.current += 1;
    setRegistry((current) => ({
      ...current,
      boardProfiles: snapshot.boardProfiles,
      devices: snapshot.devices,
      candidates: snapshot.candidates,
    }));
    const currentDevices = new Map(snapshot.devices.map((device) => [device.deviceId, device]));
    const nextOwners = new Map(pressedOwnersRef.current);
    for (const [deviceId, owner] of nextOwners) {
      const currentDevice = currentDevices.get(deviceId);
      if (currentDevice?.connection !== "online" ||
        currentDevice.runtimeAssignment?.device_profile_id !== owner.deviceProfileId ||
        currentDevice.runtimeAssignment.hardware_profile_id !== owner.hardwareProfileId) {
        nextOwners.delete(deviceId);
      }
    }
    pressedOwnersRef.current = nextOwners;
    setPressedButtonIds(pressedButtons(nextOwners));
  }, []);

  const applySnapshot = useCallback((snapshot: AppSnapshot, preserveDrafts = false) => {
    registryEpochRef.current += 1;
    const serverProfiles = snapshot.deviceProfiles.map(normalizeDeviceProfile);
    const snapshotProfileIds = new Set(serverProfiles.map((profile) => profile.profile.id));
    if (preserveDrafts) {
      for (const profileId of profileDraftsRef.current.keys()) {
        if (!snapshotProfileIds.has(profileId)) profileDraftsRef.current.delete(profileId);
      }
      setCapturedDraftProfileIds((current) => {
        const next = new Set([...current].filter((profileId) => snapshotProfileIds.has(profileId)));
        return next.size === current.size ? current : next;
      });
      setManualSaveProfileIds((current) => {
        const next = new Set([...current].filter((profileId) => snapshotProfileIds.has(profileId)));
        return next.size === current.size ? current : next;
      });
    } else {
      profileDraftsRef.current.clear();
      setCapturedDraftProfileIds(new Set());
      setManualSaveProfileIds(new Set());
    }
    setRegistry({
      deviceProfiles: serverProfiles.map((profile) =>
        preserveDrafts ? profileDraftsRef.current.get(profile.profile.id) ?? profile : profile
      ),
      editorProfile: snapshot.editorProfile,
      boardProfiles: snapshot.boardProfiles,
      devices: snapshot.devices,
      candidates: snapshot.candidates,
    });
    setSavedDeviceProfiles(Object.fromEntries(serverProfiles.map((profile) =>
      [profile.profile.id, JSON.stringify(profile)]
    )));
    setLanguage(snapshot.language);
    setHomeMetrics(snapshot.homeMetrics);
    pressedOwnersRef.current = new Map();
    setPressedButtonIds(new Set());
  }, []);

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

  const saveEditorProfile = useCallback(async (profile: DeviceProfile | undefined) => {
    if (!profile) return;
    if (!PREVIEW_MODE) {
      const snapshot = await invoke<AppSnapshot>("save_device_profile", { profile });
      if (mountedRef.current) replaceRegistrySnapshot(snapshot);
    }
    profileDraftsRef.current.delete(profile.profile.id);
    setManualSaveProfileIds((current) => {
      if (!current.has(profile.profile.id)) return current;
      const next = new Set(current);
      next.delete(profile.profile.id);
      return next;
    });
    setCapturedDraftProfileIds((current) => {
      if (!current.has(profile.profile.id)) return current;
      const next = new Set(current);
      next.delete(profile.profile.id);
      return next;
    });
    setSavedDeviceProfiles((current) => ({
      ...current,
      [profile.profile.id]: JSON.stringify(profile),
    }));
  }, [replaceRegistrySnapshot]);

  const saveProfile = saveEditorProfile;

  const renameManagedDevice = useCallback(async (deviceId: string, name: string) => {
    try {
      const snapshot = await invoke<AppSnapshot>("rename_device", { deviceId, name });
      if (mountedRef.current) replaceRegistrySnapshot(snapshot);
    } catch (operationError) {
      setError(`${t(language, "error.save")}: ${errorMessage(operationError)}`);
      throw operationError;
    }
  }, [language, replaceRegistrySnapshot]);

  const forgetManagedDevice = useCallback(async (deviceId: string) => {
    try {
      const snapshot = await invoke<AppSnapshot>("forget_device", { deviceId });
      if (mountedRef.current) replaceRegistrySnapshot(snapshot);
    } catch (operationError) {
      setError(`${t(language, "error.delete")}: ${errorMessage(operationError)}`);
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

  const clearManagedRuntimeAssignment = useCallback(async (deviceId: string) => {
    try {
      const snapshot = await invoke<AppSnapshot>("clear_runtime_assignment", {
        deviceId,
      });
      if (mountedRef.current) replaceRegistrySnapshot(snapshot);
    } catch (operationError) {
      setError(`${t(language, "error.save")}: ${errorMessage(operationError)}`);
      throw operationError;
    }
  }, [language, replaceRegistrySnapshot]);

  const refreshManagedDeviceMetrics = useCallback(async (deviceId: string | null) => {
    selectedManagedDeviceIdRef.current = deviceId;
    const generation = ++managedMetricsGenerationRef.current;
    if (!deviceId) {
      setDeviceMetrics(null);
      return;
    }
    setDeviceMetrics(null);
    try {
      const metrics = await invoke<AppSnapshot["homeMetrics"]>("get_device_metrics", { deviceId });
      if (mountedRef.current && selectedManagedDeviceIdRef.current === deviceId && generation === managedMetricsGenerationRef.current) {
        setDeviceMetrics(metrics && "logs" in metrics ? { deviceId, snapshot: metrics } : null);
      }
    } catch {
      if (mountedRef.current && selectedManagedDeviceIdRef.current === deviceId && generation === managedMetricsGenerationRef.current) setDeviceMetrics(null);
    }
  }, []);

  const handleManagedMetricsChange = useCallback((deviceId: string | null) => {
    void refreshManagedDeviceMetrics(deviceId);
  }, [refreshManagedDeviceMetrics]);

  const handleHardwareEditorSelection = useCallback((
    hardwareProfileId: string | null,
    deviceId: string | null,
  ) => {
    const deviceProfileId = editorProfileConfig?.profile.id;
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
  }, [editorProfileConfig?.profile.id]);

  const autosaveTarget = useMemo<ProfileAutosaveTarget>(() => editorProfileConfig
    ? { profileId: editorProfileConfig.profile.id, profile: editorProfileConfig }
    : null, [editorProfileConfig]);
  const autosave = useAutosave<ProfileAutosaveTarget>({
    value: autosaveTarget,
    valid: dirty && !capturedDraftProfileIds.has(editorProfileConfig?.profile.id ?? "") &&
      !manualSaveProfileIds.has(editorProfileConfig?.profile.id ?? "") &&
      !tentativeLearningCounts.has(editorProfileConfig?.profile.id ?? "") &&
      !editorLearningActive &&
      isValidDraft(editorProfileConfig, boardProfiles),
    save: (target) => saveProfile(target?.profile),
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

  const profileRef = useRef(editorProfileConfig);
  const devicesRef = useRef(devices);
  const selectedRef = useRef(selectedButtonId);
  const viewRef = useRef(view);
  profileRef.current = editorProfileConfig;
  devicesRef.current = devices;
  hardwareEditorTargetRef.current = hardwareEditorTarget;
  selectedRef.current = selectedButtonId;
  viewRef.current = view;

  useEffect(() => {
    mountedRef.current = true;
    let active = true;
    let unlisten: (() => void) | undefined;
    const refreshTimer = PREVIEW_MODE ? undefined : setInterval(() => {
      void refreshRegistry();
    }, REGISTRY_REFRESH_MS);
    void (async () => {
      try {
        if (PREVIEW_MODE) {
          applySnapshot((await import("./preview")).previewSnapshot);
          return;
        }
        const registeredUnlisten = await listen<RuntimeEvent>("runtime-event", ({ payload }) => {
          if (!active) return;
          if (payload.homeUpdate && payload.deviceProfileId === profileRef.current?.profile.id) {
            setHomeMetrics(payload.homeUpdate);
          }
          if (payload.deviceId === selectedManagedDeviceIdRef.current) void refreshManagedDeviceMetrics(payload.deviceId);
          if (payload.input && payload.pressed !== null) {
            const currentProfile = profileRef.current;
            const currentEditorTarget = hardwareEditorTargetRef.current;
            const activeLearningTarget = devicesRef.current.find(
              ({ deviceId }) => deviceId === currentEditorTarget?.deviceId,
            )?.learning;
            if (payload.code === "learning_input" && payload.pressed && payload.learningTarget
              && selectedRef.current && viewRef.current === "hardware" && currentProfile && currentEditorTarget?.deviceId
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
            const assignment = emittingDevice?.runtimeAssignment;
            if (payload.code === "input_state" && currentProfile
              && payload.deviceProfileId === currentProfile.profile.id
              && payload.hardwareProfileId
              && assignment?.device_profile_id === payload.deviceProfileId
              && assignment.hardware_profile_id === payload.hardwareProfileId) {
              const eventHardware = currentProfile.hardware_profiles.find((hardware) =>
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
              }
            }
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
      unlisten?.();
    };
  }, [applySnapshot, refreshManagedDeviceMetrics, refreshRegistry]);

  useEffect(() => {
    const buttons = allButtons(editorProfileConfig);
    if (!buttons.some((button) => button.id === selectedButtonId)) {
      setSelectedButtonId(buttons[0]?.id ?? null);
    }
  }, [editorProfileConfig, selectedButtonId]);

  const updateProfile = useCallback((profileId: string, update: (profile: DeviceProfile) => DeviceProfile) => {
    if (!editorLearningActive || profileId !== editorProfileConfig?.profile.id) {
      setCapturedDraftProfileIds((current) => {
        if (!current.has(profileId)) return current;
        const next = new Set(current);
        next.delete(profileId);
        return next;
      });
    }
    setRegistry((current) => {
      const currentProfile = current.deviceProfiles.find((profile) => profile.profile.id === profileId);
      if (!currentProfile) return current;
      const updatedProfile = update(currentProfile);
      profileDraftsRef.current.set(profileId, updatedProfile);
      return {
        ...current,
        deviceProfiles: current.deviceProfiles.map((profile) =>
          profile.profile.id === profileId ? updatedProfile : profile
        ),
      };
    });
  }, [editorLearningActive, editorProfileConfig?.profile.id]);

  const updateEditorProfile = useCallback((update: (profile: DeviceProfile) => DeviceProfile) => {
    const profileId = editorProfileConfig?.profile.id;
    if (profileId) updateProfile(profileId, update);
  }, [editorProfileConfig?.profile.id, updateProfile]);

  const setProfileManualSave = useCallback((profileId: string, enabled: boolean) => {
    setManualSaveProfileIds((current) => {
      const next = new Set(current);
      if (enabled) next.add(profileId);
      else next.delete(profileId);
      return next;
    });
  }, []);

  const saveSettings = async (nextEditorProfile: string | null, nextLanguage: Language) => {
    await autosave.flush();
    if (PREVIEW_MODE) {
      setRegistry((current) => ({ ...current, editorProfile: nextEditorProfile }));
      setLanguage(nextLanguage);
      return;
    }
    const snapshot = await queue.enqueue(() => invoke<AppSnapshot>("save_settings", {
      settings: { schema_version: 2, editor_profile: nextEditorProfile, language: nextLanguage },
    }));
    applySnapshot(snapshot, true);
  };

  const completeDeviceSetup = async (
    deviceId: string,
    name: string,
    assignment: RuntimeAssignment,
  ) => {
    await autosave.flush();
    const completed = await invoke<AppSnapshot>("complete_device_setup", {
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
    setHardwareEditorTarget({
      deviceId,
      deviceProfileId: assignment.device_profile_id,
      hardwareProfileId: assignment.hardware_profile_id,
    });
    setSetupOpen(false);
    setView("hardware");
  };

  const run = async (label: string, task: () => Promise<void>) => {
    setError(null);
    try {
      await task();
    } catch (operationError) {
      setError(`${label}: ${errorMessage(operationError)}`);
    }
  };

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

  const confirmOperation = () => {
    const current = confirmation;
    if (!current) return;
    setConfirmation(null);
    void run(
      t(language, current.kind === "restore" ? "error.restore" : current.kind === "delete" ? "error.delete" : "error.import"),
      async () => {
        const snapshot = current.kind === "import"
          ? await invoke<AppSnapshot>("import_device_profile", { path: current.path })
          : current.kind === "restore"
            ? await invoke<AppSnapshot>("restore_backup", { path: current.path })
            : await invoke<AppSnapshot>("delete_device_profile", { id: current.profile.profile.id });
        applySnapshot(snapshot);
      },
    );
  };

  const selectedButton = allButtons(editorProfileConfig).find((button) => button.id === selectedButtonId) ?? null;
  const selectedActions = editorProfileConfig && selectedButtonId
    ? editorProfileConfig.actions[selectedButtonId]?.press ?? []
    : [];

  return (
    <main className="product-shell">
      <header className="topbar">
        <div className="brand"><img src={brandIcon} alt="" /><h1>Kivo</h1></div>
        <div className="device-summary" aria-label={t(language, "device.summary")}>
          <span className="summary-ready"><i />{summary.ready} {t(language, "device.ready")}</span>
          <b aria-hidden="true">{" · "}</b>
          <span className="summary-attention"><i />{attentionCount} {t(language, "device.attention")}</span>
          <b aria-hidden="true">{" · "}</b>
          <span className="summary-offline"><i />{summary.offline} {t(language, "device.offline")}</span>
        </div>
        <div className={`save-state is-${autosave.status}`} aria-live="polite">
          {autosave.status === "saving" && t(language, "save.saving")}
          {autosave.status === "error" && (
            <><span>{t(language, "save.failed")}</span><button type="button" onClick={() => void autosave.retry()}>{t(language, "save.retry")}</button></>
          )}
        </div>
      </header>

      {error && (
        <div className="error-toast" role="alert">
          <span className="error-banner">{error}</span>
          <button className="icon-button" type="button" aria-label={t(language, "common.close")} onClick={() => {
            setError(null);
          }}><X size={15} /></button>
        </div>
      )}

      <div className={view === "home" || view === "devices" || view === "data" ? "product-workspace is-home" : "product-workspace"}>
        <aside className="sidebar">
          <button className={`home-nav-button ${view === "home" ? "is-active" : ""}`} type="button" onClick={() => void navigate("home")}>
            <Home size={17} />{t(language, "nav.home")}
          </button>

          <button className={`devices-nav-button ${view === "devices" ? "is-active" : ""}`} type="button" onClick={() => void navigate("devices")}>
            <Usb size={17} />{t(language, "nav.devices")}
          </button>

          <nav aria-label={t(language, "nav.configuration")}>
            <span>{t(language, "nav.configuration")}</span>
            <button className={view === "behavior" ? "is-active" : ""} type="button" onClick={() => void navigate("behavior")}>
              <Keyboard size={17} />{t(language, "nav.behavior")}
            </button>
            <button className={view === "hardware" ? "is-active" : ""} type="button" disabled={!editorProfileConfig} onClick={() => void navigate("hardware")}>
              <Cable size={17} />{t(language, "nav.hardware")}
            </button>
            <button className={view === "layout" ? "is-active" : ""} type="button" disabled={!editorProfileConfig} onClick={() => void navigate("layout")}>
              <LayoutGrid size={17} />{t(language, "nav.layout")}
            </button>
          </nav>

          <button className={`data-nav-button ${view === "data" ? "is-active" : ""}`} type="button" onClick={() => void navigate("data")}>
            <FileInput size={17} />{t(language, "nav.data")}
          </button>

        </aside>

        <section className="content-panel">
          {view === "data" ? (
            <div className="data-page">
              <div className="content-heading">
                <div><h2>{t(language, "nav.data")}</h2></div>
                <button className="primary-button" type="button" onClick={() => setProfileCreatorOpen(true)}>
                  <Plus size={16} />{t(language, "profile.create")}
                </button>
              </div>
              <div className="data-page-body">
                <section className="data-card">
                  <h3>{t(language, "data.groupModel")}</h3>
                  <label className="model-picker"><span>{t(language, "model.select")}</span><select
                    aria-label={t(language, "model.select")}
                    value={editorProfile ?? ""}
                    disabled={!loaded || deviceProfiles.length === 0}
                    onChange={(event) => void run(t(language, "error.save"), () => saveSettings(event.target.value, language))}
                  >
                    {deviceProfiles.length === 0 && <option value="">{t(language, "model.empty")}</option>}
                    {deviceProfiles.map((profile) => <option value={profile.profile.id} key={profile.profile.id}>{profile.profile.name}</option>)}
                  </select></label>
                </section>
                <section className="data-card">
                  <h3>{t(language, "data.groupTransfer")}</h3>
                  <div className="data-menu">
                    <button type="button" onClick={() => void chooseImport()}><FileInput size={16} />{t(language, "nav.import")}</button>
                    <button type="button" disabled={!editorProfileConfig} onClick={() => void run(t(language, "error.export"), async () => {
                      await autosave.flush();
                      const path = await saveFile({ defaultPath: `${editorProfileConfig?.profile.id ?? "device-profile"}.yaml`, filters: [{ name: "Kivo", extensions: ["yaml"] }] });
                      if (path && editorProfileConfig) await invoke("export_device_profile", { id: editorProfileConfig.profile.id, path });
                    })}><Upload size={16} />{t(language, "nav.export")}</button>
                    <button type="button" disabled={deviceProfiles.length === 0} onClick={() => void run(t(language, "error.export"), async () => {
                      await autosave.flush();
                      const path = await saveFile({ defaultPath: "kivo-backup.yaml", filters: [{ name: "Kivo", extensions: ["yaml"] }] });
                      if (path) await invoke("export_backup", { path });
                    })}><DatabaseBackup size={16} />{t(language, "nav.backup")}</button>
                    <button type="button" onClick={() => void chooseRestore()}><ArchiveRestore size={16} />{t(language, "nav.restore")}</button>
                  </div>
                </section>
                <section className="data-card is-danger">
                  <h3>{t(language, "data.groupDanger")}</h3>
                  <div className="data-menu">
                    <button className="is-danger" type="button" disabled={!editorProfileConfig} onClick={() => editorProfileConfig && setConfirmation({ kind: "delete", profile: editorProfileConfig })}>
                      <Trash2 size={16} />{t(language, "nav.delete")}
                    </button>
                  </div>
                </section>
              </div>
            </div>
          ) : view === "devices" ? (
            <DeviceManagement
              language={language}
              devices={devices}
              candidates={candidates}
              boardProfiles={boardProfiles}
              deviceProfiles={deviceProfiles}
              metrics={deviceMetrics}
              onRename={renameManagedDevice}
              onForget={forgetManagedDevice}
              onSaveRuntimeAssignment={saveManagedRuntimeAssignment}
              onClearRuntimeAssignment={clearManagedRuntimeAssignment}
              onMetricsChange={handleManagedMetricsChange}
              onOpenSetup={openSetup}
              onRetryCandidate={retrySetupCandidate}
            />
          ) : view === "home" ? (
            <HomeDashboard
              devices={devices}
              language={language}
              metrics={homeMetrics}
              profile={editorProfileConfig}
            />
          ) : !editorProfileConfig ? (
            <div className="empty-workspace">
              <Download size={28} />
              <h2>{t(language, "model.empty")}</h2>
              <div><button className="primary-button" type="button" onClick={() => void chooseImport()}>{t(language, "model.import")}</button><button type="button" onClick={() => void chooseRestore()}>{t(language, "model.restore")}</button></div>
            </div>
          ) : view === "hardware" ? (
            <HardwareMapping
              key={editorProfileConfig.profile.id}
              language={language}
              layout={editorProfileConfig.profile}
              hardwareProfiles={editorProfileConfig.hardware_profiles}
              boardProfiles={boardProfiles}
              devices={devices}
              learning={selectedLearningDevice?.learning ?? null}
              initialHardwareProfileId={
                hardwareEditorTarget?.deviceProfileId === editorProfile
                  ? hardwareEditorTarget.hardwareProfileId
                  : undefined
              }
              initialDeviceId={
                hardwareEditorTarget?.deviceProfileId === editorProfile
                  ? hardwareEditorTarget.deviceId
                  : undefined
              }
              selectedButtonId={selectedButtonId}
              onSelectButton={setSelectedButtonId}
              onChange={(hardwareProfiles) => updateEditorProfile((profile) => ({
                ...profile,
                hardware_profiles: hardwareProfiles,
              }))}
              onSelectionChange={handleHardwareEditorSelection}
              onBeginLearning={(hardwareProfileId, deviceId, pins) => {
                const hardware = editorProfileConfig.hardware_profiles.find(({ id }) => id === hardwareProfileId);
                const selectedDevice = devices.find((device) =>
                  device.deviceId === deviceId &&
                  device.connection === "online" &&
                  device.mode === "runtime" &&
                  device.identity === "valid" &&
                  device.boardProfileId === hardware?.board_profile_id
                );
                if (!hardware || !selectedDevice) return;
                const learningProfileId = editorProfileConfig.profile.id;
                const editingRevision = ++learningEditingRevisionRef.current;
                setTentativeLearningCounts((current) => {
                  const next = new Map(current);
                  next.set(learningProfileId, (next.get(learningProfileId) ?? 0) + 1);
                  return next;
                });
                void run(t(language, "error.learning"), async () => {
                  try {
                    replaceRegistrySnapshot(await invoke("begin_learning", {
                      deviceId: selectedDevice.deviceId,
                      deviceProfileId: learningProfileId,
                      hardwareProfileId: hardware.id,
                      editingRevision,
                      pins,
                    }));
                  } finally {
                    setTentativeLearningCounts((current) => {
                      const next = new Map(current);
                      const remaining = (next.get(learningProfileId) ?? 1) - 1;
                      if (remaining > 0) next.set(learningProfileId, remaining);
                      else next.delete(learningProfileId);
                      return next;
                    });
                  }
                });
              }}
              onEndLearning={(deviceId) => {
                void run(t(language, "error.learning"), async () => replaceRegistrySnapshot(await invoke("end_learning", {
                  deviceId,
                })));
              }}
            />
          ) : (
            <>
              <div className="content-heading">
                <div>
                  {view === "behavior" && <label className="model-picker">
                    <span>{t(language, "model.select")}</span>
                    <select
                      aria-label={t(language, "model.select")}
                      value={editorProfile ?? ""}
                      disabled={!loaded || deviceProfiles.length === 0}
                      onChange={(event) => void run(t(language, "error.save"), () => saveSettings(event.target.value, language))}
                    >
                      {deviceProfiles.map((profile) => (
                        <option value={profile.profile.id} key={profile.profile.id}>{profile.profile.name}</option>
                      ))}
                    </select>
                  </label>}
                  <span>{editorProfileConfig.profile.name}</span>
                  <h2>{t(language, view === "layout" ? "layout.title" : "behavior.title")}</h2>
                </div>
                {view === "behavior" && selectedButton && <span className="selected-crumb">{t(language, "behavior.selected", { label: selectedButton.label })}</span>}
                {view === "layout" && <button className="primary-button" type="button" onClick={() => setLayoutEditorOpen(true)}><LayoutGrid size={16} />{t(language, "layout.edit")}</button>}
              </div>
              <div className="keypad-stage">
                <Keypad
                  layout={editorProfileConfig.profile}
                  actions={editorProfileConfig.actions}
                  selectedButtonId={selectedButtonId}
                  pressedButtonIds={pressedButtonIds}
                  actionCountLabel={(count) => t(language, "model.actionCount", { count })}
                  onSelect={setSelectedButtonId}
                />
              </div>
            </>
          )}
        </section>

        {view !== "home" && view !== "devices" && view !== "data" && <ActionEditor
          language={language}
          button={selectedButton}
          actions={selectedActions}
          onChange={(actions: ButtonAction[]) => selectedButtonId && updateEditorProfile((profile) => ({
            ...profile,
            actions: {
              ...profile.actions,
              [selectedButtonId]: {
                ...(profile.actions[selectedButtonId] ?? emptyTriggerActions()),
                press: actions,
              },
            },
          }))}
        />}
      </div>

      <LayoutEditor
        layout={editorProfileConfig?.profile ?? null}
        language={language}
        open={layoutEditorOpen}
        onCancel={() => setLayoutEditorOpen(false)}
        onApply={(layout) => {
          updateEditorProfile((profile) => {
            const buttonIds = new Set(layout.groups.flatMap((group) => group.buttons.map((button) => button.id)));
            const actions = Object.fromEntries(Object.entries(profile.actions).filter(([id]) => buttonIds.has(id)));
            const hardwareProfiles = profile.hardware_profiles.map((hardware) => ({
              ...hardware,
              inputs: hardware.inputs.map((source) => ({
                ...source,
                keys: Object.fromEntries(Object.entries(source.keys).filter(([id]) => buttonIds.has(id))),
              })) as InputSource[],
            }));
            return { ...profile, profile: layout, actions, hardware_profiles: hardwareProfiles };
          });
          setLayoutEditorOpen(false);
        }}
      />

      {profileCreatorOpen && (
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
                onClick={() => setProfileCreatorOpen(false)}
              >
                <X size={17} />
              </button>
            </header>
            <div className="device-setup-body">
              <CreateDeviceProfileForm
                language={language}
                boardProfiles={boardProfiles}
                deviceProfiles={deviceProfiles}
                onCreate={async (request) => {
                  await createDeviceProfile(request);
                  setProfileCreatorOpen(false);
                  setView("layout");
                }}
                onCancel={() => setProfileCreatorOpen(false)}
              />
            </div>
          </section>
        </div>
      )}

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
        onComplete={completeDeviceSetup}
        onClose={() => setSetupOpen(false)}
      />

      {confirmation && (
        <ConfirmDialog
          title={confirmation.kind === "restore"
            ? t(language, "dialog.restoreTitle")
            : confirmation.kind === "delete"
              ? t(language, "dialog.deleteTitle")
              : t(language, confirmation.preview.replacesExisting ? "dialog.replaceTitle" : "dialog.importTitle")}
          body={confirmation.kind === "restore"
            ? t(language, "dialog.restoreBody")
            : confirmation.kind === "delete"
              ? t(language, "dialog.deleteBody", { name: confirmation.profile.profile.name })
              : t(language, confirmation.preview.replacesExisting ? "dialog.replaceBody" : "dialog.importBody")}
          summary={confirmation.kind === "restore"
            ? t(language, "dialog.backupSummary", {
              models: confirmation.preview.profileCount,
              buttons: confirmation.preview.buttonCount,
              bindings: confirmation.preview.hardwareBindingCount,
              actions: confirmation.preview.actionCount,
              devices: confirmation.preview.deviceCount,
              assignments: confirmation.preview.assignmentCount,
              metricRows: confirmation.preview.metricRowCount,
              activity: confirmation.preview.activityCount,
            })
            : confirmation.kind === "import"
              ? t(language, "dialog.modelSummary", {
                buttons: confirmation.preview.buttonCount,
                bindings: confirmation.preview.hardwareBindingCount,
                actions: confirmation.preview.actionCount,
              })
              : confirmation.profile.profile.name}
          confirmLabel={t(language, "common.confirm")}
          cancelLabel={t(language, "common.cancel")}
          danger={confirmation.kind !== "import" || confirmation.preview.replacesExisting}
          onCancel={() => setConfirmation(null)}
          onConfirm={confirmOperation}
        />
      )}
    </main>
  );
}
