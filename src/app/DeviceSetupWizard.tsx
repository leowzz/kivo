import {
  Check,
  ChevronRight,
  FilePlus2,
  Keyboard,
  RefreshCw,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { CreateDeviceProfileForm } from "./CreateDeviceProfileForm";
import { candidateDisplayLabel, serialSuffix } from "./deviceStatus";
import { t, type MessageKey } from "./i18n";
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
} from "./types";

export interface DeviceSetupWizardProps {
  open: boolean;
  targetId: string | null;
  language: Language;
  devices: DeviceStatus[];
  candidates: CandidateStatus[];
  boardProfiles: BoardProfileSummary[];
  deviceProfiles: DeviceProfile[];
  onTargetChange(targetId: string): void;
  onRetryCandidate(deviceId: string): Promise<void>;
  onCreateProfile(request: CreateDeviceProfileRequest): Promise<AppSnapshot>;
  /**
   * Gives the host a chance to prepare a device-specific profile before assignment.
   * A null source means that setup should start from a blank profile.
   */
  onPrepareProfile?(
    deviceId: string,
    deviceName: string,
    sourceProfileId: string | null,
    preferredHardwareProfileId: string | null,
  ): Promise<RuntimeAssignment>;
  onComplete(
    deviceId: string,
    name: string,
    assignment: RuntimeAssignment,
  ): Promise<void>;
  /** Runtime feedback for the final physical-key check; omitted means waiting. */
  verification?: DeviceSetupVerification;
  /** Allows a future runtime integration to restart an expired physical-key check. */
  onVerificationRetry?(deviceId: string): void | Promise<void>;
  onClose(): void;
}

export type DeviceSetupVerification = {
  status: "waiting" | "success" | "timeout" | "error";
  buttonId?: string | null;
  buttonLabel?: string | null;
  detail?: string | null;
};

type SetupStep = "identify" | "template" | "verify";

