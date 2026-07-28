import type { ButtonAction, ConfigMode, ModelLayout } from "./types";

interface KeypadProps {
  layout: ModelLayout;
  mode: ConfigMode;
  ioMap: Record<number, string>;
  actions: Record<string, ButtonAction>;
  selectedButtonId: string | null;
  onSelect(buttonId: string, anchor: DOMRect): void;
}

const KEY_WIDTH = 76;
const KEY_GAP = 8;
const HOTKEY_LABELS: Record<string, string> = {
  alt: "Option",
  option: "Option",
  cmd: "Command",
  ctrl: "Control",
  shift: "Shift",
  enter: "Enter",
  escape: "Escape",
  backspace: "Backspace",
  tab: "Tab",
  space: "Space",
  arrow_up: "Arrow Up",
  arrow_down: "Arrow Down",
  arrow_left: "Arrow Left",
  arrow_right: "Arrow Right",
  up: "Arrow Up",
  down: "Arrow Down",
  left: "Arrow Left",
  right: "Arrow Right",
  delete: "Delete",
  home: "Home",
  end: "End",
  pageup: "Page Up",
  page_up: "Page Up",
  pagedown: "Page Down",
  page_down: "Page Down",
};

export function gpioForButton(ioMap: Record<number, string>, buttonId: string) {
  const entry = Object.entries(ioMap).find(([, value]) => value === buttonId);
  return entry ? Number(entry[0]) : null;
}

function behaviorSummary(action: ButtonAction | undefined) {
  if (!action) return "No action";
  if (action.type === "hotkey") {
    return action.keys
      .map((key) => HOTKEY_LABELS[key.toLowerCase()] ?? key.toUpperCase())
      .join(" + ");
  }
  const text = action.text.replace(/\s+/g, " ").trim();
  return text.length > 32 ? `${text.slice(0, 29)}...` : text;
}

export function Keypad({
  layout,
  mode,
  ioMap,
  actions,
  selectedButtonId,
  onSelect,
}: KeypadProps) {
  return (
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
                  className={selectedButtonId === button.id ? "key is-selected" : "key"}
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
  );
}
