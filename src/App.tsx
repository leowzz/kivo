import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  Save,
  Trash2,
  Unplug,
  Usb,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { AppSnapshot, ConnectionStatus, RuntimeEvent } from "./types";

const SUPPORTED_GPIOS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18];
const SEARCHING: ConnectionStatus = { state: "searching", port: null };

function normalizeButtons(buttons: Record<number, string>) {
  return Object.fromEntries(
    SUPPORTED_GPIOS.map((gpio) => [gpio, buttons[gpio] ?? ""]),
  ) as Record<number, string>;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function eventTime(timestampMs: number) {
  return new Date(timestampMs).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export default function App() {
  const [buttons, setButtons] = useState<Record<number, string>>(() => normalizeButtons({}));
  const [savedButtons, setSavedButtons] = useState<Record<number, string>>(() => normalizeButtons({}));
  const [selectedGpio, setSelectedGpio] = useState(0);
  const [configPath, setConfigPath] = useState("");
  const [connection, setConnection] = useState<ConnectionStatus>(SEARCHING);
  const [events, setEvents] = useState<RuntimeEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let active = true;
    invoke<AppSnapshot>("get_snapshot")
      .then((snapshot) => {
        if (!active) return;
        const loadedButtons = normalizeButtons(snapshot.buttons);
        setButtons(loadedButtons);
        setSavedButtons(loadedButtons);
        setConfigPath(snapshot.configPath);
        setConnection(snapshot.connection);
        setError(snapshot.configError);
        setLoaded(true);
      })
      .catch((loadError) => {
        if (!active) return;
        setError(errorMessage(loadError));
        setLoaded(true);
      });

    const unlisten = listen<RuntimeEvent>("runtime-event", ({ payload }) => {
      if (!active) return;
      setConnection(payload.connection);
      setEvents((current) => [...current, payload].slice(-200));
    });
    return () => {
      active = false;
      void unlisten.then((stopListening) => stopListening());
    };
  }, []);

  const dirty = useMemo(
    () =>
      SUPPORTED_GPIOS.some(
        (gpio) => buttons[gpio] !== savedButtons[gpio],
      ),
    [buttons, savedButtons],
  );

  const saveMappings = useCallback(async () => {
    if (!loaded || !dirty || saving) return;
    setSaving(true);
    setError(null);
    const submitted = { ...buttons };
    try {
      const snapshot = await invoke<AppSnapshot>("save_mappings", {
        buttons: submitted,
      });
      setSavedButtons(normalizeButtons(snapshot.buttons));
      setConfigPath(snapshot.configPath);
      setConnection(snapshot.connection);
      setError(snapshot.configError);
    } catch (saveError) {
      const message = errorMessage(saveError);
      setError(message);
      setEvents((current) => [
        ...current,
        {
          timestampMs: Date.now(),
          level: "error" as const,
          message: `Save failed: ${message}`,
          connection,
        },
      ].slice(-200));
    } finally {
      setSaving(false);
    }
  }, [buttons, connection, dirty, loaded, saving]);

  useEffect(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      if (event.metaKey && event.key.toLowerCase() === "s") {
        event.preventDefault();
        void saveMappings();
      }
    };
    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, [saveMappings]);

  const connected = connection.state === "connected";
  const selectedText = buttons[selectedGpio] ?? "";

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark"><Usb size={20} strokeWidth={2} /></span>
          <h1>Vibe Tool</h1>
        </div>
        <div className={`connection ${connected ? "is-connected" : ""}`}>
          {connected ? <Usb size={16} /> : <Unplug size={16} />}
          <span>{connected ? "Connected" : "Waiting"}</span>
          {connection.port && <code>{connection.port}</code>}
        </div>
        <div className="config-location" title={configPath}>
          {configPath || "Loading configuration..."}
        </div>
        <button
          className="save-button"
          type="button"
          aria-label="Save mappings"
          disabled={!loaded || !dirty || saving}
          onClick={() => void saveMappings()}
        >
          <Save size={17} />
          {saving ? "Saving" : "Save"}
        </button>
      </header>

      {error && <div className="error-banner" role="alert">{error}</div>}

      <div className="workspace">
        <section className="mapping-section" aria-labelledby="mappings-heading">
          <div className="section-heading">
            <div>
              <p className="eyebrow">Configuration</p>
              <h2 id="mappings-heading">GPIO mappings</h2>
            </div>
            <span className="mapping-count">{SUPPORTED_GPIOS.length} pins</span>
          </div>

          <div className="mapping-workspace">
            <div className="mapping-list" role="listbox" aria-label="GPIO mappings">
              {SUPPORTED_GPIOS.map((gpio) => {
                const text = buttons[gpio];
                return (
                  <button
                    key={gpio}
                    className={`mapping-row ${selectedGpio === gpio ? "is-selected" : ""}`}
                    type="button"
                    role="option"
                    aria-selected={selectedGpio === gpio}
                    onClick={() => setSelectedGpio(gpio)}
                  >
                    <span className="gpio-label">GPIO{gpio}</span>
                    <span className={`mapping-preview ${text ? "" : "is-empty"}`}>
                      {text ? text.replaceAll("\n", " ↵ ") : "No text"}
                    </span>
                  </button>
                );
              })}
            </div>

            <div className="mapping-editor">
              <div className="editor-heading">
                <div>
                  <p className="eyebrow">Selected pin</p>
                  <h3>GPIO{selectedGpio}</h3>
                </div>
                {selectedGpio === 0 && (
                  <div className="gpio-warning">
                    <AlertTriangle size={16} />
                    <span>GPIO0 enters download mode when held during startup.</span>
                  </div>
                )}
              </div>
              <label className="editor-label" htmlFor="mapping-text">Pasted text</label>
              <textarea
                id="mapping-text"
                aria-label={`GPIO${selectedGpio} mapping`}
                value={selectedText}
                onChange={(event) =>
                  setButtons((current) => ({
                    ...current,
                    [selectedGpio]: event.target.value,
                  }))
                }
                placeholder="No text assigned"
                spellCheck={false}
              />
              <div className="editor-meta">
                <span>{selectedText.length} characters</span>
                {dirty && <span className="unsaved-indicator">Unsaved changes</span>}
              </div>
            </div>
          </div>
        </section>

        <section className="activity-section" aria-labelledby="activity-heading">
          <div className="section-heading">
            <div>
              <p className="eyebrow">Runtime</p>
              <h2 id="activity-heading">Activity</h2>
            </div>
            <button
              className="icon-button"
              type="button"
              aria-label="Clear activity"
              title="Clear activity"
              disabled={events.length === 0}
              onClick={() => setEvents([])}
            >
              <Trash2 size={17} />
            </button>
          </div>
          <div className="event-log" role="log" aria-live="polite">
            {events.length === 0 ? (
              <div className="empty-log">
                <Unplug size={22} />
                <span>No button activity yet</span>
              </div>
            ) : (
              events.map((event, index) => (
                <div className={`event-row is-${event.level}`} key={`${event.timestampMs}-${index}`}>
                  <time dateTime={new Date(event.timestampMs).toISOString()}>
                    {eventTime(event.timestampMs)}
                  </time>
                  <span className="event-level" aria-hidden="true" />
                  <span>{event.message}</span>
                </div>
              ))
            )}
          </div>
        </section>
      </div>
    </main>
  );
}
