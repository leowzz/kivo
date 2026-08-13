import { RefreshCw, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { CreateDeviceProfileForm } from "./CreateDeviceProfileForm";
import { candidateDisplayLabel, serialSuffix } from "./deviceStatus";
import { t, type MessageKey } from "./i18n";
import { resolveButton } from "./inputMapping";
import { Keypad } from "./Keypad";
import type {
  AppSnapshot,
  BoardProfileSummary,
  CandidateIssue,
  CandidateStatus,
  CreateDeviceProfileRequest,
  DeviceProfile,
  DeviceStatus,
  Language,
  RuntimeAssignment,
  PhysicalInput,
} from "./types";

export interface SetupInputEvent {
  timestampMs: number;
  deviceId: string;
  input: PhysicalInput;
  pressed: boolean;
}

export interface DeviceSetupWizardProps {
  open: boolean;
  targetId: string | null;
  language: Language;
  devices: DeviceStatus[];
  candidates: CandidateStatus[];
  boardProfiles: BoardProfileSummary[];
  deviceProfiles: DeviceProfile[];
  inputEvent: SetupInputEvent | null;
  onTargetChange(targetId: string): void;
  onRetryCandidate(deviceId: string): Promise<void>;
  onCreateProfile(request: CreateDeviceProfileRequest): Promise<AppSnapshot>;
  onComplete(
    deviceId: string,
    name: string,
    assignment: RuntimeAssignment,
  ): Promise<void>;
  onClose(): void;
}

type SetupStep = "recognized" | "preset" | "test";

const issueMessages: Record<
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

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error && "code" in error) {
    return String(error.code);
  }
  return String(error);
}

function candidateTargetId(candidate: CandidateStatus) {
  return candidate.deviceId ?? `candidate:${candidate.key}`;
}

function setupDevices(devices: DeviceStatus[]) {
  return devices.filter(
    (device) =>
      device.connection === "online" &&
      device.mode === "runtime" &&
      device.identity === "valid" &&
      device.assignment === "unassigned",
  );
}

function compatibleProfiles(
  deviceProfiles: DeviceProfile[],
  boardProfileId: string,
) {
  return deviceProfiles.filter((profile) =>
    profile.hardware_profiles.some(
      (hardware) => hardware.board_profile_id === boardProfileId,
    ),
  );
}

