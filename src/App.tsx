import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save as saveFile } from "@tauri-apps/plugin-dialog";
import {
  ArchiveRestore,
  Cable,
  DatabaseBackup,
  Download,
  FileInput,
  Home,
  Keyboard,
  LayoutGrid,
  Trash2,
  Unplug,
  Upload,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import brandIcon from "../src-tauri/icons/128x128.png";
import { ActionEditor } from "./ActionEditor";
import { ConfirmDialog } from "./ConfirmDialog";
import { HardwareMapping } from "./HardwareMapping";
import { HomeDashboard } from "./HomeDashboard";
import { Keypad } from "./Keypad";
import { LayoutEditor } from "./LayoutEditor";
import { t } from "./i18n";
import type {
  AppSnapshot,
  BackupPreview,
  ButtonAction,
  ImportPreview,
  InputSource,
  Language,
  ModelConfig,
  PhysicalInput,
  RuntimeEvent,
} from "./types";
import { SerializedSaveQueue, useAutosave } from "./useAutosave";

type View = "home" | "behavior" | "hardware" | "layout" | "data";
type Confirmation =
  | { kind: "import"; path: string; preview: ImportPreview }
  | { kind: "restore"; path: string; preview: BackupPreview }
  | { kind: "delete"; model: ModelConfig };

const PREVIEW_MODE = import.meta.env.DEV && new URLSearchParams(window.location.search).has("preview");

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error && "code" in error) return String(error.code);
  return String(error);
}

function allButtons(model: ModelConfig | undefined) {
  return model?.model.groups.flatMap((group) => group.buttons) ?? [];
}

function isValidDraft(model: ModelConfig | undefined) {
  return Boolean(model && Object.values(model.actions).every((actions) => actions.every((action) =>
    action.type === "paste" ? action.text.length > 0 : action.keys.length > 0
  )));
}

function resolveButton(model: ModelConfig, input: PhysicalInput) {
  let runtimeSource = 0;
  for (const source of model.hardware.inputs) {
    if (Object.keys(source.keys).length === 0) continue;
    if (source.type === "direct" && input.type === "direct") {
      const match = Object.entries(source.keys).find(([, gpio]) => gpio === input.gpio);
      if (match) return match[0];
    }
    if (source.type === "contact_matrix" && input.type === "contact" && input.source === runtimeSource) {
      const pair = [Math.min(input.pin_a, input.pin_b), Math.max(input.pin_a, input.pin_b)];
      const match = Object.entries(source.keys).find(([, pins]) => pins[0] === pair[0] && pins[1] === pair[1]);
      if (match) return match[0];
    }
    runtimeSource += 1;
  }
  return null;
}

function learnInput(model: ModelConfig, buttonId: string, input: PhysicalInput): ModelConfig {
  const inputs = model.hardware.inputs.map((source): InputSource => ({
    ...source,
    keys: Object.fromEntries(Object.entries(source.keys).filter(([id]) => id !== buttonId)),
  })) as InputSource[];

  if (input.type === "direct") {
    let index = inputs.findIndex((source) => source.type === "direct");
    if (index < 0) {
      inputs.push({ type: "direct", id: "direct", keys: {} });
      index = inputs.length - 1;
    }
    const source = inputs[index];
    if (source.type === "direct") {
      source.keys = Object.fromEntries(Object.entries(source.keys).filter(([, gpio]) => gpio !== input.gpio));
      source.keys[buttonId] = input.gpio;
    }
  } else {
    let index = inputs.findIndex((source) => source.type === "contact_matrix");
    if (index < 0) {
      inputs.push({ type: "contact_matrix", id: "matrix", pins: [], keys: {} });
      index = inputs.length - 1;
    }
    const source = inputs[index];
    if (source.type === "contact_matrix") {
      const pair: [number, number] = [Math.min(input.pin_a, input.pin_b), Math.max(input.pin_a, input.pin_b)];
      source.pins = [...new Set([...source.pins, ...pair])].sort((left, right) => left - right);
      source.keys = Object.fromEntries(Object.entries(source.keys).filter(([, pins]) =>
        pins[0] !== pair[0] || pins[1] !== pair[1]
      ));
      source.keys[buttonId] = pair;
    }
  }

  return { ...model, hardware: { ...model.hardware, inputs } };
}

