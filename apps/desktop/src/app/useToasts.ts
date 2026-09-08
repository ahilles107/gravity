import { useCallback, useRef, useState } from "react";
import type { Toast, ToastAction } from "../components/Toasts";
import type { NotifyLevel } from "../protocol/entities";

const TOAST_TTL_MS = 7000;
const MAX_VISIBLE = 5;

interface ToastOptions {
  readonly action?: ToastAction;
  /** Keeps the toast up until it is dismissed by hand. */
  readonly sticky?: boolean;
}

export type AddToast = (
  level: NotifyLevel,
  title: string,
  body: string,
  options?: ToastOptions,
) => void;

export interface ToastsApi {
  readonly toasts: readonly Toast[];
  readonly addToast: AddToast;
  readonly dismissToast: (id: number) => void;
}

/**
 * Trims to `MAX_VISIBLE` by evicting the oldest transient toast first, so a
 * sticky prompt outlives a burst of ordinary notifications. Sticky toasts go
 * only once there is nothing transient left to drop.
 */
function trimToCap(toasts: readonly Toast[]): readonly Toast[] {
  if (toasts.length <= MAX_VISIBLE) {
    return toasts;
  }
  const kept = [...toasts];
  while (kept.length > MAX_VISIBLE) {
    const transient = kept.findIndex((toast) => toast.sticky !== true);
    kept.splice(transient === -1 ? 0 : transient, 1);
  }
  return kept;
}

/** Transient notification stack; each toast self-dismisses after `TOAST_TTL_MS` unless sticky. */
export function useToasts(): ToastsApi {
  const [toasts, setToasts] = useState<readonly Toast[]>([]);
  const nextIdRef = useRef(1);

  const dismissToast = useCallback((id: number): void => {
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
  }, []);

  const addToast = useCallback<AddToast>(
    (level, title, body, options) => {
      const id = nextIdRef.current;
      nextIdRef.current += 1;
      const sticky = options?.sticky === true;
      const action = options?.action;
      const toast: Toast = {
        id,
        level,
        title,
        body,
        ...(action === undefined ? {} : { action }),
        ...(sticky ? { sticky } : {}),
      };
      setToasts((prev) => trimToCap([...prev, toast]));
      if (sticky) {
        return;
      }
      setTimeout(() => {
        dismissToast(id);
      }, TOAST_TTL_MS);
    },
    [dismissToast],
  );

  return { toasts, addToast, dismissToast };
}
