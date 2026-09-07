import { useMemo, useState } from "react";
import { t } from "./i18n";
import type {
  BoardProfileSummary,
  CreateDeviceProfileRequest,
  DeviceProfile,
  Language,
} from "./types";

interface CreateDeviceProfileFormProps {
  language: Language;
  boardProfiles: BoardProfileSummary[];
  deviceProfiles: DeviceProfile[];
  fixedBoardProfileId?: string;
  initialSourceProfileId?: string;
  onCreate(request: CreateDeviceProfileRequest): Promise<void>;
  onCancel(): void;
}

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error && "code" in error) {
    return String(error.code);
  }
  return String(error);
}

export function CreateDeviceProfileForm({
  language,
  boardProfiles,
  deviceProfiles,
  fixedBoardProfileId,
  initialSourceProfileId,
  onCreate,
  onCancel,
}: CreateDeviceProfileFormProps) {
  const cloneSources = useMemo(
    () =>
      fixedBoardProfileId
        ? deviceProfiles.filter((profile) =>
            profile.hardware_profiles.some(
              (hardware) => hardware.board_profile_id === fixedBoardProfileId,
            ),
          )
        : deviceProfiles,
    [deviceProfiles, fixedBoardProfileId],
  );
  const [mode, setMode] = useState<"clone" | "blank">(
    cloneSources.length > 0 ? "clone" : "blank",
  );
  const [name, setName] = useState("");
  const [sourceProfileId, setSourceProfileId] = useState(
    cloneSources.some((profile) => profile.profile.id === initialSourceProfileId)
      ? initialSourceProfileId!
      : cloneSources[0]?.profile.id ?? "",
  );
  const [boardProfileId, setBoardProfileId] = useState(fixedBoardProfileId ?? "");
  const [pending, setPending] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  const [operationError, setOperationError] = useState<string | null>(null);
  const validationError =
    name.trim().length === 0
      ? t(language, "profile.nameRequired")
      : mode === "clone" && !sourceProfileId
        ? t(language, "profile.sourceRequired")
        : mode === "blank" && !(fixedBoardProfileId ?? boardProfileId)
          ? t(language, "profile.boardRequired")
          : null;

  async function submit() {
    setSubmitted(true);
    if (pending || validationError) return;
    const request: CreateDeviceProfileRequest =
      mode === "clone"
        ? {
            kind: "clone",
            name: name.trim(),
            source_profile_id: sourceProfileId,
          }
        : {
            kind: "blank",
            name: name.trim(),
            board_profile_id: fixedBoardProfileId ?? boardProfileId,
          };
    setPending(true);
    setOperationError(null);
    try {
      await onCreate(request);
    } catch (error) {
      setOperationError(errorMessage(error));
    } finally {
      setPending(false);
    }
  }

  return (
    <form
      className="profile-create-form"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <fieldset className="profile-create-mode" disabled={pending}>
        <legend>{t(language, "profile.mode")}</legend>
        <label>
          <input
            type="radio"
            name="profile-mode"
            checked={mode === "clone"}
            disabled={cloneSources.length === 0}
            onChange={() => setMode("clone")}
          />
          {t(language, "profile.clone")}
        </label>
        <label>
          <input
            type="radio"
            name="profile-mode"
            checked={mode === "blank"}
            onChange={() => setMode("blank")}
          />
          {t(language, "profile.blank")}
        </label>
      </fieldset>
      <label className="profile-create-field">
        <span>{t(language, "profile.name")}</span>
        <input
          aria-label={t(language, "profile.name")}
          value={name}
          disabled={pending}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      {mode === "clone" && (
        <label className="profile-create-field">
          <span>{t(language, "profile.source")}</span>
          <select
            aria-label={t(language, "profile.source")}
            value={sourceProfileId}
            disabled={pending}
            onChange={(event) => setSourceProfileId(event.target.value)}
          >
            {cloneSources.map((profile) => (
              <option key={profile.profile.id} value={profile.profile.id}>
                {profile.profile.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {mode === "blank" && !fixedBoardProfileId && (
        <label className="profile-create-field">
          <span>{t(language, "profile.board")}</span>
          <select
            aria-label={t(language, "profile.board")}
            value={boardProfileId}
            disabled={pending}
            onChange={(event) => setBoardProfileId(event.target.value)}
          >
            <option value="">-</option>
            {boardProfiles.map((board) => (
              <option key={board.id} value={board.id}>
                {board.displayName}
              </option>
            ))}
          </select>
        </label>
      )}
      {submitted && validationError && (
        <p className="field-error" role="alert">
          {validationError}
        </p>
      )}
      {operationError && (
        <p className="field-error" role="alert">
          {operationError}
        </p>
      )}
      <div className="profile-create-actions">
        <button type="button" disabled={pending} onClick={onCancel}>
          {t(language, "common.cancel")}
        </button>
        <button className="primary-button" type="submit" disabled={pending}>
          {t(language, "profile.createAction")}
        </button>
      </div>
    </form>
  );
}
