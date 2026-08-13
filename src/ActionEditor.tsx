import {
  ArrowDown,
  ArrowUp,
  AudioLines,
  Check,
  Clock3,
  ExternalLink,
  Keyboard,
  Pencil,
  Plus,
  TextCursorInput,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
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

type CommonAction = {
  key: "copy" | "paste" | "text" | "hotkey" | "open" | "media";
  draft: ActionDraft;
  commitImmediately: boolean;
};

const COMMON_ACTIONS: CommonAction[] = [
  { key: "copy", draft: { trigger: "press", action: { type: "hotkey", keys: ["primary", "c"] } }, commitImmediately: true },
  { key: "paste", draft: { trigger: "press", action: { type: "hotkey", keys: ["primary", "v"] } }, commitImmediately: true },
  { key: "text", draft: { trigger: "press", action: { type: "paste", text: "" } }, commitImmediately: false },
  { key: "hotkey", draft: { trigger: "press", action: { type: "hotkey", keys: [] } }, commitImmediately: false },
  { key: "open", draft: { trigger: "press", action: { type: "open", target: "" } }, commitImmediately: false },
  { key: "media", draft: { trigger: "press", action: { type: "media", command: "play_pause" } }, commitImmediately: false },
];

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

export function ActionEditor({ language, button, actions, onChange, onRename }: ActionEditorProps) {
  const [editingTarget, setEditingTarget] = useState<EditingTarget>(null);
  const [dialogDraft, setDialogDraft] = useState<ActionDraft | undefined>();
  const [editingLabel, setEditingLabel] = useState(false);
  const [labelDraft, setLabelDraft] = useState(button?.label ?? "");
  const groups = normalizedActions(actions);
  const totalCount = TRIGGER_ORDER.reduce((count, trigger) => count + groups[trigger].length, 0);

  useEffect(() => {
    setEditingLabel(false);
    setLabelDraft(button?.label ?? "");
  }, [button?.id]);

  if (!button) {
    return (
      <aside className="action-panel" aria-labelledby="action-title">
        <h2 id="action-title">{t(language, "behavior.title")}</h2>
        <div className="panel-empty">{t(language, "behavior.empty")}</div>
      </aside>
    );
  }

  const openCreateDialog = () => {
    setDialogDraft({ trigger: "press", action: { type: "hotkey", keys: [] } });
    setEditingTarget("create");
  };

  const chooseCommonAction = (commonAction: CommonAction) => {
    if (commonAction.commitImmediately) {
      onChange({ ...groups, press: [...groups.press, commonAction.draft.action] });
      return;
    }
    setDialogDraft(commonAction.draft);
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

  return (
    <aside className="action-panel" aria-labelledby="action-title">
      <div className="panel-title">
        <div className="action-panel-heading">
          <span>{t(language, "behavior.title")}</span>
          {editingLabel ? (
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
            </div>
          )}
        </div>
        <strong>{totalCount}</strong>
      </div>

      <div className="action-list action-group-list">
        {TRIGGER_ORDER.map((trigger) => {
          const group = groups[trigger];
          if (trigger !== "press" && group.length === 0) return null;
          const isUnconfiguredPress = trigger === "press" && group.length === 0;
          return (
            <section className="action-group" key={trigger} aria-labelledby={`action-group-${trigger}`}>
              <div className="action-group-heading">
                <h3 id={`action-group-${trigger}`}>
                  {isUnconfiguredPress ? t(language, "behavior.pressUnconfigured") : t(language, TRIGGER_LABELS[trigger])}
                </h3>
                {!isUnconfiguredPress && <span>{group.length}</span>}
              </div>
              {isUnconfiguredPress ? (
                <div className="common-action-grid">
                  {COMMON_ACTIONS.map((commonAction) => (
                    <button
                      type="button"
                      key={commonAction.key}
                      onClick={() => chooseCommonAction(commonAction)}
                    >
                      {t(language, `behavior.common.${commonAction.key}` as MessageKey)}
                    </button>
                  ))}
                </div>
              ) : <div className="action-group-items">
                {group.map((action, index) => {
                  const summary = actionSummary(action, language);
                  const Icon = actionIcon(action);
                  return (
                    <div className="action-row" key={`${trigger}-${index}`}>
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
              </div>}
            </section>
          );
        })}
      </div>

      <div className="add-actions">
        <button type="button" onClick={openCreateDialog}>
          <Plus size={16} />{t(language, "behavior.addOther")}
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
