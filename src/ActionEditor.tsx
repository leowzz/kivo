import {
  ArrowDown,
  ArrowUp,
  AudioLines,
  Check,
  Clock3,
  ExternalLink,
  Keyboard,
  Library,
  Pencil,
  Plus,
  Search,
  TextCursorInput,
  X,
} from "lucide-react";
import { useEffect, useState, type DragEvent } from "react";
import { ActionDialog, type ActionDraft } from "./ActionDialog";
import { formatHotkey } from "./hotkey";
import { t, type MessageKey } from "./i18n";
import type { ActionTrigger, ButtonAction, Language, MediaCommand, ModelButton, TriggerActions } from "./types";

interface ActionEditorProps {
  language: Language;
  button: ModelButton | null;
  actions: TriggerActions;
  onChange(actions: TriggerActions): void;
  onRename(buttonId: string, label: string): void;
  canRename?: boolean;
  emptyTemplates?: ReadonlyArray<{ id: string; name: string }>;
  onUseTemplate?(profileId: string): void;
}

export const TRIGGER_ORDER: ActionTrigger[] = ["press", "release", "long_press", "double_press"];

const TRIGGER_LABELS: Record<ActionTrigger, MessageKey> = {
  press: "behavior.trigger.press",
  release: "behavior.trigger.release",
  long_press: "behavior.trigger.longPress",
  double_press: "behavior.trigger.doublePress",
};

const ACTION_SUMMARY_LABELS: Record<ButtonAction["type"], MessageKey> = {
  paste: "behavior.summary.paste",
  hotkey: "behavior.summary.hotkey",
  delay: "behavior.summary.delay",
  media: "behavior.summary.media",
  open: "behavior.summary.open",
};

type LibraryCategory = "common" | "recent" | ButtonAction["type"];

interface ActionLibraryItem {
  type: ButtonAction["type"];
  label: MessageKey;
  description: MessageKey;
  action: ButtonAction;
}

const ACTION_LIBRARY: readonly ActionLibraryItem[] = [
  {
    type: "paste",
    label: "behavior.paste",
    description: "behavior.library.pasteDescription",
    action: { type: "paste", text: "Paste text" },
  },
  {
    type: "hotkey",
    label: "behavior.hotkey",
    description: "behavior.library.hotkeyDescription",
    action: { type: "hotkey", keys: ["primary", "c"] },
  },
  {
    type: "open",
    label: "behavior.open",
    description: "behavior.library.openDescription",
    action: { type: "open", target: "https://example.com" },
  },
  {
    type: "media",
    label: "behavior.media",
    description: "behavior.library.mediaDescription",
    action: { type: "media", command: "play_pause" },
  },
  {
    type: "delay",
    label: "behavior.delay",
    description: "behavior.library.delayDescription",
    action: { type: "delay", duration_ms: 500 },
  },
];

const LIBRARY_CATEGORIES: ReadonlyArray<{ value: LibraryCategory; label: MessageKey }> = [
  { value: "common", label: "behavior.library.common" },
  { value: "recent", label: "behavior.library.recent" },
  { value: "paste", label: "behavior.paste" },
  { value: "hotkey", label: "behavior.hotkey" },
  { value: "open", label: "behavior.open" },
  { value: "media", label: "behavior.media" },
  { value: "delay", label: "behavior.delay" },
];

const ACTION_DRAG_MIME = "application/x-kivo-action";
const ACTION_REORDER_MIME = "application/x-kivo-action-index";

const MEDIA_COMMANDS: Array<{ value: MediaCommand; label: MessageKey }> = [
  { value: "play_pause", label: "behavior.media.playPause" },
  { value: "previous_track", label: "behavior.media.previousTrack" },
  { value: "next_track", label: "behavior.media.nextTrack" },
  { value: "stop", label: "behavior.media.stop" },
  { value: "volume_up", label: "behavior.media.volumeUp" },
  { value: "volume_down", label: "behavior.media.volumeDown" },
  { value: "mute", label: "behavior.media.mute" },
];

type EditingTarget = { trigger: ActionTrigger; index: number } | "create" | null;

function emptyTriggerActions(): TriggerActions {
  return { press: [], release: [], long_press: [], double_press: [] };
}

