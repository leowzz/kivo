import type { ModelLayout, TriggerActions } from "./types";

interface KeypadProps {
  layout: ModelLayout;
  actions: Record<string, TriggerActions>;
  selectedButtonId: string | null;
  pressedButtonIds: Set<string>;
  actionCountLabel(count: number): string;
  unconfiguredLabel: string;
  onSelect(buttonId: string): void;
}

export function Keypad({
  layout,
  actions,
  selectedButtonId,
  pressedButtonIds,
  actionCountLabel,
  unconfiguredLabel,
  onSelect,
}: KeypadProps) {
  return (
    <div className="keypad" aria-label={layout.name}>
      {layout.groups.map((group) => {
        const rows = Math.ceil(group.buttons.length / group.columns);
        return <div
          className="key-group"
          key={group.id}
          style={{
            flexGrow: rows / group.columns,
            gridTemplateColumns: `repeat(${group.columns}, minmax(0, 1fr))`,
            gridTemplateRows: `repeat(${rows}, minmax(0, 1fr))`,
          }}
        >
          {group.buttons.map((button) => {
            const triggerActions = actions[button.id];
            const count = triggerActions
              ? (triggerActions.press?.length ?? 0) +
                (triggerActions.release?.length ?? 0) +
                (triggerActions.long_press?.length ?? 0) +
                (triggerActions.double_press?.length ?? 0)
              : 0;
            return (
              <button
                className={[
                  "key-button",
                  selectedButtonId === button.id ? "is-selected" : "",
                  pressedButtonIds.has(button.id) ? "is-pressed" : "",
                  count === 0 ? "is-unconfigured" : "",
                ].filter(Boolean).join(" ")}
                type="button"
                aria-pressed={selectedButtonId === button.id}
                aria-label={`${button.label}，${actionCountLabel(count)}`}
                key={button.id}
                onClick={() => onSelect(button.id)}
              >
                <span>{button.label}</span>
                {count === 0
                  ? <small className="key-state" aria-hidden="true">{unconfiguredLabel}</small>
                  : <small aria-hidden="true">{count}</small>}
              </button>
            );
          })}
        </div>;
      })}
    </div>
  );
}