export default function App() {
  const queue = useRef(new SerializedSaveQueue()).current;
  const [models, setModels] = useState<ModelConfig[]>([]);
  const [savedModels, setSavedModels] = useState<Record<string, string>>({});
  const [activeModel, setActiveModel] = useState<string | null>(null);
  const [language, setLanguage] = useState<Language>("zh-CN");
  const [supportedGpios, setSupportedGpios] = useState<number[]>([]);
  const [connection, setConnection] = useState<AppSnapshot["connection"]>({ state: "searching", port: null });
  const [learning, setLearning] = useState<AppSnapshot["learning"]>(null);
  const [runtimeError, setRuntimeError] = useState<AppSnapshot["runtimeError"]>(null);
  const [view, setView] = useState<View>("home");
  const [homeMetrics, setHomeMetrics] = useState<AppSnapshot["homeMetrics"]>(null);
  const [selectedButtonId, setSelectedButtonId] = useState<string | null>(null);
  const [pressedButtonIds, setPressedButtonIds] = useState<Set<string>>(() => new Set());
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [layoutEditorOpen, setLayoutEditorOpen] = useState(false);

  const activeConfig = useMemo(
    () => models.find((model) => model.model.id === activeModel),
    [activeModel, models],
  );
  const dirty = Boolean(activeConfig && savedModels[activeConfig.model.id] !== JSON.stringify(activeConfig));

  const applySnapshot = useCallback((snapshot: AppSnapshot) => {
    setModels(snapshot.models);
    setSavedModels(Object.fromEntries(snapshot.models.map((model) => [model.model.id, JSON.stringify(model)])));
    setActiveModel(snapshot.activeModel);
    setLanguage("zh-CN");
    setSupportedGpios(snapshot.supportedGpios);
    setConnection(snapshot.connection);
    setRuntimeError(snapshot.runtimeError);
    setLearning(snapshot.learning);
    setHomeMetrics(snapshot.homeMetrics);
    setPressedButtonIds(new Set());
  }, []);

  const saveActiveModel = useCallback(async (model: ModelConfig | undefined) => {
    if (!model) return;
    if (!PREVIEW_MODE) await invoke("save_model", { model });
    setSavedModels((current) => ({ ...current, [model.model.id]: JSON.stringify(model) }));
  }, []);

  const autosave = useAutosave({
    value: activeConfig,
    valid: dirty && isValidDraft(activeConfig),
    save: saveActiveModel,
    queue,
  });

  const modelRef = useRef(activeConfig);
  const selectedRef = useRef(selectedButtonId);
  const learningRef = useRef(learning);
  const viewRef = useRef(view);
  modelRef.current = activeConfig;
  selectedRef.current = selectedButtonId;
  learningRef.current = learning;
  viewRef.current = view;

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        if (PREVIEW_MODE) {
          applySnapshot((await import("./preview")).previewSnapshot);
          return;
        }
        unlisten = await listen<RuntimeEvent>("runtime-event", ({ payload }) => {
          if (!active) return;
          setConnection(payload.connection);
          if (payload.homeUpdate) setHomeMetrics(payload.homeUpdate);
          if (payload.input && payload.pressed !== null) {
            const currentModel = modelRef.current;
            if (payload.code === "learning_input" && payload.pressed && learningRef.current
              && selectedRef.current && viewRef.current === "hardware" && currentModel) {
              const learned = learnInput(currentModel, selectedRef.current, payload.input);
              setModels((current) => current.map((model) => model.model.id === learned.model.id ? learned : model));
            }
            if (currentModel) {
              const buttonId = resolveButton(currentModel, payload.input);
              if (buttonId) {
                setPressedButtonIds((current) => {
                  const next = new Set(current);
                  if (payload.pressed) next.add(buttonId);
                  else next.delete(buttonId);
                  return next;
                });
              }
            }
          }
        });
        const snapshot = await invoke<AppSnapshot>("get_snapshot");
        if (active) applySnapshot(snapshot);
      } catch (loadError) {
        if (active) setError(`${t("zh-CN", "error.load")}: ${errorMessage(loadError)}`);
      } finally {
        if (active) setLoaded(true);
      }
    })();
    return () => {
      active = false;
      unlisten?.();
    };
  }, [applySnapshot]);

  useEffect(() => {
    const buttons = allButtons(activeConfig);
    if (!buttons.some((button) => button.id === selectedButtonId)) {
      setSelectedButtonId(buttons[0]?.id ?? null);
    }
  }, [activeConfig, selectedButtonId]);

  const updateActive = (update: (model: ModelConfig) => ModelConfig) => {
    if (!activeConfig) return;
    setModels((current) => current.map((model) => model.model.id === activeConfig.model.id ? update(model) : model));
  };

  const saveSettings = async (nextActive: string | null, nextLanguage: Language) => {
    await autosave.flush();
    if (PREVIEW_MODE) {
      setActiveModel(nextActive);
      setLanguage(nextLanguage);
      return;
    }
    const snapshot = await queue.enqueue(() => invoke<AppSnapshot>("save_settings", {
      settings: { schema_version: 1, active_model: nextActive, language: nextLanguage },
    }));
    applySnapshot(snapshot);
  };

  const run = async (label: string, task: () => Promise<void>) => {
    setError(null);
    try {
      await task();
    } catch (operationError) {
      setError(`${label}: ${errorMessage(operationError)}`);
    }
  };

  const chooseImport = () => run(t(language, "error.import"), async () => {
    await autosave.flush();
    const path = await open({ multiple: false, filters: [{ name: "Kivo", extensions: ["yaml", "yml"] }] });
    if (!path) return;
    const preview = await invoke<ImportPreview>("preview_model_import", { path });
    setConfirmation({ kind: "import", path, preview });
  });

  const chooseRestore = () => run(t(language, "error.restore"), async () => {
    await autosave.flush();
    const path = await open({ multiple: false, filters: [{ name: "Kivo", extensions: ["yaml", "yml"] }] });
    if (!path) return;
    const preview = await invoke<BackupPreview>("preview_backup", { path });
    setConfirmation({ kind: "restore", path, preview });
  });

  const confirmOperation = () => {
    const current = confirmation;
    if (!current) return;
    setConfirmation(null);
    void run(
      t(language, current.kind === "restore" ? "error.restore" : current.kind === "delete" ? "error.delete" : "error.import"),
      async () => {
        const snapshot = current.kind === "import"
          ? await invoke<AppSnapshot>("import_model", { path: current.path })
          : current.kind === "restore"
            ? await invoke<AppSnapshot>("restore_backup", { path: current.path })
            : await invoke<AppSnapshot>("delete_model", { id: current.model.model.id });
        applySnapshot(snapshot);
      },
    );
  };

  const selectedButton = allButtons(activeConfig).find((button) => button.id === selectedButtonId) ?? null;
  const selectedActions = activeConfig && selectedButtonId ? activeConfig.actions[selectedButtonId] ?? [] : [];
  const connected = connection.state === "connected";

  return (
    <main className="product-shell">
      <header className="topbar">
        <div className="brand"><img src={brandIcon} alt="" /><h1>Kivo</h1></div>
        <div className={connected ? "connection is-connected" : "connection"}>
          <span className="status-dot" aria-hidden="true" />
          {connected ? <Cable size={14} /> : <Unplug size={14} />}
          <span>{t(language, connected ? "connection.connected" : "connection.searching")}</span>
          {connection.port && <code>{connection.port}</code>}
        </div>
        <div className={`save-state is-${autosave.status}`} aria-live="polite">
          {autosave.status === "saving" && t(language, "save.saving")}
          {autosave.status === "error" && (
            <><span>{t(language, "save.failed")}</span><button type="button" onClick={() => void autosave.retry()}>{t(language, "save.retry")}</button></>
          )}
        </div>
      </header>

      {(error || runtimeError) && (
        <div className="error-toast" role="alert">
          <span>{error ?? runtimeError?.detail ?? runtimeError?.code}</span>
          <button className="icon-button" type="button" aria-label={t(language, "common.close")} onClick={() => {
            setError(null);
            setRuntimeError(null);
          }}><X size={15} /></button>
        </div>
      )}

      <div className={view === "home" || view === "data" ? "product-workspace is-home" : "product-workspace"}>
        <aside className="sidebar">
          <button className={`home-nav-button ${view === "home" ? "is-active" : ""}`} type="button" onClick={() => setView("home")}>
            <Home size={17} />{t(language, "nav.home")}
          </button>

          <nav aria-label={t(language, "nav.configuration")}>
            <span>{t(language, "nav.configuration")}</span>
            <button className={view === "behavior" ? "is-active" : ""} type="button" onClick={() => setView("behavior")}>
              <Keyboard size={17} />{t(language, "nav.behavior")}
            </button>
            <button className={view === "hardware" ? "is-active" : ""} type="button" disabled={!activeConfig} onClick={() => setView("hardware")}>
              <Cable size={17} />{t(language, "nav.hardware")}
            </button>
            <button className={view === "layout" ? "is-active" : ""} type="button" disabled={!activeConfig} onClick={() => setView("layout")}>
              <LayoutGrid size={17} />{t(language, "nav.layout")}
            </button>
          </nav>

          <button className={`data-nav-button ${view === "data" ? "is-active" : ""}`} type="button" onClick={() => setView("data")}>
            <FileInput size={17} />{t(language, "nav.data")}
          </button>

        </aside>

        <section className="content-panel">
          {view === "data" ? (
            <div className="data-page">
              <div className="content-heading"><div><h2>{t(language, "nav.data")}</h2></div></div>
              <div className="data-page-body">
                <label className="model-picker"><span>{t(language, "model.select")}</span><select
                  aria-label={t(language, "model.select")}
                  value={activeModel ?? ""}
                  disabled={!loaded || models.length === 0}
                  onChange={(event) => void run(t(language, "error.save"), () => saveSettings(event.target.value, language))}
                >
                  {models.length === 0 && <option value="">{t(language, "model.empty")}</option>}
                  {models.map((model) => <option value={model.model.id} key={model.model.id}>{model.model.name}</option>)}
                </select></label>
                <div className="data-menu">
            <button type="button" onClick={() => void chooseImport()}><FileInput size={16} />{t(language, "nav.import")}</button>
            <button type="button" disabled={!activeConfig} onClick={() => void run(t(language, "error.export"), async () => {
              await autosave.flush();
              const path = await saveFile({ defaultPath: `${activeConfig?.model.id ?? "model"}.yaml`, filters: [{ name: "Kivo", extensions: ["yaml"] }] });
              if (path && activeConfig) await invoke("export_model", { id: activeConfig.model.id, path });
            })}><Upload size={16} />{t(language, "nav.export")}</button>
            <button type="button" disabled={models.length === 0} onClick={() => void run(t(language, "error.export"), async () => {
              await autosave.flush();
              const path = await saveFile({ defaultPath: "kivo-backup.yaml", filters: [{ name: "Kivo", extensions: ["yaml"] }] });
              if (path) await invoke("export_backup", { path });
            })}><DatabaseBackup size={16} />{t(language, "nav.backup")}</button>
            <button type="button" onClick={() => void chooseRestore()}><ArchiveRestore size={16} />{t(language, "nav.restore")}</button>
            <button className="is-danger" type="button" disabled={!activeConfig} onClick={() => activeConfig && setConfirmation({ kind: "delete", model: activeConfig })}>
              <Trash2 size={16} />{t(language, "nav.delete")}
            </button>
                </div>
              </div>
            </div>
          ) : view === "home" ? (
            <HomeDashboard
              connection={connection}
              language={language}
              metrics={homeMetrics}
              model={activeConfig}
            />
          ) : !activeConfig ? (
            <div className="empty-workspace">
              <Download size={28} />
              <h2>{t(language, "model.empty")}</h2>
              <div><button className="primary-button" type="button" onClick={() => void chooseImport()}>{t(language, "model.import")}</button><button type="button" onClick={() => void chooseRestore()}>{t(language, "model.restore")}</button></div>
            </div>
          ) : view === "hardware" ? (
            <HardwareMapping
              language={language}
              layout={activeConfig.model}
              hardware={activeConfig.hardware}
              supportedGpios={supportedGpios}
              learning={learning}
              selectedButtonId={selectedButtonId}
              onSelectButton={setSelectedButtonId}
              onChange={(hardware) => updateActive((model) => ({ ...model, hardware }))}
              onBeginLearning={(pins) => void run(t(language, "error.learning"), async () => applySnapshot(await invoke("begin_learning", { pins })))}
              onEndLearning={() => void run(t(language, "error.learning"), async () => applySnapshot(await invoke("end_learning")))}
            />
          ) : (
            <>
              <div className="content-heading">
                <div><span>{activeConfig.model.name}</span><h2>{t(language, view === "layout" ? "layout.title" : "behavior.title")}</h2></div>
                {view === "layout" && <button className="primary-button" type="button" onClick={() => setLayoutEditorOpen(true)}><LayoutGrid size={16} />{t(language, "layout.edit")}</button>}
              </div>
              <div className="keypad-stage">
                <Keypad
                  layout={activeConfig.model}
                  actions={activeConfig.actions}
                  selectedButtonId={selectedButtonId}
                  pressedButtonIds={pressedButtonIds}
                  actionCountLabel={(count) => t(language, "model.actionCount", { count })}
                  onSelect={setSelectedButtonId}
                />
              </div>
            </>
          )}
        </section>

        {view !== "home" && view !== "data" && <ActionEditor
          language={language}
          button={selectedButton}
          actions={selectedActions}
          onChange={(actions: ButtonAction[]) => selectedButtonId && updateActive((model) => ({
            ...model,
            actions: { ...model.actions, [selectedButtonId]: actions },
          }))}
        />}
      </div>

      <LayoutEditor
        layout={activeConfig?.model ?? null}
        language={language}
        open={layoutEditorOpen}
        onCancel={() => setLayoutEditorOpen(false)}
        onApply={(layout) => {
          updateActive((model) => {
            const buttonIds = new Set(layout.groups.flatMap((group) => group.buttons.map((button) => button.id)));
            const actions = Object.fromEntries(Object.entries(model.actions).filter(([id]) => buttonIds.has(id)));
            const inputs = model.hardware.inputs.map((source) => ({
              ...source,
              keys: Object.fromEntries(Object.entries(source.keys).filter(([id]) => buttonIds.has(id))),
            })) as InputSource[];
            return { ...model, model: layout, actions, hardware: { ...model.hardware, inputs } };
          });
          setLayoutEditorOpen(false);
        }}
      />

      {confirmation && (
        <ConfirmDialog
          title={confirmation.kind === "restore"
            ? t(language, "dialog.restoreTitle")
            : confirmation.kind === "delete"
              ? t(language, "dialog.deleteTitle")
              : t(language, confirmation.preview.replacesExisting ? "dialog.replaceTitle" : "dialog.importTitle")}
          body={confirmation.kind === "restore"
            ? t(language, "dialog.restoreBody")
            : confirmation.kind === "delete"
              ? t(language, "dialog.deleteBody", { name: confirmation.model.model.name })
              : t(language, confirmation.preview.replacesExisting ? "dialog.replaceBody" : "dialog.importBody")}
          summary={confirmation.kind === "restore"
            ? t(language, "dialog.backupSummary", {
              models: confirmation.preview.modelCount,
              buttons: confirmation.preview.buttonCount,
              bindings: confirmation.preview.hardwareBindingCount,
              actions: confirmation.preview.actionCount,
            })
            : confirmation.kind === "import"
              ? t(language, "dialog.modelSummary", {
                buttons: confirmation.preview.buttonCount,
                bindings: confirmation.preview.hardwareBindingCount,
                actions: confirmation.preview.actionCount,
              })
              : confirmation.model.model.name}
          confirmLabel={t(language, "common.confirm")}
          cancelLabel={t(language, "common.cancel")}
          danger={confirmation.kind !== "import" || confirmation.preview.replacesExisting}
          onCancel={() => setConfirmation(null)}
          onConfirm={confirmOperation}
        />
      )}
    </main>
  );
}
