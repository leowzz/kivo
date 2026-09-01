import { CircleAlert } from "lucide-react";
import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import type { ModelLayout, TriggerActions } from "./types";

interface KeypadProps {
  layout: ModelLayout;
  actions: Record<string, TriggerActions>;
  selectedButtonId: string | null;
  pressedButtonIds: Set<string>;
  failedButtonIds?: Set<string>;
  failureLabel?: string;
  actionCountLabel(count: number): string;
  onSelect(buttonId: string): void;
  onEscape?(): void;
}

type NavigationDirection = "left" | "right" | "up" | "down";

interface NavigationEntry {
  buttonId: string;
  groupIndex: number;
  buttonIndex: number;
  row: number;
  column: number;
}

function navigationEntries(layout: ModelLayout): NavigationEntry[] {
  return layout.groups.flatMap((group, groupIndex) => {
    const columns = Math.max(1, Math.floor(group.columns));
    return group.buttons.map((button, buttonIndex) => ({
      buttonId: button.id,
      groupIndex,
      buttonIndex,
      row: Math.floor(buttonIndex / columns),
      column: buttonIndex % columns,
    }));
  });
}

function nearestEntry(
  entries: NavigationEntry[],
  column: number,
  direction: NavigationDirection,
) {
  return [...entries].sort((left, right) => {
    const columnDistance = Math.abs(left.column - column) - Math.abs(right.column - column);
    if (columnDistance !== 0) return columnDistance;
    return direction === "up" || direction === "left"
      ? right.row - left.row
      : left.row - right.row;
  })[0] ?? null;
}

function targetEntry(
  current: NavigationEntry,
  direction: NavigationDirection,
  entries: NavigationEntry[],
): NavigationEntry | null {
  const sameGroup = entries.filter((entry) => entry.groupIndex === current.groupIndex);
  const delta = direction === "left" || direction === "up" ? -1 : 1;
  if (direction === "left" || direction === "right") {
    const candidate = sameGroup.find(
      (entry) => entry.buttonIndex === current.buttonIndex + delta && entry.row === current.row,
    );
    if (candidate) return candidate;

    const adjacentRow = sameGroup.filter((entry) => entry.row === current.row + delta);
    if (adjacentRow.length > 0) {
      return direction === "right"
        ? adjacentRow.sort((left, right) => left.column - right.column)[0]
        : adjacentRow.sort((left, right) => right.column - left.column)[0];
    }
  } else {
    const adjacentRow = sameGroup.filter((entry) => entry.row === current.row + delta);
    const candidate = [...adjacentRow].sort(
      (left, right) => Math.abs(left.column - current.column) - Math.abs(right.column - current.column),
    )[0];
    if (candidate) return candidate;
  }

  const adjacentGroupIndex = current.groupIndex + delta;
  return nearestEntry(
    entries.filter((entry) => entry.groupIndex === adjacentGroupIndex),
    current.column,
    direction,
  );
}

