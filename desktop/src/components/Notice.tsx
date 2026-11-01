import type { ReactNode } from "react";
import { useEffect, useId } from "react";
import { Icon } from "./Icon";
import { useDialogFocus } from "./useDialogFocus";

interface NoticeProps {
  tone?: "info" | "error" | "ok";
  children: ReactNode;
}

export function Notice({ tone = "info", children }: NoticeProps) {
  return (
    <p
      className={`notice notice-${tone}`}
      role={tone === "error" ? "alert" : "status"}
    >
      <Icon name={tone === "error" ? "alert" : tone === "ok" ? "check" : "info"} />
      {children}
    </p>
  );
}

interface ConfirmProps {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  busy?: boolean;
  destructive?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  title,
  children,
  confirmLabel,
  busy = false,
  destructive = true,
  onConfirm,
  onCancel,
}: ConfirmProps) {
  const dialogRef = useDialogFocus<HTMLDivElement>();
  const titleId = useId();

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onCancel();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [busy, onCancel]);

  return (
    <div
      className="overlay"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onCancel();
      }}
    >
      <div
        ref={dialogRef}
        tabIndex={-1}
        className="confirm raised"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <h2 id={titleId}>{title}</h2>
        <div className="confirm-body">{children}</div>
        <div className="dialog-actions">
          <button
            type="button"
            className="text-button"
            onClick={onCancel}
            disabled={busy}
          >
            Cancel
          </button>
          <button
            type="button"
            className={destructive ? "btn-danger" : "btn"}
            onClick={onConfirm}
            disabled={busy}
            aria-busy={busy || undefined}
          >
            {busy ? "Working…" : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
