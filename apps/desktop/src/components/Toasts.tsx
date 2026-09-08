import type { ReactElement } from "react";
import type { NotifyLevel } from "../protocol/entities";

export interface ToastAction {
  readonly label: string;
  readonly run: () => void;
}

export interface Toast {
  readonly id: number;
  readonly level: NotifyLevel;
  readonly title: string;
  readonly body: string;
  readonly action?: ToastAction;
  /** Survives both the TTL and the visible-toast cap; dismissed by hand. */
  readonly sticky?: boolean;
}

interface ToastsProps {
  readonly toasts: readonly Toast[];
  readonly onDismiss: (id: number) => void;
}

export default function Toasts({ toasts, onDismiss }: ToastsProps): ReactElement | null {
  if (toasts.length === 0) {
    return null;
  }
  return (
    <div className="toasts">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast toast-${toast.level}`}>
          <div className="toast-text">
            <div className="toast-title">{toast.title}</div>
            {toast.body.length > 0 ? <div className="toast-body">{toast.body}</div> : null}
          </div>
          <div className="toast-actions">
            {toast.action !== undefined ? (
              <button
                type="button"
                className="btn btn-small"
                onClick={() => {
                  toast.action?.run();
                  onDismiss(toast.id);
                }}
              >
                {toast.action.label}
              </button>
            ) : null}
            <button
              type="button"
              className="toast-close"
              aria-label="Dismiss"
              onClick={() => {
                onDismiss(toast.id);
              }}
            >
              ×
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