function actionSummary(action: ButtonAction, language: Language): string {
  const prefix = t(language, ACTION_SUMMARY_LABELS[action.type]);
  switch (action.type) {
    case "paste":
      return `${prefix} - ${action.text || "-"}`;
    case "hotkey":
      return `${prefix} - ${action.keys.length ? formatHotkey(action.keys, language) : "-"}`;
    case "delay":
      return `${prefix} - ${action.duration_ms} ms`;
    case "media": {
      const command = MEDIA_COMMANDS.find((item) => item.value === action.command);
      return `${prefix} - ${command ? t(language, command.label) : action.command}`;
    }
    case "open":
      return `${prefix} - ${action.target || "-"}`;
  }
}

function actionIcon(action: ButtonAction) {
  switch (action.type) {
    case "paste": return TextCursorInput;
    case "hotkey": return Keyboard;
    case "delay": return Clock3;
    case "media": return AudioLines;
    case "open": return ExternalLink;
  }
}

function normalizedActions(actions: TriggerActions): TriggerActions {
  return { ...emptyTriggerActions(), ...actions };
}

function cloneAction(action: ButtonAction): ButtonAction {
  return action.type === "hotkey"
    ? { ...action, keys: [...action.keys] }
    : { ...action };
}

function libraryItemsForCategory(category: LibraryCategory, recentTypes: ButtonAction["type"][]): ActionLibraryItem[] {
  if (category === "recent") {
    return recentTypes
      .map((type) => ACTION_LIBRARY.find((item) => item.type === type))
      .filter((item): item is ActionLibraryItem => Boolean(item));
  }
  if (category === "common") return [...ACTION_LIBRARY];
  return ACTION_LIBRARY.filter((item) => item.type === category);
}