export function Keypad({
  layout,
  actions,
  selectedButtonId,
  pressedButtonIds,
  failedButtonIds = new Set<string>(),
  failureLabel = "Action failed",
  actionCountLabel,
  onSelect,
  onEscape,
}: KeypadProps) {
  const summaryPrefix = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const buttonElements = useRef<Record<string, HTMLButtonElement | null>>({});
  const entries = useMemo(() => navigationEntries(layout), [layout]);
  const entriesById = useMemo(
    () => new Map(entries.map((entry) => [entry.buttonId, entry])),
    [entries],
  );
  const firstButtonId = entries[0]?.buttonId ?? null;
  const [focusedButtonId, setFocusedButtonId] = useState<string | null>(
    () => selectedButtonId ?? firstButtonId,
  );

  useEffect(() => {
    setFocusedButtonId((current) => {
      if (current && entriesById.has(current)) return current;
      return selectedButtonId && entriesById.has(selectedButtonId)
        ? selectedButtonId
        : firstButtonId;
    });
  }, [entriesById, firstButtonId, selectedButtonId]);

  useEffect(() => {
    if (selectedButtonId && entriesById.has(selectedButtonId)) {
      setFocusedButtonId(selectedButtonId);
    }
  }, [entriesById, selectedButtonId]);

  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>, buttonId: string) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onEscape?.();
      return;
    }
    if (event.key === "Enter" || event.key === " " || event.key === "Spacebar") {
      event.preventDefault();
      onSelect(buttonId);
      return;
    }
    const direction: NavigationDirection | null = event.key === "ArrowLeft"
      ? "left"
      : event.key === "ArrowRight"
        ? "right"
        : event.key === "ArrowUp"
          ? "up"
          : event.key === "ArrowDown"
            ? "down"
            : null;
    if (!direction) return;
    event.preventDefault();
    const current = entriesById.get(buttonId);
    const target = current ? targetEntry(current, direction, entries) : null;
    if (!target) return;
    setFocusedButtonId(target.buttonId);
    buttonElements.current[target.buttonId]?.focus();
  };

  return (
    <div
      className="keypad"
      role="group"
      aria-label={layout.name}
      aria-keyshortcuts="ArrowLeft ArrowRight ArrowUp ArrowDown Enter Escape"
    >
      {layout.groups.map((group, groupIndex) => {
        const columns = Math.max(1, Math.floor(group.columns));
        const rows = Math.ceil(group.buttons.length / columns);
        return <div
          className="key-group"
          key={group.id}
          style={{
            flexGrow: rows / columns,
            gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
            gridTemplateRows: `repeat(${rows}, minmax(72px, 1fr))`,
          }}
        >
          {group.buttons.map((button, buttonIndex) => {
            const triggerActions = actions[button.id];
            const count = triggerActions
              ? (triggerActions.press?.length ?? 0) +
                (triggerActions.release?.length ?? 0) +
                (triggerActions.long_press?.length ?? 0) +
                (triggerActions.double_press?.length ?? 0)
              : 0;
            const actionSummary = actionCountLabel(count);
            const isSelected = selectedButtonId === button.id;
            const isPressed = pressedButtonIds.has(button.id);
            const isFailed = failedButtonIds.has(button.id);
            const labelLength = Array.from(button.label).length;
            const labelSizeClass = labelLength > 18
              ? "is-long"
              : labelLength > 10
                ? "is-medium"
                : "";
            const summaryId = `${summaryPrefix}-action-summary-${groupIndex}-${buttonIndex}`;
            return (
              <button
                ref={(element) => {
                  buttonElements.current[button.id] = element;
                }}
                className={[
                  "key-button",
                  isSelected ? "is-selected" : "",
                  isPressed ? "is-pressed" : "",
                  isFailed ? "is-failed" : "",
                  count > 0 ? "has-action-count" : "",
                ].filter(Boolean).join(" ")}
                type="button"
                title={button.label}
                aria-label={`${button.label}，${actionSummary}${isFailed ? `，${failureLabel}` : ""}`}
                aria-describedby={summaryId}
                aria-current={isSelected ? "true" : undefined}
                aria-selected={isSelected}
                aria-pressed={isPressed}
                tabIndex={focusedButtonId === button.id ? 0 : -1}
                key={button.id}
                onFocus={() => setFocusedButtonId(button.id)}
                onKeyDown={(event) => handleKeyDown(event, button.id)}
                onClick={() => onSelect(button.id)}
              >
                <span className={`key-button-label ${labelSizeClass}`.trim()}>{button.label}</span>
                <span className="key-action-summary" id={summaryId}>{actionSummary}</span>
                {isFailed ? <CircleAlert className="key-error-indicator" size={14} aria-hidden="true" /> : null}
                <span className="key-button-count" aria-hidden="true">
                  {count > 0 ? actionSummary : ""}
                </span>
              </button>
            );
          })}
        </div>;
      })}
    </div>
  );
}
