import { ArrowDown, ArrowUp, Plus, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { ButtonGroup, ModelButton, ModelLayout } from "./types";

interface LayoutEditorProps {
  layout: ModelLayout | null;
  open: boolean;
  onCancel(): void;
  onApply(layout: ModelLayout): void;
}

type DraftButton = ModelButton & { isNew?: boolean };
type DraftGroup = Omit<ButtonGroup, "buttons"> & {
  buttons: DraftButton[];
  isNew?: boolean;
};
type DraftLayout = Omit<ModelLayout, "groups"> & { groups: DraftGroup[] };

const NEW_ID_PATTERN = /^[A-Z0-9_]+$/;

function normalizeId(value: string) {
  return value.toUpperCase().replace(/[^A-Z0-9_]/g, "_");
}

function move<T>(items: T[], index: number, offset: -1 | 1) {
  const next = [...items];
  [next[index], next[index + offset]] = [next[index + offset], next[index]];
  return next;
}

function validationError(layout: DraftLayout | null) {
  if (!layout || layout.groups.length === 0) return "At least one group is required";
  const groupIds = layout.groups.map((group) => group.id);
  if (groupIds.some((id) => !id.trim())) return "Group IDs are required";
  if (new Set(groupIds).size !== groupIds.length) return "Group IDs must be unique";
  if (layout.groups.some((group) =>
    group.isNew && !NEW_ID_PATTERN.test(group.id)
  )) return "New group IDs must use A-Z, 0-9, and underscores";
  if (layout.groups.some((group) =>
    !Number.isInteger(group.columns) || group.columns < 1
  )) return "Columns must be at least 1";
  if (layout.groups.some((group) => group.buttons.length === 0)) {
    return "Groups must contain at least one button";
  }
  const buttons = layout.groups.flatMap((group) => group.buttons);
  if (buttons.some((button) => !button.id.trim())) return "Button IDs are required";
  if (buttons.some((button) => button.isNew && !NEW_ID_PATTERN.test(button.id))) {
    return "New button IDs must use A-Z, 0-9, and underscores";
  }
  if (new Set(buttons.map((button) => button.id)).size !== buttons.length) {
    return "Button IDs must be unique";
  }
  if (buttons.some((button) => !button.label.trim())) return "Button labels are required";
  return null;
}

export function LayoutEditor({ layout, open, onCancel, onApply }: LayoutEditorProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [draft, setDraft] = useState<DraftLayout | null>(null);

  useEffect(() => {
    if (open) {
      setDraft(layout ? {
        ...layout,
        groups: layout.groups.map((group) => ({
          ...group,
          buttons: group.buttons.map((button) => ({ ...button })),
        })),
      } : null);
    }
  }, [layout, open]);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  const error = useMemo(() => validationError(draft), [draft]);

  const updateGroup = (groupIndex: number, update: (group: DraftGroup) => DraftGroup) => {
    setDraft((current) => current && ({
      ...current,
      groups: current.groups.map((group, index) => index === groupIndex ? update(group) : group),
    }));
  };

  const apply = () => {
    if (!draft || error) return;
    onApply({
      id: draft.id,
      name: draft.name,
      groups: draft.groups.map(({ isNew: _newGroup, ...group }) => ({
        ...group,
        id: group.id.trim(),
        buttons: group.buttons.map(({ isNew: _newButton, ...button }) => ({
          ...button,
          id: button.id.trim(),
          label: button.label.trim(),
        })),
      })),
    });
  };

  return (
    <dialog
      className="layout-editor"
      ref={dialogRef}
      aria-labelledby="layout-editor-title"
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
    >
      <div className="layout-editor-header">
        <div>
          <p className="eyebrow">Developer tool</p>
          <h2 id="layout-editor-title">Edit {layout?.name ?? "layout"}</h2>
        </div>
        <button
          className="icon-button"
          type="button"
          aria-label="Close layout editor"
          title="Close"
          onClick={onCancel}
        >
          <X size={17} />
        </button>
      </div>

      <div className="layout-editor-body">
        {draft?.groups.map((group, groupIndex) => (
          <section className="layout-group-editor" key={group.isNew ? `new-${groupIndex}` : group.id}>
            <div className="layout-group-header">
              <label>
                <span>Group ID</span>
                <input
                  aria-label={group.isNew ? "New group ID" : `Group ID ${group.id}`}
                  value={group.id}
                  readOnly={!group.isNew}
                  onChange={(event) => updateGroup(groupIndex, (current) => ({
                    ...current,
                    id: normalizeId(event.target.value),
                  }))}
                />
              </label>
              <label className="columns-field">
                <span>Columns</span>
                <input
                  type="number"
                  min="1"
                  step="1"
                  aria-label={`Columns for ${group.id}`}
                  value={group.columns}
                  onChange={(event) => updateGroup(groupIndex, (current) => ({
                    ...current,
                    columns: Number(event.target.value),
                  }))}
                />
              </label>
              <div className="layout-order-controls">
                <button
                  className="icon-button"
                  type="button"
                  aria-label={`Move group ${group.id} up`}
                  title="Move group up"
                  disabled={groupIndex === 0}
                  onClick={() => setDraft((current) => current && ({
                    ...current,
                    groups: move(current.groups, groupIndex, -1),
                  }))}
                >
                  <ArrowUp size={16} />
                </button>
                <button
                  className="icon-button"
                  type="button"
                  aria-label={`Move group ${group.id} down`}
                  title="Move group down"
                  disabled={groupIndex === draft.groups.length - 1}
                  onClick={() => setDraft((current) => current && ({
                    ...current,
                    groups: move(current.groups, groupIndex, 1),
                  }))}
                >
                  <ArrowDown size={16} />
                </button>
                <button
                  className="icon-button is-danger"
                  type="button"
                  aria-label={`Delete group ${group.id}`}
                  title="Delete group"
                  onClick={() => setDraft((current) => current && ({
                    ...current,
                    groups: current.groups.filter((_, index) => index !== groupIndex),
                  }))}
                >
                  <Trash2 size={16} />
                </button>
              </div>
            </div>

            <div className="layout-button-list">
              {group.buttons.map((button, buttonIndex) => (
                <div
                  className="layout-button-row"
                  key={button.isNew ? `new-${buttonIndex}` : button.id}
                >
                  <label>
                    <span>Button ID</span>
                    <input
                      aria-label={button.isNew ? "New button ID" : `Button ID ${button.id}`}
                      value={button.id}
                      readOnly={!button.isNew}
                      onChange={(event) => updateGroup(groupIndex, (current) => ({
                        ...current,
                        buttons: current.buttons.map((item, index) => index === buttonIndex
                          ? { ...item, id: normalizeId(event.target.value) }
                          : item),
                      }))}
                    />
                  </label>
                  <label>
                    <span>Label</span>
                    <input
                      aria-label={button.isNew
                        ? "Label for new button"
                        : `Label for ${button.id}`}
                      value={button.label}
                      onChange={(event) => updateGroup(groupIndex, (current) => ({
                        ...current,
                        buttons: current.buttons.map((item, index) => index === buttonIndex
                          ? { ...item, label: event.target.value }
                          : item),
                      }))}
                    />
                  </label>
                  <div className="layout-order-controls">
                    <button
                      className="icon-button"
                      type="button"
                      aria-label={`Move ${button.id || "new button"} up`}
                      title="Move button up"
                      disabled={buttonIndex === 0}
                      onClick={() => updateGroup(groupIndex, (current) => ({
                        ...current,
                        buttons: move(current.buttons, buttonIndex, -1),
                      }))}
                    >
                      <ArrowUp size={16} />
                    </button>
                    <button
                      className="icon-button"
                      type="button"
                      aria-label={`Move ${button.id || "new button"} down`}
                      title="Move button down"
                      disabled={buttonIndex === group.buttons.length - 1}
                      onClick={() => updateGroup(groupIndex, (current) => ({
                        ...current,
                        buttons: move(current.buttons, buttonIndex, 1),
                      }))}
                    >
                      <ArrowDown size={16} />
                    </button>
                    <button
                      className="icon-button is-danger"
                      type="button"
                      aria-label={`Delete ${button.id || "new button"}`}
                      title="Delete button"
                      onClick={() => updateGroup(groupIndex, (current) => ({
                        ...current,
                        buttons: current.buttons.filter((_, index) => index !== buttonIndex),
                      }))}
                    >
                      <Trash2 size={16} />
                    </button>
                  </div>
                </div>
              ))}
            </div>

            <button
              className="layout-add-button"
              type="button"
              aria-label={`Add button to ${group.id}`}
              onClick={() => updateGroup(groupIndex, (current) => ({
                ...current,
                buttons: [...current.buttons, { id: "", label: "", isNew: true }],
              }))}
            >
              <Plus size={15} />
              Add button
            </button>
          </section>
        ))}

        <button
          className="layout-add-button"
          type="button"
          onClick={() => setDraft((current) => current && ({
            ...current,
            groups: [...current.groups, {
              id: "",
              columns: 1,
              buttons: [],
              isNew: true,
            }],
          }))}
        >
          <Plus size={15} />
          Add group
        </button>
      </div>

      <div className="layout-editor-footer">
        {error && <p className="layout-editor-error" role="alert">{error}</p>}
        <button type="button" onClick={onCancel}>Cancel</button>
        <button className="layout-apply" type="button" disabled={Boolean(error)} onClick={apply}>
          Apply layout
        </button>
      </div>
    </dialog>
  );
}
