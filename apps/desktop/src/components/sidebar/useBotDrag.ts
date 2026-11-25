import { useEffect, useRef, useState } from "react";
import type { MouseEvent, PointerEvent } from "react";
import { useLatestRef } from "../../app/useLatestRef";
import type { DropPlace } from "./reorder";

/** How far the pointer must travel before a press becomes a drag. */
const DRAG_THRESHOLD_PX = 4;

interface Press {
  readonly botId: string;
  readonly startY: number;
  readonly active: boolean;
}

export interface DropTarget {
  readonly botId: string;
  readonly place: DropPlace;
}

export interface RowDragHandlers {
  readonly onPointerDown: (event: PointerEvent<HTMLElement>) => void;
  readonly onPointerMove: (event: PointerEvent<HTMLElement>) => void;
  readonly onClickCapture: (event: MouseEvent<HTMLElement>) => void;
}

export interface BotDragApi {
  /** The bot being dragged, once the pointer has moved far enough. */
  readonly draggingId: string | null;
  readonly target: DropTarget | null;
  readonly rowHandlers: (botId: string) => RowDragHandlers;
}

/**
 * Pointer-driven dragging of the bot rows in one project.
 *
 * Built on pointer events rather than HTML5 drag and drop: the Tauri webview
 * claims native drops for files on Windows, and pointer events behave the
 * same in every webview and in tests. Rows only report which of them the
 * pointer is over; press, release and cancel are watched on the window so a
 * drag released outside the list ends cleanly.
 */
export function useBotDrag(
  enabled: boolean,
  onDrop: (botId: string, target: DropTarget) => void,
): BotDragApi {
  const [press, setPress] = useState<Press | null>(null);
  const [target, setTarget] = useState<DropTarget | null>(null);
  // A drag ends with a click on the row it was released over; that click
  // must not also select the bot.
  const swallowClick = useRef(false);
  const targetRef = useLatestRef(target);
  const onDropRef = useLatestRef(onDrop);

  useEffect(() => {
    if (press === null) {
      return undefined;
    }
    const end = (): void => {
      setPress(null);
      setTarget(null);
    };
    const onMove = (event: globalThis.PointerEvent): void => {
      if (!press.active && Math.abs(event.clientY - press.startY) > DRAG_THRESHOLD_PX) {
        setPress({ ...press, active: true });
      }
    };
    const onUp = (): void => {
      const dropOn = targetRef.current;
      if (press.active) {
        swallowClick.current = true;
        if (dropOn !== null) {
          onDropRef.current(press.botId, dropOn);
        }
      }
      end();
    };
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        end();
      }
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", end);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", end);
      window.removeEventListener("keydown", onKey);
    };
  }, [onDropRef, press, targetRef]);

  const rowHandlers = (botId: string): RowDragHandlers => ({
    onPointerDown: (event) => {
      swallowClick.current = false;
      if (enabled && event.button === 0) {
        setPress({ botId, startY: event.clientY, active: false });
      }
    },
    onPointerMove: (event) => {
      if (press === null || !press.active) {
        return;
      }
      if (botId === press.botId) {
        setTarget(null);
        return;
      }
      const rect = event.currentTarget.getBoundingClientRect();
      const place = event.clientY < rect.top + rect.height / 2 ? "before" : "after";
      if (target?.botId !== botId || target.place !== place) {
        setTarget({ botId, place });
      }
    },
    onClickCapture: (event) => {
      if (swallowClick.current) {
        swallowClick.current = false;
        event.stopPropagation();
        event.preventDefault();
      }
    },
  });

  return {
    draggingId: press?.active === true ? press.botId : null,
    target,
    rowHandlers,
  };
}
