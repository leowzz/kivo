import { useEffect, useState } from "react";

interface ButtonPopoverProps {
  buttonId: string;
  buttonLabel: string;
  buttonLabels: Record<string, string>;
  ioMap: Record<number, string>;
  supportedGpios: number[];
  capturedGpio: number | null;
  position: { left: number; top: number };
  onApply(ioMap: Record<number, string>): void;
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
  buttonId,
  buttonLabel,
  buttonLabels,
  ioMap,
  supportedGpios,
  capturedGpio,
  position,
  onApply,
  onCancel,
}: ButtonPopoverProps) {
  const currentGpio = Object.entries(ioMap).find(([, value]) => value === buttonId)?.[0];
  const [gpio, setGpio] = useState<number | null>(
    currentGpio === undefined ? null : Number(currentGpio),
  );

  useEffect(() => {
    if (capturedGpio !== null) setGpio(capturedGpio);
  }, [capturedGpio]);

  const options = [...new Set([
    ...supportedGpios,
    ...(gpio !== null ? [gpio] : []),
  ])].sort((left, right) => left - right);
  const conflict = gpio === null ? null : ioConflict(ioMap, buttonId, gpio);

  return (
    <div
      className="io-popover"
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
          GPIO{gpio} is assigned to {buttonLabels[conflict] ?? conflict}
        </p>
      )}
      <div className="io-popover-actions">
        <button type="button" aria-label="Cancel IO mapping" onClick={onCancel}>Cancel</button>
        <button
          className="io-apply"
          type="button"
          aria-label="Apply IO mapping"
          disabled={gpio === null || conflict !== null}
          onClick={() => gpio !== null && onApply(bindGpio(ioMap, buttonId, gpio))}
        >
          Apply
        </button>
      </div>
    </div>
  );
}
