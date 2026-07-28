import type { ButtonAction, ModelLayout } from "./types";

interface KeypadProps {
  layout: ModelLayout;
  actions: Record<string, ButtonAction[]>;
  selectedButtonId: string | null;
  pressedButtonIds: Set<string>;
  actionCountLabel(count: number): string;
  onSelect(buttonId: string): void;
}

export function Keypad({
  layout,
  actions,
  selectedButtonId,
  pressedButtonIds,
  actionCountLabel,
  onSelect,
}: KeypadProps) {
  return (
    <div className="keypad" aria-label={layout.name}>
      {layout.groups.map((group) => (
        <div
          className="key-group"
          key={group.id}
          style={{ gridTemplateColumns: `repeat(${group.columns}, minmax(0, 1fr))` }}
        >
          {group.buttons.map((button) => {
            const count = actions[button.id]?.length ?? 0;
            return (
              <button
                className={[
                  "key-button",
                  selectedButtonId === button.id ? "is-selected" : "",
                  pressedButtonIds.has(button.id) ? "is-pressed" : "",
                ].filter(Boolean).join(" ")}
                type="button"
                aria-pressed={selectedButtonId === button.id}
                aria-label={`${button.label}，${actionCountLabel(count)}`}
                key={button.id}
                onClick={() => onSelect(button.id)}
              >
                <span>{button.label}</span>
                {count > 0 && <small aria-hidden="true">{count}</small>}
              </button>
            );
          })}
        </div>
      ))}
    </div>
  );
}