const issueMessages: Record<
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
  onTargetChange,
  onRetryCandidate,
  onCreateProfile,
  onPrepareProfile,
  onComplete,
  verification,
  onVerificationRetry,
  onClose,
}: DeviceSetupWizardProps) {
  const [creatingProfile, setCreatingProfile] = useState(false);
  const [step, setStep] = useState<SetupStep>("identify");
  const [deviceProfileId, setDeviceProfileId] = useState("");
  const [hardwareProfileId, setHardwareProfileId] = useState("");
  const [name, setName] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [localVerification, setLocalVerification] =
    useState<DeviceSetupVerification>({ status: "waiting" });
  const initializedDeviceId = useRef<string | null>(null);
  const lastSelectedDevice = useRef<DeviceStatus | null>(null);
  const eligibleDevices = useMemo(() => setupDevices(devices), [devices]);
  const selectedDevice =
    eligibleDevices.find((device) => device.deviceId === targetId) ?? null;
  // A validated Device is authoritative when a stale Candidate for the same
  // identity is still present during the discovery transition.
  const selectedCandidate = selectedDevice
    ? null
    : candidates.find(
        (candidate) => candidateTargetId(candidate) === targetId,
      ) ?? null;
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
    lastSelectedDevice.current = selectedDevice;
    setName(selectedDevice.name);
    setDeviceProfileId(firstProfile?.profile.id ?? "");
    setHardwareProfileId(hardware[0]?.id ?? "");
    setCreatingProfile(false);
    setStep("identify");
    setLocalVerification({ status: "waiting" });
    setError(null);
  }, [selectedDevice?.deviceId]);

  if (!open) return null;

  const setupDevice =
    selectedDevice ?? (step === "verify" ? lastSelectedDevice.current : null);
  const verificationState = verification ?? localVerification;
  const selectedButtons = selectedProfile?.profile.groups.flatMap(
    (group) => group.buttons,
  ) ?? [];
  const verificationButton = verificationState.buttonLabel
    ? verificationState.buttonLabel
    : verificationState.buttonId
      ? selectedButtons.find((button) => button.id === verificationState.buttonId)
          ?.label ?? verificationState.buttonId
      : selectedButtons[0]?.label ?? t(language, "setup.anyKey");

  function selectProfile(nextId: string) {
    const next = compatible.find((profile) => profile.profile.id === nextId);
    const hardware =
      next?.hardware_profiles.filter(
        (item) => item.board_profile_id === selectedDevice?.boardProfileId,
      ) ?? [];
    setDeviceProfileId(nextId);
    setHardwareProfileId(hardware[0]?.id ?? "");
    setError(null);
  }

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
      const created =
        nextSnapshot.deviceProfiles.find(
          (profile) => profile.profile.id === nextSnapshot.editorProfile,
        ) ??
        nextSnapshot.deviceProfiles.find(
          (profile) => profile.profile.name === request.name,
        ) ??
        null;
      if (created && selectedDevice) {
        const hardware =
          created?.hardware_profiles.filter(
            (item) => item.board_profile_id === selectedDevice.boardProfileId,
          ) ?? [];
        setDeviceProfileId(created?.profile.id ?? "");
        setHardwareProfileId(hardware[0]?.id ?? "");
      }
      setCreatingProfile(false);
      return created;
    } catch (operationError) {
      setError(errorMessage(operationError));
      return null;
    } finally {
      setPending(false);
    }
  }

  async function createBlankProfile() {
    if (!selectedDevice || !name.trim() || pending) return null;
    return createProfile({
      kind: "blank",
      name: name.trim(),
      board_profile_id: selectedDevice.boardProfileId,
    });
  }

  async function complete(
    nextDeviceProfileId = deviceProfileId,
    nextHardwareProfileId = hardwareProfileId,
    prepare?: () => Promise<RuntimeAssignment>,
  ) {
    if (
      !selectedDevice ||
      !name.trim() ||
      pending
    ) {
      return;
    }
    setPending(true);
    setError(null);
    try {
      const assignment = prepare
        ? await prepare()
        : {
            device_profile_id: nextDeviceProfileId,
            hardware_profile_id: nextHardwareProfileId,
          };
      if (!assignment.device_profile_id || !assignment.hardware_profile_id) {
        return;
      }
      setDeviceProfileId(assignment.device_profile_id);
      setHardwareProfileId(assignment.hardware_profile_id);
      await onComplete(selectedDevice.deviceId, name.trim(), {
        device_profile_id: assignment.device_profile_id,
        hardware_profile_id: assignment.hardware_profile_id,
      });
      setLocalVerification({ status: "waiting" });
      setStep("verify");
    } catch (operationError) {
      setError(errorMessage(operationError));
    } finally {
      setPending(false);
    }
  }

  async function continueTemplate() {
    if (!selectedDevice || pending) return;
    if (deviceProfileId && hardwareProfileId) {
      await complete(
        deviceProfileId,
        hardwareProfileId,
        onPrepareProfile
          ? () =>
              onPrepareProfile(
                selectedDevice.deviceId,
                name.trim(),
                deviceProfileId,
                hardwareProfileId,
              )
          : undefined,
      );
      return;
    }
    if (onPrepareProfile) {
      await complete(
        "",
        "",
        () => onPrepareProfile(selectedDevice.deviceId, name.trim(), null, null),
      );
      return;
    }
    const created = await createBlankProfile();
    if (!created || !selectedDevice) return;
    const hardware = created.hardware_profiles.find(
      (item) => item.board_profile_id === selectedDevice.boardProfileId,
    );
    if (!hardware) {
      setError(t(language, "setup.noCompatibleHardware"));
      return;
    }
    await complete(created.profile.id, hardware.id);
  }

  async function retryVerification() {
    if (!setupDevice || pending) return;
    setError(null);
    setLocalVerification({ status: "waiting" });
    try {
      await onVerificationRetry?.(setupDevice.deviceId);
    } catch (operationError) {
      setError(errorMessage(operationError));
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
  const identityNeedsResolution =
    selectedCandidate !== null &&
    (selectedCandidate.issue === "invalid_identity" ||
      selectedCandidate.issue === "duplicate_identity" ||
      selectedCandidate.identity === "invalid_identity" ||
      selectedCandidate.identity === "duplicate_identity");

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
          {setupDevice && !selectedCandidate && !creatingProfile ? (
            <ol
              className="setup-progress"
              aria-label={t(language, "setup.progress")}
            >
              {(
                [
                  ["identify", "setup.stepIdentify"],
                  ["template", "setup.stepTemplate"],
                  ["verify", "setup.stepVerify"],
                ] as const
              ).map(([stepId, labelKey], index) => (
                <li
                  className={
                    step === stepId
                      ? "is-active"
                      : step === "verify" ||
                          (step === "template" && index === 0)
                        ? "is-complete"
                        : ""
                  }
                  aria-current={step === stepId ? "step" : undefined}
                  key={stepId}
                >
                  <span>
                    {step === "verify" || (step === "template" && index === 0) ? (
                      <Check size={13} />
                    ) : (
                      index + 1
                    )}
                  </span>
                  {t(language, labelKey)}
                </li>
              ))}
            </ol>
          ) : null}
          {creatingProfile ? (
            <CreateDeviceProfileForm
              language={language}
              boardProfiles={boardProfiles}
              deviceProfiles={deviceProfiles}
              fixedBoardProfileId={
                selectedCandidate?.boardProfileId ?? selectedDevice?.boardProfileId
              }
              onCreate={async (request) => {
                await createProfile(request);
              }}
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
                {!identityNeedsResolution ? (
                  <button
                    type="button"
                    disabled={pending}
                    onClick={() => setCreatingProfile(true)}
                  >
                    {t(language, "setup.createFirst")}
                  </button>
                ) : null}
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
          ) : !selectedDevice && step !== "verify" ? (
            <section className="setup-empty">
              <h3>{t(language, "setup.disconnected")}</h3>
            </section>
          ) : step === "verify" && setupDevice ? (
            <section className="setup-step setup-verification">
              <h3>{t(language, "setup.verifyTitle")}</h3>
              <p>{t(language, "setup.verifyBody", { name: setupDevice.name })}</p>
              <div
                className={`setup-verification-status is-${verificationState.status}`}
                role="status"
                aria-live="polite"
              >
                {verificationState.status === "success" ? (
                  <Check size={24} aria-hidden="true" />
                ) : (
                  <Keyboard size={24} aria-hidden="true" />
                )}
                <span>
                  <strong>
                    {verificationState.status === "success"
                      ? t(language, "setup.verifySuccess")
                      : verificationButton}
                  </strong>
                  <small>
                    {verificationState.status === "waiting"
                      ? t(language, "setup.verifyWaiting")
                      : verificationState.status === "timeout"
                        ? t(language, "setup.verifyTimeout")
                        : verificationState.status === "error"
                          ? t(language, "setup.verifyError")
                          : t(language, "setup.verifySuccessBody")}
                  </small>
                </span>
              </div>
              {verificationState.detail ? (
                <p className="setup-verification-detail">
                  {verificationState.detail}
                </p>
              ) : null}
              <div className="device-setup-actions">
                {verificationState.status === "success" ? (
                  <button
                    className="primary-button"
                    type="button"
                    onClick={onClose}
                  >
                    {t(language, "setup.openWorkspace")}
                  </button>
                ) : (
                  <>
                    {(verificationState.status === "timeout" ||
                      verificationState.status === "error") && (
                      <button
                        type="button"
                        disabled={pending}
                        onClick={() => void retryVerification()}
                      >
                        <RefreshCw size={16} />
                        {t(language, "setup.verifyRetry")}
                      </button>
                    )}
                    <button type="button" disabled={pending} onClick={onClose}>
                      {t(language, "setup.verifyLater")}
                    </button>
                  </>
                )}
              </div>
              <details className="device-technical-details">
                <summary>{t(language, "setup.technicalDetails")}</summary>
                <dl>
                  <dt>{t(language, "devices.serial")}</dt>
                  <dd>{serialSuffix(setupDevice.hardwareSerial)}</dd>
                  <dt>{t(language, "devices.id")}</dt>
                  <dd>{setupDevice.deviceId}</dd>
                  <dt>{t(language, "devices.board")}</dt>
                  <dd>{boardName(setupDevice.boardProfileId)}</dd>
                  <dt>{t(language, "setup.deviceProfile")}</dt>
                  <dd>{selectedProfile?.profile.name ?? deviceProfileId}</dd>
                  <dt>{t(language, "hardware.profile")}</dt>
                  <dd>{hardwareProfileId}</dd>
                </dl>
              </details>
            </section>
          ) : step === "identify" && selectedDevice ? (
            <section className="setup-step setup-identify">
              <div className="setup-step-device">
                <Keyboard size={24} aria-hidden="true" />
                <span>
                  <small>{t(language, "setup.detectedDevice")}</small>
                  <strong>{selectedDevice.name}</strong>
                </span>
              </div>
              <h3>{t(language, "setup.identifyTitle")}</h3>
              <p>{t(language, "setup.identifyBody")}</p>
              <label className="setup-name-field">
                <span>{t(language, "setup.deviceName")}</span>
                <input
                  aria-label={t(language, "setup.deviceName")}
                  value={name}
                  disabled={pending}
                  onChange={(event) => setName(event.target.value)}
                />
              </label>
              <details className="device-technical-details">
                <summary>{t(language, "setup.technicalDetails")}</summary>
                <dl>
                  <dt>{t(language, "devices.serial")}</dt>
                  <dd>{serialSuffix(selectedDevice.hardwareSerial)}</dd>
                  <dt>{t(language, "devices.id")}</dt>
                  <dd>{selectedDevice.deviceId}</dd>
                  <dt>{t(language, "devices.board")}</dt>
                  <dd>{boardName(selectedDevice.boardProfileId)}</dd>
                  <dt>{t(language, "devices.mode")}</dt>
                  <dd>{selectedDevice.mode ?? "-"}</dd>
                </dl>
              </details>
              <div className="device-setup-actions">
                <button
                  className="primary-button"
                  type="button"
                  disabled={pending || !name.trim()}
                  onClick={() => setStep("template")}
                >
                  {t(language, "setup.next")}
                  <ChevronRight size={16} />
                </button>
              </div>
            </section>
          ) : selectedDevice ? (
            <section className="setup-step setup-template-choice">
              <h3>{t(language, "setup.chooseTitle")}</h3>
              <p>{t(language, "setup.chooseBody")}</p>
              <div
                className="setup-template-grid"
                role="radiogroup"
                aria-label={t(language, "setup.templateGroup")}
              >
                {compatible.map((profile, index) => {
                  const selected = profile.profile.id === deviceProfileId;
                  return (
                    <button
                      className={`setup-template-card${selected ? " is-selected" : ""}`}
                      type="button"
                      role="radio"
                      aria-checked={selected}
                      key={profile.profile.id}
                      disabled={pending}
                      onClick={() => selectProfile(profile.profile.id)}
                    >
                      <span className="setup-template-icon" aria-hidden="true">
                        {selected ? <Check size={18} /> : <Keyboard size={18} />}
                      </span>
                      <span className="setup-template-copy">
                        <strong>{profile.profile.name}</strong>
                        <small>
                          {index === 0
                            ? t(language, "setup.templateHintPrimary")
                            : t(language, "setup.templateHint")}
                        </small>
                      </span>
                    </button>
                  );
                })}
                <button
                  className={`setup-template-card${!deviceProfileId ? " is-selected" : ""}`}
                  type="button"
                  role="radio"
                  aria-checked={!deviceProfileId}
                  disabled={pending}
                  onClick={() => {
                    setDeviceProfileId("");
                    setHardwareProfileId("");
                    setError(null);
                  }}
                >
                  <span className="setup-template-icon" aria-hidden="true">
                    <FilePlus2 size={18} />
                  </span>
                  <span className="setup-template-copy">
                    <strong>{t(language, "setup.blankChoice")}</strong>
                    <small>{t(language, "setup.blankChoiceHint")}</small>
                  </span>
                </button>
              </div>
              {selectedProfile ? (
                <details className="setup-advanced-choice">
                  <summary>{t(language, "setup.advancedHardware")}</summary>
                  <p>{t(language, "setup.advancedHardwareHint")}</p>
                  {compatibleHardware.length > 1 ? (
                    <label>
                      <span>{t(language, "setup.hardwareChoice")}</span>
                      <select
                        aria-label={t(language, "setup.hardwareChoice")}
                        value={hardwareProfileId}
                        disabled={pending}
                        onChange={(event) =>
                          setHardwareProfileId(event.target.value)
                        }
                      >
                        {compatibleHardware.map((hardware) => (
                          <option key={hardware.id} value={hardware.id}>
                            {hardware.name}
                          </option>
                        ))}
                      </select>
                    </label>
                  ) : (
                    <p className="setup-auto-choice">
                      {t(language, "setup.hardwareAuto", {
                        name: compatibleHardware[0]?.name ?? "-",
                      })}
                    </p>
                  )}
                </details>
              ) : null}
              {!compatible.length ? (
                <p className="field-error" role="alert">
                  {t(language, "setup.noTemplateHint")}
                </p>
              ) : null}
              <div className="device-setup-actions">
                <button
                  type="button"
                  disabled={pending}
                  onClick={() => setStep("identify")}
                >
                  {t(language, "setup.back")}
                </button>
                <button
                  className="primary-button"
                  type="button"
                  disabled={
                    pending ||
                    !name.trim() ||
                    (!!deviceProfileId && !hardwareProfileId)
                  }
                  onClick={() => void continueTemplate()}
                >
                  {t(language, "setup.next")}
                  <ChevronRight size={16} />
                </button>
              </div>
            </section>
          ) : null}
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
