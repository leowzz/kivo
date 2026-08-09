import {
  ArrowDown,
  ArrowUp,
  AudioLines,
  Clock3,
  ExternalLink,
  Keyboard,
  Plus,
  TextCursorInput,
} from "lucide-react";
import { useState } from "react";
import { ActionDialog, type ActionDraft } from "./ActionDialog";
import { formatHotkey } from "./hotkey";
import { t, type MessageKey } from "./i18n";
import type { ActionTrigger, ButtonAction, Language, MediaCommand, ModelButton, TriggerActions } from "./types";

interface ActionEditorProps {
  language: Language;
  button: ModelButton | null;
  actions: TriggerActions;
  onChange(actions: TriggerActions): void;
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

function emptyTriggerActions(): TriggerActions {
  return { press: [], release: [], long_press: [], double_press: [] };
}

function actionSummary(action: ButtonAction, language: Language): string {
  const prefix = t(language, ACTION_SUMMARY_LABELS[action.type]);
  switch (action.type) {
    case "paste":
      return `${prefix} - ${action.text || "-"}`;
    case "hotkey":
      return `${prefix} - ${action.keys.length ? formatHotkey(action.keys) : "-"}`;
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

export function ActionEditor({ language, button, actions, onChange }: ActionEditorProps) {
  const [editingTarget, setEditingTarget] = useState<EditingTarget>(null);
  const [dialogDraft, setDialogDraft] = useState<ActionDraft | undefined>();
  const groups = normalizedActions(actions);
  const totalCount = TRIGGER_ORDER.reduce((count, trigger) => count + groups[trigger].length, 0);

  if (!button) {
    return (
      <aside className="action-panel" aria-labelledby="action-title">
        <h2 id="action-title">{t(language, "behavior.title")}</h2>
        <div className="panel-empty">{t(language, "behavior.empty")}</div>
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

  return (
    <aside className="action-panel" aria-labelledby="action-title">
      <div className="panel-title">
        <div>
          <span>{t(language, "behavior.title")}</span>
          <h2 id="action-title">{button.label}</h2>
        </div>
        <strong>{totalCount}</strong>
      </div>

      <div className="action-list action-group-list">
        {totalCount === 0 && <div className="panel-empty">{t(language, "behavior.noActions")}</div>}
        {TRIGGER_ORDER.map((trigger) => {
          const group = groups[trigger];
          if (group.length === 0) return null;
          return (
            <section className="action-group" key={trigger} aria-labelledby={`action-group-${trigger}`}>
              <div className="action-group-heading">
                <h3 id={`action-group-${trigger}`}>{t(language, TRIGGER_LABELS[trigger])}</h3>
                <span>{group.length}</span>
              </div>
              <div className="action-group-items">
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
              </div>
            </section>
          );
        })}
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
