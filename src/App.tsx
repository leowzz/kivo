import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Pencil, RotateCcw, Save, Trash2, Unplug, Usb } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Keypad } from "./Keypad";
import { LayoutEditor } from "./LayoutEditor";
import type {
  AppSnapshot,
  ButtonAction,
  ConfigMode,
  ConnectionStatus,
  ModelLayout,
  RuntimeEvent,
} from "./types";

const SEARCHING: ConnectionStatus = { state: "searching", port: null };

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
  const [models, setModels] = useState<ModelLayout[]>([]);
  const [savedModels, setSavedModels] = useState<ModelLayout[]>([]);
  const [activeModel, setActiveModel] = useState("");
  const [savedActiveModel, setSavedActiveModel] = useState("");
  const [ioMaps, setIoMaps] = useState<Record<string, Record<number, string>>>({});
  const [savedIoMaps, setSavedIoMaps] = useState<Record<string, Record<number, string>>>({});
  const [supportedGpios, setSupportedGpios] = useState<number[]>([]);
  const [actions, setActions] = useState<Record<string, ButtonAction>>({});
  const [savedActions, setSavedActions] = useState<Record<string, ButtonAction>>({});
  const [mode, setMode] = useState<ConfigMode>("io");
  const [selectedButtonId, setSelectedButtonId] = useState<string | null>(null);
  const [selectedAnchor, setSelectedAnchor] = useState<DOMRect | null>(null);
  const [capturedGpio, setCapturedGpio] = useState<number | null>(null);
  const [pressedGpios, setPressedGpios] = useState<Set<number>>(() => new Set());
  const capturingButtonRef = useRef<string | null>(null);
  const captureGenerationRef = useRef(0);
  const ioCaptureQueueRef = useRef<Promise<void>>(Promise.resolve());
  const [configPath, setConfigPath] = useState("");
  const [connection, setConnection] = useState<ConnectionStatus>(SEARCHING);
  const [events, setEvents] = useState<RuntimeEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [layoutEditorOpen, setLayoutEditorOpen] = useState(false);

  const applySnapshot = useCallback((snapshot: AppSnapshot) => {
    setModels(snapshot.models);
    setSavedModels(snapshot.models);
    setActiveModel(snapshot.activeModel);
    setSavedActiveModel(snapshot.activeModel);
    setIoMaps(snapshot.ioMaps);
    setSavedIoMaps(snapshot.ioMaps);
    setSupportedGpios(snapshot.supportedGpios);
    setActions(snapshot.actions);
    setSavedActions(snapshot.actions);
    setConfigPath(snapshot.configPath);
    setConnection(snapshot.connection);
    setError(snapshot.configError);
  }, []);

  useEffect(() => {
    let active = true;
    let stopListening: (() => void) | undefined;
    const initialize = async () => {
      try {
        stopListening = await listen<RuntimeEvent>("runtime-event", ({ payload }) => {
          if (!active) return;
          setConnection(payload.connection);
          setEvents((current) => [...current, payload].slice(-200));
          if (payload.connection.state !== "connected") {
            setPressedGpios(new Set());
          } else if (payload.gpio !== null && payload.pressed !== null) {
            const gpio = payload.gpio;
            const pressed = payload.pressed;
            setPressedGpios((current) => {
              const next = new Set(current);
              if (pressed) next.add(gpio);
              else next.delete(gpio);
              return next;
            });
          }
          if (capturingButtonRef.current && payload.gpio !== null && payload.pressed) {
            capturingButtonRef.current = null;
            setCapturedGpio(payload.gpio);
          }
        });
        if (!active) {
          stopListening();
          return;
        }
        const snapshot = await invoke<AppSnapshot>("get_snapshot");
        if (!active) return;
        applySnapshot(snapshot);
      } catch (loadError) {
        if (!active) return;
        setError(errorMessage(loadError));
      } finally {
        if (active) setLoaded(true);
      }
    };
    void initialize();

    return () => {
      active = false;
      stopListening?.();
    };
  }, [applySnapshot]);

  const enqueueIoCapture = useCallback((enabled: boolean) => {
    const transition = ioCaptureQueueRef.current.then(async () => {
      await invoke("set_io_capture", { enabled });
    });
    ioCaptureQueueRef.current = transition.catch(() => undefined);
    return transition;
  }, []);

  useEffect(() => {
    if (mode !== "io" || !selectedButtonId || connection.state !== "connected") return;
    const generation = ++captureGenerationRef.current;
    capturingButtonRef.current = null;
    void enqueueIoCapture(true)
      .then(() => {
        if (captureGenerationRef.current === generation) {
          capturingButtonRef.current = selectedButtonId;
        }
      })
      .catch((captureError) => {
        if (captureGenerationRef.current === generation) {
          capturingButtonRef.current = null;
          setError(errorMessage(captureError));
        }
      });
    return () => {
      captureGenerationRef.current += 1;
      capturingButtonRef.current = null;
      void enqueueIoCapture(false).catch(() => undefined);
    };
  }, [connection.state, enqueueIoCapture, mode, selectedButtonId]);

  const dirty = useMemo(
    () => JSON.stringify([models, activeModel, ioMaps, actions])
      !== JSON.stringify([savedModels, savedActiveModel, savedIoMaps, savedActions]),
    [actions, activeModel, ioMaps, models, savedActions, savedActiveModel, savedIoMaps, savedModels],
  );

  const saveWorkspace = useCallback(async () => {
    if (!loaded || !dirty || saving || layoutEditorOpen) return;
    setSaving(true);
    setError(null);
    try {
      const snapshot = await invoke<AppSnapshot>("save_workspace", {
        activeModel,
        ioMaps,
        actions,
        models,
      });
      applySnapshot(snapshot);
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
          gpio: null,
          pressed: null,
        },
      ].slice(-200));
    } finally {
      setSaving(false);
    }
  }, [
    actions,
    activeModel,
    applySnapshot,
    connection,
    dirty,
    ioMaps,
    layoutEditorOpen,
    loaded,
    models,
    saving,
  ]);

  useEffect(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      if (event.metaKey && event.key.toLowerCase() === "s") {
        event.preventDefault();
        void saveWorkspace();
      }
    };
    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, [saveWorkspace]);

  useEffect(() => {
    setPressedGpios(new Set());
  }, [activeModel]);

  const activeLayout = models.find((model) => model.id === activeModel);
  const pressedButtonIds = useMemo(() => new Set(
    Object.entries(ioMaps[activeModel] ?? {})
      .filter(([gpio]) => pressedGpios.has(Number(gpio)))
      .map(([, buttonId]) => buttonId),
  ), [activeModel, ioMaps, pressedGpios]);
  const missingActiveModel = loaded && !activeLayout;
  const connected = connection.state === "connected";

  const selectModel = (modelId: string) => {
    setActiveModel(modelId);
    setSelectedButtonId(null);
    setSelectedAnchor(null);
    setCapturedGpio(null);
  };

  const closePopover = () => {
    setSelectedButtonId(null);
    setSelectedAnchor(null);
    setCapturedGpio(null);
  };

  const revertWorkspace = () => {
    setModels(savedModels);
    setActiveModel(savedActiveModel);
    setIoMaps(savedIoMaps);
    setActions(savedActions);
    setLayoutEditorOpen(false);
    closePopover();
  };

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
        <div className="save-controls">
          <button
            className="icon-button"
            type="button"
            aria-label="Revert workspace"
            title="Revert workspace"
            disabled={!loaded || !dirty || saving || layoutEditorOpen}
            onClick={revertWorkspace}
          >
            <RotateCcw size={17} />
          </button>
          <button
            className="save-button"
            type="button"
            aria-label="Save workspace"
            disabled={!loaded || !dirty || saving || layoutEditorOpen}
            onClick={() => void saveWorkspace()}
          >
            <Save size={17} />
            {saving ? "Saving" : "Save"}
          </button>
        </div>
      </header>

      {error && <div className="error-banner" role="alert">{error}</div>}

      <div className="workspace">
        <section className="mapping-section" aria-labelledby="keypad-heading">
          <div className="section-heading workspace-heading">
            <div>
              <p className="eyebrow">Configuration</p>
              <h2 id="keypad-heading">Keypad</h2>
            </div>
            <div className="workspace-controls">
              <button
                className="icon-button"
                type="button"
                aria-label="Edit layout"
                title="Edit layout"
                disabled={!loaded || saving || !activeLayout}
                onClick={() => {
                  closePopover();
                  setLayoutEditorOpen(true);
                }}
              >
                <Pencil size={16} />
              </button>
              <label className="model-picker">
                <span>Model</span>
                <select
                  aria-label="Device model"
                  value={activeModel}
                  disabled={!loaded || saving || dirty}
                  onChange={(event) => selectModel(event.target.value)}
                >
                  {missingActiveModel && (
                    <option value={activeModel} disabled>Missing: {activeModel}</option>
                  )}
                  {models.map((model) => (
                    <option value={model.id} key={model.id}>{model.name}</option>
                  ))}
                </select>
              </label>
              <div className="mode-switch" role="group" aria-label="Configuration mode">
                {(["io", "behavior"] as const).map((value) => (
                  <button
                    className={mode === value ? "is-active" : ""}
                    type="button"
                    aria-label={value === "io" ? "IO" : "Behavior"}
                    aria-pressed={mode === value}
                    key={value}
                    onClick={() => {
                      setMode(value);
                      if (value !== mode) closePopover();
                    }}
                  >
                    {value === "io" ? "IO" : "Behavior"}
                  </button>
                ))}
              </div>
            </div>
          </div>

          <div className="keypad-workspace">
            {activeLayout && (
              <Keypad
                layout={activeLayout}
                mode={mode}
                ioMap={ioMaps[activeModel] ?? {}}
                actions={actions}
                supportedGpios={supportedGpios}
                pressedButtonIds={pressedButtonIds}
                selectedButtonId={selectedButtonId}
                selectedAnchor={selectedAnchor}
                capturedGpio={capturedGpio}
                onSelect={(buttonId, anchor) => {
                  setCapturedGpio(null);
                  setSelectedButtonId(buttonId);
                  setSelectedAnchor(anchor);
                }}
                onApplyIoMap={(ioMap) => {
                  setIoMaps((current) => ({ ...current, [activeModel]: ioMap }));
                  closePopover();
                }}
                onApplyAction={(buttonId, action) => {
                  setActions((current) => ({ ...current, [buttonId]: action }));
                  closePopover();
                }}
                onDeleteAction={(buttonId) => {
                  setActions((current) => Object.fromEntries(
                    Object.entries(current).filter(([id]) => id !== buttonId),
                  ));
                  closePopover();
                }}
                onCancel={closePopover}
              />
            )}
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
      <LayoutEditor
        layout={activeLayout ?? null}
        open={layoutEditorOpen}
        onCancel={() => setLayoutEditorOpen(false)}
        onApply={(layout) => {
          setModels((current) => current.map((model) => model.id === layout.id ? layout : model));
          const buttonIds = new Set(
            layout.groups.flatMap((group) => group.buttons.map((button) => button.id)),
          );
          setIoMaps((current) => {
            const activeIoMap = current[layout.id];
            if (!activeIoMap) return current;
            return {
              ...current,
              [layout.id]: Object.fromEntries(
                Object.entries(activeIoMap).filter(([, buttonId]) => buttonIds.has(buttonId)),
              ),
            };
          });
          setLayoutEditorOpen(false);
          closePopover();
        }}
      />
    </main>
  );
}
