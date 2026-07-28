import { X } from "lucide-react";
import type { ReactNode } from "react";

interface ConfirmDialogProps {
  title: string;
  body: string;
  summary?: ReactNode;
  confirmLabel: string;
  cancelLabel: string;
  danger?: boolean;
  onConfirm(): void;
  onCancel(): void;
}

export function ConfirmDialog({
  title,
  body,
  summary,
  confirmLabel,
  cancelLabel,
  danger,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget) onCancel();
    }}>
      <section className="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
        <div className="confirm-header">
          <h2 id="confirm-title">{title}</h2>
          <button className="icon-button" type="button" aria-label={cancelLabel} title={cancelLabel} onClick={onCancel}>
            <X size={17} />
          </button>
        </div>
        <p>{body}</p>
        {summary && <div className="confirm-summary">{summary}</div>}
        <div className="confirm-actions">
          <button type="button" onClick={onCancel}>{cancelLabel}</button>
          <button className={danger ? "danger-button" : "primary-button"} type="button" onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </section>
    </div>
  );
}