export function DeviceSetupWizard({
  open,
  targetId,
  language,
  devices,
  candidates,
  boardProfiles,
  deviceProfiles,
  inputEvent,
  onTargetChange,
  onRetryCandidate,
  onCreateProfile,
  onComplete,
  onClose,
}: DeviceSetupWizardProps) {
  const [creatingProfile, setCreatingProfile] = useState(false);
  const [step, setStep] = useState<SetupStep>("recognized");
  const [testPressedButtonIds, setTestPressedButtonIds] = useState<Set<string>>(new Set());
  const [deviceProfileId, setDeviceProfileId] = useState("");
  const [hardwareProfileId, setHardwareProfileId] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const initializedDeviceId = useRef<string | null>(null);
  const eligibleDevices = useMemo(() => setupDevices(devices), [devices]);
  const selectedCandidate =
    candidates.find(
      (candidate) => candidateTargetId(candidate) === targetId,
    ) ?? null;
  const selectedDevice =
    eligibleDevices.find((device) => device.deviceId === targetId) ?? null;
  const targets = useMemo(() => {
    const values = new Map<string, string>();
    for (const [index, candidate] of candidates.entries()) {
      values.set(
        candidateTargetId(candidate),
        candidateDisplayLabel(candidate, index + 1, language),
      );
    }
    for (const device of eligibleDevices) {
      values.set(device.deviceId, device.name);
    }
    return [...values].map(([id, label]) => ({ id, label }));
  }, [candidates, eligibleDevices, language]);
  const compatible = useMemo(
    () =>
      selectedDevice
        ? compatibleProfiles(deviceProfiles, selectedDevice.boardProfileId)
        : [],
    [deviceProfiles, selectedDevice],
  );
  const selectedProfile =
    compatible.find((profile) => profile.profile.id === deviceProfileId) ?? null;
  const compatibleHardware =
    selectedProfile?.hardware_profiles.filter(
      (hardware) => hardware.board_profile_id === selectedDevice?.boardProfileId,
    ) ?? [];
  const boardName = (boardProfileId: string) =>
    boardProfiles.find((board) => board.id === boardProfileId)?.displayName ??
    boardProfileId;

  useEffect(() => {
    if (!selectedDevice) return;
    if (initializedDeviceId.current === selectedDevice.deviceId) return;
    initializedDeviceId.current = selectedDevice.deviceId;
    const firstProfile = compatible[0] ?? null;
    const hardware =
      firstProfile?.hardware_profiles.filter(
        (item) => item.board_profile_id === selectedDevice.boardProfileId,
      ) ?? [];
    setDeviceProfileId(firstProfile?.profile.id ?? "");
    setHardwareProfileId(hardware[0]?.id ?? "");
    setCreatingProfile(false);
    setStep("recognized");
    setTestPressedButtonIds(new Set());
    setError(null);
  }, [selectedDevice?.deviceId]);

  useEffect(() => {
    if (targetId && !selectedDevice) setTestPressedButtonIds(new Set());
  }, [selectedDevice, targetId]);

  useEffect(() => {
    if (!inputEvent || step !== "test" || inputEvent.deviceId !== selectedDevice?.deviceId) return;
    const hardware = compatibleHardware.find((item) => item.id === hardwareProfileId);
    const buttonId = resolveButton(hardware, inputEvent.input);
    if (!buttonId) return;
    setTestPressedButtonIds((current) => {
      const next = new Set(current);
      if (inputEvent.pressed) next.add(buttonId);
      else next.delete(buttonId);
      return next;
    });
  }, [compatibleHardware, hardwareProfileId, inputEvent, selectedDevice?.deviceId, step]);

  if (!open) return null;

  async function retryCandidate() {
    if (!selectedCandidate?.deviceId || pending) return;
    setPending(true);
    setError(null);
    try {
      await onRetryCandidate(selectedCandidate.deviceId);
    } catch (operationError) {
      setError(errorMessage(operationError));
    } finally {
      setPending(false);
    }
  }

  async function createProfile(request: CreateDeviceProfileRequest) {
    setPending(true);
    setError(null);
    try {
      const nextSnapshot = await onCreateProfile(request);
      if (selectedDevice && nextSnapshot.editorProfile) {
        const created = nextSnapshot.deviceProfiles.find(
          (profile) => profile.profile.id === nextSnapshot.editorProfile,
        );
        const hardware =
          created?.hardware_profiles.filter(
            (item) => item.board_profile_id === selectedDevice.boardProfileId,
          ) ?? [];
        setDeviceProfileId(created?.profile.id ?? "");
        setHardwareProfileId(hardware[0]?.id ?? "");
      }
      setCreatingProfile(false);
      setStep("preset");
    } catch (operationError) {
      setError(errorMessage(operationError));
    } finally {
      setPending(false);
    }
  }

  async function complete() {
    if (
      !selectedDevice ||
      !deviceProfileId ||
      !hardwareProfileId ||
      pending
    ) {
      return;
    }
    setPending(true);
    setError(null);
    try {
      await onComplete(selectedDevice.deviceId, selectedDevice.name, {
        device_profile_id: deviceProfileId,
        hardware_profile_id: hardwareProfileId,
      });
    } catch (operationError) {
      setError(errorMessage(operationError));
    } finally {
      setPending(false);
    }
  }

  const canRetry =
    selectedCandidate !== null &&
    selectedCandidate.deviceId !== null &&
    [
      "validating",
      "firmware_not_responding",
      "firmware_incompatible",
      "port_unavailable",
      "unknown",
    ].includes(selectedCandidate.issue);

  return (
    <div className="modal-backdrop" role="presentation">
      <section
        className="device-setup-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="device-setup-title"
      >
        <header className="device-setup-header">
          <h2 id="device-setup-title">{t(language, "setup.title")}</h2>
          <button
            className="icon-button"
            type="button"
            aria-label={t(language, "common.close")}
            title={t(language, "common.close")}
            disabled={pending}
            onClick={onClose}
          >
            <X size={17} />
          </button>
        </header>
        <div className="device-setup-body">
          {creatingProfile ? (
            <CreateDeviceProfileForm
              language={language}
              boardProfiles={boardProfiles}
              deviceProfiles={deviceProfiles}
              fixedBoardProfileId={
                selectedCandidate?.boardProfileId ?? selectedDevice?.boardProfileId
              }
              onCreate={createProfile}
              onCancel={() => setCreatingProfile(false)}
            />
          ) : targetId === null && targets.length > 0 ? (
            <section className="setup-targets">
              <h3>{t(language, "setup.selectTarget")}</h3>
              {targets.map((target) => (
                <button
                  type="button"
                  key={target.id}
                  onClick={() => onTargetChange(target.id)}
                >
                  {target.label}
                </button>
              ))}
            </section>
          ) : targetId === null ? (
            <section className="setup-empty">
              <h3>{t(language, "setup.waiting")}</h3>
              <button type="button" onClick={() => setCreatingProfile(true)}>
                {t(language, "profile.create")}
              </button>
            </section>
          ) : selectedCandidate ? (
            <section className="candidate-setup">
              <h3>{t(language, issueMessages[selectedCandidate.issue].title)}</h3>
              <p>{t(language, issueMessages[selectedCandidate.issue].body)}</p>
              <div className="candidate-actions">
                {canRetry ? (
                  <button
                    type="button"
                    disabled={pending}
                    onClick={() => void retryCandidate()}
                  >
                    <RefreshCw size={16} />
                    {t(language, "setup.retry")}
                  </button>
                ) : null}
                <button
                  type="button"
                  disabled={pending}
                  onClick={() => setCreatingProfile(true)}
                >
                  {t(language, "setup.createFirst")}
                </button>
                <button type="button" disabled={pending} onClick={onClose}>
                  {t(language, "setup.later")}
                </button>
              </div>
              <details className="device-technical-details">
                <summary>{t(language, "setup.technicalDetails")}</summary>
                <dl>
                  <dt>{t(language, "devices.serial")}</dt>
                  <dd>{selectedCandidate.rawSerial ?? "-"}</dd>
                  <dt>{t(language, "devices.id")}</dt>
                  <dd>{selectedCandidate.deviceId ?? "-"}</dd>
                  <dt>{t(language, "devices.board")}</dt>
                  <dd>{boardName(selectedCandidate.boardProfileId)}</dd>
                  <dt>{t(language, "devices.controller")}</dt>
                  <dd>{selectedCandidate.controllerFamilyId}</dd>
                  <dt>{t(language, "devices.mode")}</dt>
                  <dd>{selectedCandidate.mode}</dd>
                  <dt>{t(language, "setup.systemPort")}</dt>
                  <dd>{selectedCandidate.port ?? "-"}</dd>
                  <dt>{t(language, "devices.error")}</dt>
                  <dd>{selectedCandidate.latestError ?? "-"}</dd>
                </dl>
              </details>
            </section>
          ) : !selectedDevice ? (
            <section className="setup-empty">
              <h3>{t(language, "setup.disconnected")}</h3>
            </section>
          ) : step === "recognized" ? (
            <section className="setup-recognized">
              <p className="setup-step">{t(language, "setup.step", { current: 1, total: 3 })}</p>
              <h3>{t(language, "setup.recognizedTitle")}</h3>
              <dl>
                <dt>{t(language, "devices.name")}</dt>
                <dd>{selectedDevice.name}</dd>
                <dt>{t(language, "devices.board")}</dt>
                <dd>{boardName(selectedDevice.boardProfileId)}</dd>
                <dt>{t(language, "devices.serial")}</dt>
                <dd>{serialSuffix(selectedDevice.hardwareSerial)}</dd>
                <dt>{t(language, "setup.recommendedProfile")}</dt>
                <dd>{compatible[0]?.profile.name ?? t(language, "setup.noCompatibleProfile")}</dd>
              </dl>
              <div className="device-setup-actions">
                <button
                  className="primary-button"
                  type="button"
                  disabled={pending}
                  onClick={() => setStep("preset")}
                >
                  {t(language, "setup.continue")}
                </button>
              </div>
            </section>
          ) : step === "preset" ? (
            <section className="setup-profile-choice">
              <p className="setup-step">{t(language, "setup.step", { current: 2, total: 3 })}</p>
              <h3>{t(language, "setup.selectProfile")}</h3>
              <p>{boardName(selectedDevice.boardProfileId)}</p>
              <label>
                <span>{t(language, "setup.deviceProfile")}</span>
                <select
                  aria-label={t(language, "setup.deviceProfile")}
                  value={deviceProfileId}
                  disabled={pending}
                  onChange={(event) => {
                    const nextId = event.target.value;
                    const next = compatible.find(
                      (profile) => profile.profile.id === nextId,
                    );
                    const hardware =
                      next?.hardware_profiles.filter(
                        (item) =>
                          item.board_profile_id === selectedDevice.boardProfileId,
                      ) ?? [];
                    setDeviceProfileId(nextId);
                    setHardwareProfileId(hardware[0]?.id ?? "");
                  }}
                >
                  {compatible.map((profile) => (
                    <option key={profile.profile.id} value={profile.profile.id}>
                      {profile.profile.name}
                    </option>
                  ))}
                </select>
              </label>
              <div className="device-setup-actions">
                <button
                  type="button"
                  disabled={pending}
                  onClick={() => setCreatingProfile(true)}
                >
                  {t(language, "profile.create")}
                </button>
                <button
                  className="primary-button"
                  type="button"
                  disabled={!deviceProfileId || !hardwareProfileId || pending}
                  onClick={() => setStep("test")}
                >
                  {t(language, "setup.next")}
                </button>
              </div>
            </section>
          ) : (
            <section className="setup-test">
              <p className="setup-step">{t(language, "setup.step", { current: 3, total: 3 })}</p>
              <h3>{t(language, "setup.testTitle")}</h3>
              <p>{t(language, "setup.testBody")}</p>
              {selectedProfile ? (
                <Keypad
                  layout={selectedProfile.profile}
                  actions={selectedProfile.actions}
                  selectedButtonId={null}
                  pressedButtonIds={testPressedButtonIds}
                  actionCountLabel={() => ""}
                  unconfiguredLabel=""
                  onSelect={() => undefined}
                />
              ) : null}
              <div className="device-setup-actions">
                <button type="button" disabled={pending} onClick={() => void complete()}>{t(language, "setup.skipTest")}</button>
                <button className="primary-button" type="button" disabled={pending} onClick={() => void complete()}>{t(language, "setup.complete")}</button>
              </div>
            </section>
          )}
          {error ? (
            <p className="field-error" role="alert">
              {error}
            </p>
          ) : null}
        </div>
      </section>
    </div>
  );
}