export function ActionEditor({
  language,
  button,
  actions,
  onChange,
  onRename,
  canRename = true,
  emptyTemplates = [],
  onUseTemplate,
}: ActionEditorProps) {
  const [editingTarget, setEditingTarget] = useState<EditingTarget>(null);
  const [dialogDraft, setDialogDraft] = useState<ActionDraft | undefined>();
  const [editingLabel, setEditingLabel] = useState(false);
  const [labelDraft, setLabelDraft] = useState(button?.label ?? "");
  const [libraryQuery, setLibraryQuery] = useState("");
  const [libraryCategory, setLibraryCategory] = useState<LibraryCategory>("common");
  const [recentTypes, setRecentTypes] = useState<ButtonAction["type"][]>([]);
  const [advancedTriggersOpen, setAdvancedTriggersOpen] = useState(false);
  const [dropTargetTrigger, setDropTargetTrigger] = useState<ActionTrigger | null>(null);
  const groups = normalizedActions(actions);
  const totalCount = TRIGGER_ORDER.reduce((count, trigger) => count + groups[trigger].length, 0);
  const advancedTriggers: ActionTrigger[] = ["release", "long_press", "double_press"];
  const advancedCount = advancedTriggers.reduce((count, trigger) => count + groups[trigger].length, 0);

  const libraryItems = libraryItemsForCategory(libraryCategory, recentTypes)
    .filter((item) => {
      const query = libraryQuery.trim().toLocaleLowerCase();
      if (!query) return true;
      return [t(language, item.label), t(language, item.description), item.type]
        .some((value) => value.toLocaleLowerCase().includes(query));
    });

  useEffect(() => {
    setEditingLabel(false);
    setLabelDraft(button?.label ?? "");
    setAdvancedTriggersOpen(false);
    setLibraryQuery("");
    setLibraryCategory("common");
    setDropTargetTrigger(null);
  }, [button?.id]);

  if (!button) {
    return (
      <aside className="action-panel" aria-labelledby="action-title">
        <h2 id="action-title">{t(language, "behavior.title")}</h2>
        <div className="panel-empty action-empty-state">
          <span>{t(language, "behavior.empty")}</span>
          {emptyTemplates.length > 0 && onUseTemplate ? (
            <section className="action-empty-templates" aria-labelledby="action-empty-templates-title">
              <h3 id="action-empty-templates-title">{t(language, "behavior.emptyTemplates")}</h3>
              <p>{t(language, "behavior.emptyTemplatesHint")}</p>
              <div className="setup-template-grid">
                {emptyTemplates.map((template) => (
                  <button
                    className="setup-template-card"
                    type="button"
                    key={template.id}
                    onClick={() => onUseTemplate(template.id)}
                  >
                    <span className="setup-template-icon" aria-hidden="true"><Keyboard size={18} /></span>
                    <span className="setup-template-copy">
                      <strong>{template.name}</strong>
                      <small>{t(language, "behavior.useTemplate")}</small>
                    </span>
                  </button>
                ))}
              </div>
            </section>
          ) : null}
        </div>
      </aside>
    );
  }

  const openCreateDialog = () => {
    setDialogDraft(undefined);
    setEditingTarget("create");
  };

  const openEditDialog = (trigger: ActionTrigger, index: number) => {
    const action = groups[trigger][index];
    if (!action) return;
    setDialogDraft({ trigger, action });
    setEditingTarget({ trigger, index });
  };

  const updateGroup = (trigger: ActionTrigger, nextActions: ButtonAction[]) => {
    onChange({ ...groups, [trigger]: nextActions });
  };

  const addLibraryAction = (item: ActionLibraryItem, trigger: ActionTrigger = "press") => {
    updateGroup(trigger, [...groups[trigger], cloneAction(item.action)]);
    setRecentTypes((current) => [item.type, ...current.filter((type) => type !== item.type)]);
  };

  const handleLibraryDragStart = (event: DragEvent<HTMLButtonElement>, item: ActionLibraryItem) => {
    event.dataTransfer.effectAllowed = "copy";
    event.dataTransfer.setData(ACTION_DRAG_MIME, item.type);
  };

  const handleLibraryDragEnd = () => setDropTargetTrigger(null);

  const hasDataTransferType = (event: DragEvent<HTMLElement>, type: string) =>
    Array.from(event.dataTransfer.types).includes(type);

  const handleTriggerDragEnter = (event: DragEvent<HTMLElement>, trigger: ActionTrigger) => {
    if (hasDataTransferType(event, ACTION_DRAG_MIME)) setDropTargetTrigger(trigger);
  };

  const handleTriggerDragOver = (event: DragEvent<HTMLElement>, trigger: ActionTrigger) => {
    if (!hasDataTransferType(event, ACTION_DRAG_MIME)) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    setDropTargetTrigger(trigger);
  };

  const handleTriggerDrop = (event: DragEvent<HTMLElement>, trigger: ActionTrigger) => {
    if (!hasDataTransferType(event, ACTION_DRAG_MIME)) return;
    event.preventDefault();
    event.stopPropagation();
    setDropTargetTrigger(null);
    const type = event.dataTransfer.getData(ACTION_DRAG_MIME);
    const item = ACTION_LIBRARY.find((candidate) => candidate.type === type);
    if (item) addLibraryAction(item, trigger);
  };

  const handleActionDragStart = (
    event: DragEvent<HTMLDivElement>,
    trigger: ActionTrigger,
    index: number,
  ) => {
    event.stopPropagation();
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData(
      ACTION_REORDER_MIME,
      JSON.stringify({ trigger, index }),
    );
  };

  const handleActionDrop = (
    event: DragEvent<HTMLDivElement>,
    targetTrigger: ActionTrigger,
    targetIndex: number,
  ) => {
    const serialized = event.dataTransfer.getData(ACTION_REORDER_MIME);
    if (!serialized) return;
    event.preventDefault();
    event.stopPropagation();
    try {
      const source = JSON.parse(serialized) as {
        trigger?: ActionTrigger;
        index?: number;
      };
      const sourceIndex = source.index;
      if (
        source.trigger !== targetTrigger ||
        typeof sourceIndex !== "number" ||
        !Number.isInteger(sourceIndex) ||
        sourceIndex < 0 ||
        sourceIndex >= groups[targetTrigger].length ||
        targetIndex < 0 ||
        targetIndex >= groups[targetTrigger].length ||
        sourceIndex === targetIndex
      ) {
        return;
      }
      const group = [...groups[targetTrigger]];
      const [moved] = group.splice(sourceIndex, 1);
      if (!moved) return;
      group.splice(targetIndex, 0, moved);
      updateGroup(targetTrigger, group);
    } catch {
      // Ignore unrelated or malformed native drag payloads.
    }
  };

  const moveAction = (trigger: ActionTrigger, index: number, offset: -1 | 1) => {
    const nextIndex = index + offset;
    const group = groups[trigger];
    if (nextIndex < 0 || nextIndex >= group.length) return;
    const next = [...group];
    [next[index], next[nextIndex]] = [next[nextIndex], next[index]];
    updateGroup(trigger, next);
  };

  const saveAction = ({ trigger, action }: ActionDraft) => {
    const target = editingTarget;
    const next = normalizedActions(groups);
    if (target === "create" || target === null) {
      next[trigger] = [...next[trigger], action];
    } else if (target.trigger === trigger) {
      if (target.index < 0 || target.index >= next[target.trigger].length) {
        setEditingTarget(null);
        return;
      }
      next[trigger] = next[trigger].map((item, index) => index === target.index ? action : item);
    } else {
      if (target.index < 0 || target.index >= next[target.trigger].length) {
        setEditingTarget(null);
        return;
      }
      next[target.trigger] = next[target.trigger].filter((_, index) => index !== target.index);
      next[trigger] = [...next[trigger], action];
    }
    setEditingTarget(null);
    onChange(next);
    setRecentTypes((current) => [action.type, ...current.filter((type) => type !== action.type)]);
  };

  const deleteAction = () => {
    if (!editingTarget || editingTarget === "create") return;
    const next = normalizedActions(groups);
    if (editingTarget.index < 0 || editingTarget.index >= next[editingTarget.trigger].length) {
      setEditingTarget(null);
      return;
    }
    next[editingTarget.trigger] = next[editingTarget.trigger]
      .filter((_, index) => index !== editingTarget.index);
    setEditingTarget(null);
    onChange(next);
  };

  const saveLabel = () => {
    const label = labelDraft.trim();
    if (!button || !label) return;
    onRename(button.id, label);
    setEditingLabel(false);
  };

  const renderTriggerGroup = (trigger: ActionTrigger) => {
    const group = groups[trigger];
    return (
      <section
        className={`action-group${dropTargetTrigger === trigger ? " is-drop-target" : ""}`}
        key={trigger}
        aria-labelledby={`action-group-${trigger}`}
        onDragEnter={(event) => handleTriggerDragEnter(event, trigger)}
        onDragOver={(event) => handleTriggerDragOver(event, trigger)}
        onDrop={(event) => handleTriggerDrop(event, trigger)}
      >
        <div className="action-group-heading">
          <h3 id={`action-group-${trigger}`}>{t(language, TRIGGER_LABELS[trigger])}</h3>
          <span>{group.length}</span>
        </div>
        <div className="action-group-items">
          {group.length === 0 ? <div className="action-group-empty">{t(language, "behavior.noActions")}</div> : null}
          {group.map((action, index) => {
            const summary = actionSummary(action, language);
            const Icon = actionIcon(action);
            return (
              <div
                className="action-row"
                key={`${trigger}-${index}`}
                draggable
                onDragStart={(event) => handleActionDragStart(event, trigger, index)}
                onDragOver={(event) => {
                  if (!hasDataTransferType(event, ACTION_REORDER_MIME)) return;
                  event.preventDefault();
                  event.stopPropagation();
                  event.dataTransfer.dropEffect = "move";
                }}
                onDrop={(event) => handleActionDrop(event, trigger, index)}
              >
                <button
                  className="action-row-main"
                  type="button"
                  aria-label={`${t(language, "behavior.edit")} ${summary}`}
                  title={`${t(language, "behavior.edit")} ${summary}`}
                  onClick={() => openEditDialog(trigger, index)}
                >
                  <Icon size={15} aria-hidden="true" />
                  <span>{summary}</span>
                </button>
                <div className="action-row-controls">
                  <button
                    className="icon-button"
                    type="button"
                    aria-label={t(language, "behavior.moveUp")}
                    title={`${t(language, "behavior.moveUp")}: ${summary}`}
                    disabled={index === 0}
                    onClick={() => moveAction(trigger, index, -1)}
                  ><ArrowUp size={15} aria-hidden="true" /></button>
                  <button
                    className="icon-button"
                    type="button"
                    aria-label={t(language, "behavior.moveDown")}
                    title={`${t(language, "behavior.moveDown")}: ${summary}`}
                    disabled={index === group.length - 1}
                    onClick={() => moveAction(trigger, index, 1)}
                  ><ArrowDown size={15} aria-hidden="true" /></button>
                </div>
              </div>
            );
          })}
        </div>
      </section>
    );
  };

  return (
    <aside className="action-panel" aria-labelledby="action-title">
      <div className="panel-title">
        <div className="action-panel-heading">
          <span>{t(language, "behavior.title")}</span>
          {editingLabel && canRename ? (
            <div className="button-label-edit">
              <input
                autoFocus
                aria-label={t(language, "behavior.buttonName")}
                value={labelDraft}
                onChange={(event) => setLabelDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") saveLabel();
                  if (event.key === "Escape") {
                    setLabelDraft(button.label);
                    setEditingLabel(false);
                  }
                }}
              />
              <button
                className="icon-button"
                type="button"
                aria-label={t(language, "behavior.confirmRename")}
                title={t(language, "behavior.confirmRename")}
                disabled={!labelDraft.trim()}
                onClick={saveLabel}
              ><Check size={16} /></button>
              <button
                className="icon-button"
                type="button"
                aria-label={t(language, "common.cancel")}
                title={t(language, "common.cancel")}
                onClick={() => {
                  setLabelDraft(button.label);
                  setEditingLabel(false);
                }}
              ><X size={16} /></button>
            </div>
          ) : (
            <div className="button-label-display">
              <h2 id="action-title">{button.label}</h2>
              {canRename ? (
                <button
                  className="icon-button"
                  type="button"
                  aria-label={`${t(language, "behavior.renameButton")} ${button.label}`}
                  title={t(language, "behavior.renameButton")}
                  onClick={() => {
                    setLabelDraft(button.label);
                    setEditingLabel(true);
                  }}
                ><Pencil size={16} /></button>
              ) : null}
            </div>
          )}
        </div>
        <strong>{totalCount}</strong>
      </div>

      <section className="action-library" aria-labelledby="action-library-title">
        <div className="action-library-heading">
          <div className="action-library-title">
            <Library size={16} aria-hidden="true" />
            <h3 id="action-library-title">{t(language, "behavior.library.title")}</h3>
          </div>
          <label className="action-library-search">
            <Search size={14} aria-hidden="true" />
            <span className="sr-only">{t(language, "behavior.library.search")}</span>
            <input
              type="search"
              aria-label={t(language, "behavior.library.search")}
              placeholder={t(language, "behavior.library.search")}
              value={libraryQuery}
              onChange={(event) => setLibraryQuery(event.target.value)}
            />
          </label>
        </div>
        <div className="action-library-tabs" role="group" aria-label={t(language, "behavior.library.categories")}>
          {LIBRARY_CATEGORIES.map(({ value, label }) => (
            <button
              key={value}
              type="button"
              aria-pressed={libraryCategory === value}
              onClick={() => setLibraryCategory(value)}
            >
              {t(language, label)}
            </button>
          ))}
        </div>
        <div className="action-library-items" aria-live="polite">
          {libraryItems.length === 0 ? (
            <div className="action-library-empty">
              {libraryCategory === "recent" && recentTypes.length === 0
                ? t(language, "behavior.library.noRecent")
                : t(language, "behavior.library.noResults")}
            </div>
          ) : libraryItems.map((item) => {
            const Icon = actionIcon(item.action);
            return (
              <button
                className="action-library-entry"
                key={item.type}
                type="button"
                draggable
                title={`${t(language, "behavior.library.add")} ${t(language, item.label)}`}
                onClick={() => addLibraryAction(item)}
                onDragStart={(event) => handleLibraryDragStart(event, item)}
                onDragEnd={handleLibraryDragEnd}
              >
                <Icon size={16} aria-hidden="true" />
                <span className="action-library-entry-copy">
                  <strong>{t(language, item.label)}</strong>
                  <small>{t(language, item.description)}</small>
                </span>
                <Plus size={14} aria-hidden="true" />
              </button>
            );
          })}
        </div>
      </section>

      <div className="action-list action-group-list">
        {renderTriggerGroup("press")}
        <details
          className="advanced-triggers"
          open={advancedTriggersOpen}
          onToggle={(event) => setAdvancedTriggersOpen(event.currentTarget.open)}
        >
          <summary>
            <span>{t(language, "behavior.advancedTriggers")}</span>
            <span className="advanced-triggers-count">{advancedCount}</span>
          </summary>
          {advancedTriggersOpen ? (
            <div className="advanced-trigger-groups">
              {advancedTriggers.map(renderTriggerGroup)}
            </div>
          ) : null}
        </details>
      </div>

      <div className="add-actions">
        <button type="button" onClick={openCreateDialog}>
          <Plus size={16} />{t(language, "behavior.add")}
        </button>
      </div>

      <ActionDialog
        open={editingTarget !== null}
        language={language}
        mode={editingTarget === "create" ? "create" : "edit"}
        initial={dialogDraft}
        onSave={saveAction}
        onDelete={editingTarget === "create" ? undefined : deleteAction}
        onCancel={() => setEditingTarget(null)}
      />
    </aside>
  );
}
