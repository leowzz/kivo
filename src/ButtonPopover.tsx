import { useEffect, useState } from "react";
import { formatHotkey, normalizeHotkey } from "./hotkey";
import type { ButtonAction, ConfigMode } from "./types";

interface ButtonPopoverProps {
  mode: ConfigMode;
  buttonId: string;
  buttonLabel: string;
  buttonLabels: Record<string, string>;
  ioMap: Record<number, string>;
  supportedGpios: number[];
  capturedGpio: number | null;
  action: ButtonAction | undefined;
  position: { left: number; top: number };
  onApplyIoMap(ioMap: Record<number, string>): void;
  onApplyAction(action: ButtonAction): void;
  onDeleteAction(): void;
  onSelectConflict(buttonId: string): void;
  onCancel(): void;
}

export function ioConflict(
  ioMap: Record<number, string>,
  buttonId: string,
  gpio: number,
) {
  const existing = ioMap[gpio];
  return existing && existing !== buttonId ? existing : null;
}

export function bindGpio(
  ioMap: Record<number, string>,
  buttonId: string,
  gpio: number,
) {
  const next = Object.fromEntries(
    Object.entries(ioMap).filter(([, value]) => value !== buttonId),
  ) as Record<number, string>;
  next[gpio] = buttonId;
  return next;
}

export function ButtonPopover({
  mode,
  buttonId,
  buttonLabel,
  buttonLabels,
  ioMap,
  supportedGpios,
  capturedGpio,
  action,
  position,
  onApplyIoMap,
  onApplyAction,
  onDeleteAction,
  onSelectConflict,
  onCancel,
}: ButtonPopoverProps) {
  const currentGpio = Object.entries(ioMap).find(([, value]) => value === buttonId)?.[0];
  const [gpio, setGpio] = useState<number | null>(
    currentGpio === undefined ? null : Number(currentGpio),
  );
  const [draftAction, setDraftAction] = useState<ButtonAction>(
    action ?? { type: "paste", text: "" },
  );
  const [recording, setRecording] = useState(false);
  const [hotkeyError, setHotkeyError] = useState<string | null>(null);

  useEffect(() => {
    if (capturedGpio !== null) setGpio(capturedGpio);
  }, [capturedGpio]);

  useEffect(() => {
    if (!recording) return;
    const handler = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      try {
        const keys = normalizeHotkey(event);
        if (!keys) return;
        setDraftAction({ type: "hotkey", keys });
        setHotkeyError(null);
      } catch (recordError) {
        setHotkeyError(recordError instanceof Error ? recordError.message : String(recordError));
      }
      setRecording(false);
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [recording]);

  const options = [...new Set([
    ...supportedGpios,
    ...(gpio !== null ? [gpio] : []),
  ])].sort((left, right) => left - right);
  const conflict = gpio === null ? null : ioConflict(ioMap, buttonId, gpio);

  if (mode === "behavior") {
    const canApply = draftAction.type === "paste"
      ? draftAction.text.trim().length > 0
      : draftAction.keys.length > 0;
    return (
      <div
        className="button-popover"
        role="dialog"
        aria-label={`Configure behavior for ${buttonLabel}`}
        style={position}
      >
        <h3>Behavior: {buttonLabel}</h3>
        <label>
          <span>Action type</span>
          <select
            aria-label={`Action type for ${buttonLabel}`}
            value={draftAction.type}
            onChange={(event) => {
              setRecording(false);
              setHotkeyError(null);
              setDraftAction(event.target.value === "paste"
                ? { type: "paste", text: "" }
                : { type: "hotkey", keys: [] });
            }}
          >
            <option value="paste">Paste</option>
            <option value="hotkey">Shortcut</option>
          </select>
        </label>
        {draftAction.type === "paste" ? (
          <label>
            <span>Text</span>
            <textarea
              aria-label={`Paste text for ${buttonLabel}`}
              rows={5}
              value={draftAction.text}
              onChange={(event) => setDraftAction({ type: "paste", text: event.target.value })}
            />
          </label>
        ) : (
          <div className="shortcut-field">
            <span>Shortcut</span>
            <output aria-label={`Shortcut for ${buttonLabel}`}>
              {recording
                ? "Press shortcut"
                : draftAction.keys.length > 0 ? formatHotkey(draftAction.keys) : "Not recorded"}
            </output>
            <button type="button" onClick={() => {
              setHotkeyError(null);
              setRecording((current) => !current);
            }}>
              {recording ? "Cancel recording" : "Record shortcut"}
            </button>
          </div>
        )}
        {hotkeyError && <p className="behavior-error" role="alert">{hotkeyError}</p>}
        <div className="button-popover-actions">
          <button
            type="button"
            aria-label="Delete behavior"
            disabled={!action}
            onClick={onDeleteAction}
          >
            Delete
          </button>
          <span className="popover-action-spacer" />
          <button type="button" aria-label="Cancel behavior" onClick={onCancel}>Cancel</button>
          <button
            className="popover-apply"
            type="button"
            aria-label="Apply behavior"
            disabled={!canApply}
            onClick={() => onApplyAction(draftAction)}
          >
            Apply
          </button>
        </div>
      </div>
    );
  }

  return (
    <div
      className="button-popover"
      role="dialog"
      aria-label={`Configure IO for ${buttonLabel}`}
      style={position}
    >
      <h3>IO mapping: {buttonLabel}</h3>
      <label>
        <span>GPIO</span>
        <select
          aria-label={`GPIO for ${buttonLabel}`}
          value={gpio ?? ""}
          onChange={(event) => setGpio(Number(event.target.value))}
        >
          <option value="" disabled>Select</option>
          {options.map((value) => (
            <option value={value} key={value}>GPIO {value}</option>
          ))}
        </select>
      </label>
      {conflict && (
        <p className="io-conflict" role="alert">
          <span>GPIO{gpio} is assigned to {buttonLabels[conflict] ?? conflict}</span>
          <button type="button" onClick={() => onSelectConflict(conflict)}>
            Go to {buttonLabels[conflict] ?? conflict}
          </button>
        </p>
      )}
      <div className="button-popover-actions">
        <button type="button" aria-label="Cancel IO mapping" onClick={onCancel}>Cancel</button>
        <button
          className="popover-apply"
          type="button"
          aria-label="Apply IO mapping"
          disabled={gpio === null || conflict !== null}
          onClick={() => gpio !== null && onApplyIoMap(bindGpio(ioMap, buttonId, gpio))}
        >
          Apply
        </button>
      </div>
    </div>
  );
}
