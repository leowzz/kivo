import { useRef } from "react";
import { ButtonPopover } from "./ButtonPopover";
import { formatHotkey } from "./hotkey";
import type { ButtonAction, ConfigMode, ModelLayout } from "./types";

interface KeypadProps {
  layout: ModelLayout;
  mode: ConfigMode;
  ioMap: Record<number, string>;
  actions: Record<string, ButtonAction>;
  supportedGpios: number[];
  pressedButtonIds: ReadonlySet<string>;
  selectedButtonId: string | null;
  selectedAnchor: DOMRect | null;
  capturedGpio: number | null;
  onSelect(buttonId: string, anchor: DOMRect): void;
  onApplyIoMap(ioMap: Record<number, string>): void;
  onApplyAction(buttonId: string, action: ButtonAction): void;
  onDeleteAction(buttonId: string): void;
  onCancel(): void;
}

const KEY_WIDTH = 84;
const KEY_GAP = 8;
const POPOVER_WIDTH = 240;
const IO_POPOVER_HEIGHT = 180;
const BEHAVIOR_POPOVER_HEIGHT = 280;
const POPOVER_GAP = 12;
export function gpioForButton(ioMap: Record<number, string>, buttonId: string) {
  const entry = Object.entries(ioMap).find(([, value]) => value === buttonId);
  return entry ? Number(entry[0]) : null;
}

export function popoverPosition(
  anchor: Pick<DOMRect, "left" | "right" | "top">,
  width: number,
  height: number,
  viewportWidth: number,
  viewportHeight: number,
) {
  const desiredLeft = anchor.right + POPOVER_GAP + width <= viewportWidth - POPOVER_GAP
    ? anchor.right + POPOVER_GAP
    : anchor.left - width - POPOVER_GAP;
  const maxLeft = Math.max(POPOVER_GAP, viewportWidth - width - POPOVER_GAP);
  const maxTop = Math.max(POPOVER_GAP, viewportHeight - height - POPOVER_GAP);
  return {
    left: Math.min(Math.max(POPOVER_GAP, desiredLeft), maxLeft),
    top: Math.min(Math.max(POPOVER_GAP, anchor.top), maxTop),
  };
}

function behaviorSummary(action: ButtonAction | undefined) {
  if (!action) return "No action";
  if (action.type === "hotkey") {
    return formatHotkey(action.keys);
  }
  const text = action.text.replace(/\s+/g, " ").trim();
  return text.length > 32 ? `${text.slice(0, 29)}...` : text;
}

export function Keypad({
  layout,
  mode,
  ioMap,
  actions,
  supportedGpios,
  pressedButtonIds,
  selectedButtonId,
  selectedAnchor,
  capturedGpio,
  onSelect,
  onApplyIoMap,
  onApplyAction,
  onDeleteAction,
  onCancel,
}: KeypadProps) {
  const buttonElements = useRef<Record<string, HTMLButtonElement | null>>({});
  const selectedButton = layout.groups
    .flatMap((group) => group.buttons)
    .find((button) => button.id === selectedButtonId);
  const buttonLabels = Object.fromEntries(
    layout.groups.flatMap((group) => group.buttons.map((button) => [button.id, button.label])),
  );

  return (
    <>
      <div className="keypad" aria-label={`${layout.name} keypad`}>
        {layout.groups.map((group, groupIndex) => (
          <div
            className="key-group"
            data-testid={`group-${group.id}`}
            key={group.id}
            style={{
              gridTemplateColumns: `repeat(${group.columns}, minmax(0, 1fr))`,
              maxWidth: group.columns * KEY_WIDTH + (group.columns - 1) * KEY_GAP,
            }}
          >
            {group.buttons.map((button, buttonIndex) => {
              const gpio = gpioForButton(ioMap, button.id);
              const summaryId = `key-summary-${layout.id}-${groupIndex}-${buttonIndex}`;
              const summary = mode === "io"
                ? gpio === null ? "Unmapped" : `GPIO ${gpio}`
                : behaviorSummary(actions[button.id]);
              return (
                <div className="key-shell" key={button.id}>
                  <button
                    ref={(element) => {
                      buttonElements.current[button.id] = element;
                    }}
                    className={[
                      "key",
                      selectedButtonId === button.id && "is-selected",
                      pressedButtonIds.has(button.id) && "is-physically-pressed",
                    ].filter(Boolean).join(" ")}
                    type="button"
                    aria-label={`Configure ${button.label}`}
                    aria-describedby={summaryId}
                    aria-pressed={selectedButtonId === button.id}
                    onClick={(event) =>
                      onSelect(button.id, event.currentTarget.getBoundingClientRect())
                    }
                  >
                    {button.label}
                  </button>
                  <span className="key-summary" id={summaryId} role="tooltip">{summary}</span>
                </div>
              );
            })}
          </div>
        ))}
      </div>
      {selectedButton && selectedAnchor && (
        <ButtonPopover
          key={selectedButton.id}
          mode={mode}
          buttonId={selectedButton.id}
          buttonLabel={selectedButton.label}
          buttonLabels={buttonLabels}
          ioMap={ioMap}
          supportedGpios={supportedGpios}
          capturedGpio={capturedGpio}
          action={actions[selectedButton.id]}
          position={popoverPosition(
              selectedAnchor,
              POPOVER_WIDTH,
              mode === "io" ? IO_POPOVER_HEIGHT : BEHAVIOR_POPOVER_HEIGHT,
            window.innerWidth,
            window.innerHeight,
          )}
          onApplyIoMap={onApplyIoMap}
          onApplyAction={(action) => onApplyAction(selectedButton.id, action)}
          onDeleteAction={() => onDeleteAction(selectedButton.id)}
          onSelectConflict={(buttonId) => {
            const button = buttonElements.current[buttonId];
            if (button) onSelect(buttonId, button.getBoundingClientRect());
          }}
          onCancel={onCancel}
        />
      )}
    </>
  );
}
