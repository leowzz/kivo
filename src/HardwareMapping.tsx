import { Cable, LayoutGrid, Plus, Radio, SquareStop, Trash2 } from "lucide-react";
import { t } from "./i18n";
import type { HardwareConfig, InputSource, Language, LearningSession, ModelLayout } from "./types";

interface HardwareMappingProps {
  language: Language;
  layout: ModelLayout;
  hardware: HardwareConfig;
  supportedGpios: number[];
  learning: LearningSession | null;
  selectedButtonId: string | null;
  onSelectButton(buttonId: string): void;
  onChange(hardware: HardwareConfig): void;
  onBeginLearning(pins: number[]): void;
  onEndLearning(): void;
}

function sourceName(language: Language, source: InputSource) {
  return t(language, source.type === "direct" ? "hardware.direct" : "hardware.matrix");
}

export function HardwareMapping({
  language,
  layout,
  hardware,
  supportedGpios,
  learning,
  selectedButtonId,
  onSelectButton,
  onChange,
  onBeginLearning,
  onEndLearning,
}: HardwareMappingProps) {
  const buttons = layout.groups.flatMap((group) => group.buttons);

  const updateSource = (index: number, source: InputSource) => {
    onChange({
      ...hardware,
      inputs: hardware.inputs.map((item, itemIndex) => itemIndex === index ? source : item),
    });
  };

  return (
    <section className="hardware-view" aria-labelledby="hardware-title">
      <div className="content-heading">
        <div>
          <span>{layout.name}</span>
          <h2 id="hardware-title">{t(language, "hardware.title")}</h2>
        </div>
        <label className="debounce-field">
          <span>{t(language, "hardware.debounce")}</span>
          <input
            type="number"
            min="1"
            max="1000"
            value={hardware.debounce_ms}
            onChange={(event) => onChange({ ...hardware, debounce_ms: Number(event.target.value) })}
          />
          <small>ms</small>
        </label>
      </div>

      <div className="source-list">
        {hardware.inputs.map((source, sourceIndex) => (
          <section className="source-editor" key={`${source.type}-${source.id}`}>
            <div className="source-heading">
              <div>
                {source.type === "direct" ? <Cable size={14} /> : <LayoutGrid size={14} />}
                <strong>{sourceName(language, source)}</strong>
                <code>{source.id}</code>
              </div>
              <button
                className="icon-button is-danger"
                type="button"
                aria-label={t(language, "hardware.removeSource")}
                title={t(language, "hardware.removeSource")}
                onClick={() => onChange({
                  ...hardware,
                  inputs: hardware.inputs.filter((_, index) => index !== sourceIndex),
                })}
              >
                <Trash2 size={16} />
              </button>
            </div>

            {source.type === "contact_matrix" && (
              <label className="field-stack compact-field">
                <span>{t(language, "hardware.matrixPins")}</span>
                <input
                  value={source.pins.join(", ")}
                  onChange={(event) => {
                    const pins = [...new Set(event.target.value.split(",")
                      .map((pin) => Number(pin.trim()))
                      .filter((pin) => Number.isInteger(pin) && pin >= 0 && pin <= 255))]
                      .sort((left, right) => left - right);
                    updateSource(sourceIndex, {
                      ...source,
                      pins,
                      keys: Object.fromEntries(Object.entries(source.keys).filter(([, pair]) =>
                        pins.includes(pair[0]) && pins.includes(pair[1])
                      )),
                    });
                  }}
                />
              </label>
            )}

            <div className="mapping-table">
              <div className="mapping-head">
                <span>{t(language, "hardware.key")}</span>
                <span>{source.type === "direct" ? "GPIO" : t(language, "hardware.contacts")}</span>
              </div>
              {buttons.map((button) => {
                return (
                  <div className={selectedButtonId === button.id ? "mapping-row is-selected" : "mapping-row"} key={button.id}>
                    <button type="button" onClick={() => onSelectButton(button.id)}>{button.label}</button>
                    {source.type === "direct" ? (
                      <input
                        type="number"
                        aria-label={`${button.label} GPIO`}
                        placeholder="-"
                        value={source.keys[button.id] ?? ""}
                        onChange={(event) => {
                          const keys = { ...source.keys };
                          if (event.target.value === "") delete keys[button.id];
                          else keys[button.id] = Number(event.target.value);
                          updateSource(sourceIndex, { ...source, keys });
                        }}
                      />
                    ) : (
                      <div className="contact-inputs">
                        {[0, 1].map((side) => (
                          <select
                            aria-label={`${button.label} ${side === 0 ? "A" : "B"}`}
                            value={source.keys[button.id]?.[side] ?? ""}
                            key={side}
                            onChange={(event) => {
                              const keys = { ...source.keys };
                              const currentPair = source.keys[button.id];
                              if (event.target.value === "") delete keys[button.id];
                              else {
                                const selected = Number(event.target.value);
                                const other = currentPair?.[side === 0 ? 1 : 0]
                                  ?? source.pins.find((pin) => pin !== selected);
                                if (other !== undefined && other !== selected) {
                                  keys[button.id] = selected < other ? [selected, other] : [other, selected];
                                }
                              }
                              updateSource(sourceIndex, { ...source, keys });
                            }}
                          >
                            <option value="">-</option>
                            {source.pins.map((pin) => <option value={pin} key={pin}>{pin}</option>)}
                          </select>
                        ))}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </section>
        ))}
      </div>

      <div className="source-actions">
        <button type="button" onClick={() => onChange({
          ...hardware,
          inputs: [...hardware.inputs, { type: "direct", id: `direct-${hardware.inputs.length + 1}`, keys: {} }],
        })}>
          <Plus size={16} />{t(language, "hardware.addDirect")}
        </button>
        <button type="button" onClick={() => onChange({
          ...hardware,
          inputs: [...hardware.inputs, {
            type: "contact_matrix",
            id: `matrix-${hardware.inputs.length + 1}`,
            pins: [],
            keys: {},
          }],
        })}>
          <Plus size={16} />{t(language, "hardware.addMatrix")}
        </button>
      </div>

      <details className="learning-panel">
        <summary>
          <Radio size={15} />
          <span>{t(language, "hardware.advanced")}</span>
          <small>{t(language, "hardware.advancedHint")}</small>
        </summary>
        <div className={learning ? "learning-controls is-learning" : "learning-controls"}>
          <label className="field-stack compact-field">
            <span>{t(language, "hardware.controller")}</span>
            <select value={hardware.controller} onChange={(event) => onChange({ ...hardware, controller: event.target.value })}>
              <option value="esp32s3">ESP32-S3</option>
            </select>
          </label>
          <label className="safety-check">
            <input type="checkbox" required />
            <span>{t(language, "hardware.safety")}</span>
          </label>
          <fieldset>
            <legend>{t(language, "hardware.candidatePins")}</legend>
            <div className="pin-grid">
              {supportedGpios.map((gpio) => (
                <label key={gpio}><input type="checkbox" name="learning-pin" value={gpio} />{gpio}</label>
              ))}
            </div>
          </fieldset>
          {learning ? (
            <button type="button" className="learning-button" onClick={onEndLearning}>
              <SquareStop size={16} />{t(language, "hardware.stopLearning")}
            </button>
          ) : (
            <button
              type="button"
              className="learning-button"
              onClick={(event) => {
                const container = event.currentTarget.closest(".learning-controls");
                const safe = container?.querySelector<HTMLInputElement>(".safety-check input")?.checked;
                const pins = [...(container?.querySelectorAll<HTMLInputElement>('input[name="learning-pin"]:checked') ?? [])]
                  .map((input) => Number(input.value));
                if (safe && pins.length) onBeginLearning(pins);
              }}
            >
              <Radio size={16} />{t(language, "hardware.startLearning")}
            </button>
          )}
        </div>
      </details>
    </section>
  );
}
