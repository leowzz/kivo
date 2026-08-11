import { ArrowDown, ArrowUp, Plus, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { t } from "./i18n";
import type { ButtonGroup, Language, ModelButton, ModelLayout } from "./types";

interface LayoutEditorProps {
  layout: ModelLayout | null;
  language: Language;
  open?: boolean;
  onCancel?(): void;
  onApply?(layout: ModelLayout): void;
  onChange?(layout: ModelLayout): void;
}
type DraftButton = ModelButton & { isNew?: boolean };
type DraftGroup = Omit<ButtonGroup, "buttons"> & { buttons: DraftButton[]; isNew?: boolean };
type DraftLayout = Omit<ModelLayout, "groups"> & { groups: DraftGroup[] };
const NEW_ID_PATTERN = /^[A-Z0-9_]+$/;
function normalizeId(value: string) { return value.toUpperCase().replace(/[^A-Z0-9_]/g, "_"); }
function move<T>(items: T[], index: number, offset: -1 | 1) { const next = [...items]; [next[index], next[index + offset]] = [next[index + offset], next[index]]; return next; }
function validationError(layout: DraftLayout | null, language: Language) {
  const zh = language === "zh-CN";
  if (!layout || layout.groups.length === 0) return zh ? "至少需要一个按键组" : "At least one group is required";
  const groupIds = layout.groups.map((group) => group.id);
  if (groupIds.some((id) => !id.trim())) return zh ? "按键组 ID 不能为空" : "Group IDs are required";
  if (new Set(groupIds).size !== groupIds.length) return zh ? "按键组 ID 不能重复" : "Group IDs must be unique";
  if (layout.groups.some((group) => group.isNew && !NEW_ID_PATTERN.test(group.id))) return zh ? "新按键组 ID 只能使用 A-Z、0-9 和下划线" : "New group IDs must use A-Z, 0-9, and underscores";
  if (layout.groups.some((group) => !Number.isInteger(group.columns) || group.columns < 1)) return zh ? "列数至少为 1" : "Columns must be at least 1";
  if (layout.groups.some((group) => group.buttons.length === 0)) return zh ? "每组至少需要一个按键" : "Groups must contain at least one button";
  const buttons = layout.groups.flatMap((group) => group.buttons);
  if (buttons.some((button) => !button.id.trim())) return zh ? "按键 ID 不能为空" : "Button IDs are required";
  if (buttons.some((button) => button.isNew && !NEW_ID_PATTERN.test(button.id))) return zh ? "新按键 ID 只能使用 A-Z、0-9 和下划线" : "New button IDs must use A-Z, 0-9, and underscores";
  if (new Set(buttons.map((button) => button.id)).size !== buttons.length) return zh ? "按键 ID 不能重复" : "Button IDs must be unique";
  if (buttons.some((button) => !button.label.trim())) return zh ? "按键名称不能为空" : "Button labels are required";
  return null;
}

function toLayout(draft: DraftLayout): ModelLayout {
  return {
    id: draft.id,
    name: draft.name,
    groups: draft.groups.map(({ isNew: _group, ...group }) => ({
      ...group,
      buttons: group.buttons.map(({ isNew: _button, ...button }) => ({
        ...button,
        label: button.label.trim(),
      })),
    })),
  };
}

export function LayoutEditor({ layout, language, open, onCancel = () => undefined, onApply, onChange }: LayoutEditorProps) {
  const embedded = open === undefined;
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [draft, setDraft] = useState<DraftLayout | null>(null);
  const zh = language === "zh-CN";
  const label = (chinese: string, english: string) => zh ? chinese : english;
  useEffect(() => {
    if (embedded || open) setDraft(layout ? { ...layout, groups: layout.groups.map((group) => ({ ...group, buttons: group.buttons.map((button) => ({ ...button })) })) } : null);
  }, [embedded, layout, open]);
  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog || embedded) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [embedded, open]);
  const error = useMemo(() => validationError(draft, language), [draft, language]);
  const updateDraft = (update: (current: DraftLayout) => DraftLayout) => setDraft((current) => {
    if (!current) return current;
    const next = update(current);
    if (embedded && !validationError(next, language)) onChange?.(toLayout(next));
    return next;
  });
  const updateGroup = (groupIndex: number, update: (group: DraftGroup) => DraftGroup) => updateDraft((current) => ({ ...current, groups: current.groups.map((group, index) => index === groupIndex ? update(group) : group) }));
  const apply = () => {
    if (!draft || error) return;
    const next = toLayout(draft);
    onChange?.(next);
    onApply?.(next);
  };
  const content = (
    <>
      <div className="layout-editor-header"><div><h2 id="layout-editor-title">{t(language, "layout.edit")}</h2><p className="modal-subtitle">{label("修改按键分组、列数与名称", "Edit groups, columns, and button labels")}</p></div>{!embedded && <button className="icon-button" type="button" aria-label={t(language, "common.close")} title={t(language, "common.close")} onClick={onCancel}><X size={17} /></button>}</div>
      <div className="layout-editor-body">
        {draft?.groups.map((group, groupIndex) => <section className="layout-group-editor" key={group.isNew ? `new-${groupIndex}` : group.id}>
          <div className="layout-group-header"><label><span>{label("按键组 ID", "Group ID")}</span><input value={group.id} readOnly={!group.isNew} onChange={(event) => updateGroup(groupIndex, (current) => ({ ...current, id: normalizeId(event.target.value) }))} /></label><label className="columns-field"><span>{label("列数", "Columns")}</span><input type="number" min="1" value={group.columns} onChange={(event) => updateGroup(groupIndex, (current) => ({ ...current, columns: Number(event.target.value) }))} /></label><div className="icon-row"><button className="icon-button" type="button" aria-label={label("上移按键组", "Move group up")} title={label("上移按键组", "Move group up")} disabled={groupIndex === 0} onClick={() => updateDraft((current) => ({ ...current, groups: move(current.groups, groupIndex, -1) }))}><ArrowUp size={16} /></button><button className="icon-button" type="button" aria-label={label("下移按键组", "Move group down")} title={label("下移按键组", "Move group down")} disabled={groupIndex === (draft?.groups.length ?? 1) - 1} onClick={() => updateDraft((current) => ({ ...current, groups: move(current.groups, groupIndex, 1) }))}><ArrowDown size={16} /></button><button className="icon-button is-danger" type="button" aria-label={label("删除按键组", "Delete group")} title={label("删除按键组", "Delete group")} onClick={() => updateDraft((current) => ({ ...current, groups: current.groups.filter((_, index) => index !== groupIndex) }))}><Trash2 size={16} /></button></div></div>
          <div className="layout-button-list">{group.buttons.map((button, buttonIndex) => <div className="layout-button-row" key={button.isNew ? `new-${buttonIndex}` : button.id}><label><span>{label("按键 ID", "Button ID")}</span><input value={button.id} readOnly={!button.isNew} onChange={(event) => updateGroup(groupIndex, (current) => ({ ...current, buttons: current.buttons.map((item, index) => index === buttonIndex ? { ...item, id: normalizeId(event.target.value) } : item) }))} /></label><label><span>{label("名称", "Label")}</span><input value={button.label} onChange={(event) => updateGroup(groupIndex, (current) => ({ ...current, buttons: current.buttons.map((item, index) => index === buttonIndex ? { ...item, label: event.target.value } : item) }))} /></label><div className="icon-row"><button className="icon-button" type="button" aria-label={label("上移按键", "Move button up")} title={label("上移按键", "Move button up")} disabled={buttonIndex === 0} onClick={() => updateGroup(groupIndex, (current) => ({ ...current, buttons: move(current.buttons, buttonIndex, -1) }))}><ArrowUp size={16} /></button><button className="icon-button" type="button" aria-label={label("下移按键", "Move button down")} title={label("下移按键", "Move button down")} disabled={buttonIndex === group.buttons.length - 1} onClick={() => updateGroup(groupIndex, (current) => ({ ...current, buttons: move(current.buttons, buttonIndex, 1) }))}><ArrowDown size={16} /></button><button className="icon-button is-danger" type="button" aria-label={label("删除按键", "Delete button")} title={label("删除按键", "Delete button")} onClick={() => updateGroup(groupIndex, (current) => ({ ...current, buttons: current.buttons.filter((_, index) => index !== buttonIndex) }))}><Trash2 size={16} /></button></div></div>)}</div>
          <button className="layout-add-button" type="button" onClick={() => updateGroup(groupIndex, (current) => ({ ...current, buttons: [...current.buttons, { id: "", label: "", isNew: true }] }))}><Plus size={15} />{label("添加按键", "Add button")}</button>
        </section>)}
        <button className="layout-add-button" type="button" onClick={() => updateDraft((current) => ({ ...current, groups: [...current.groups, { id: "", columns: 1, buttons: [], isNew: true }] }))}><Plus size={15} />{label("添加按键组", "Add group")}</button>
      </div>
      {(error || !embedded) && <div className="layout-editor-footer">{error && <p className="layout-editor-error" role="alert">{error}</p>}{!embedded && <button type="button" onClick={onCancel}>{t(language, "common.cancel")}</button>}{!embedded && <button className="primary-button" type="button" disabled={Boolean(error)} onClick={apply}>{t(language, "layout.apply")}</button>}</div>}
    </>
  );
  if (embedded) return <section className="layout-editor embedded-layout-editor" aria-labelledby="layout-editor-title">{content}</section>;
  return <dialog className="layout-editor" ref={dialogRef} aria-labelledby="layout-editor-title" onCancel={(event) => { event.preventDefault(); onCancel(); }}>{content}</dialog>;
}
