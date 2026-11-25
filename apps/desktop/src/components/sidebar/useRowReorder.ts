import { useLayoutEffect, useRef, useState } from "react";
import type { RefObject } from "react";
import type { Bot } from "../../protocol/entities";
import { moveBot, sameOrder, stepBot } from "./reorder";
import { useBotDrag } from "./useBotDrag";
import type { BotDragApi } from "./useBotDrag";

interface RowReorderOptions {
  readonly projectId: string;
  /** Every bot in the project, pinned ones included, in roster order. */
  readonly bots: readonly Bot[];
  /** The plain rows, which are the ones that can be dragged and stepped. */
  readonly rows: readonly Bot[];
  readonly canControl: boolean;
  readonly onReorderBots: (projectId: string, botIds: readonly string[]) => void;
}

export interface RowReorderApi {
  readonly drag: BotDragApi;
  /** The one-step move for a row, or null when it is already at that end. */
  readonly mover: (botId: string, delta: -1 | 1) => (() => void) | null;
  /** The section the rows live in, so a moved row can take focus back. */
  readonly sectionRef: RefObject<HTMLElement>;
}

/**
 * Every way of rearranging one project's rows — dragging, the keyboard and
 * the context menu — resolved to one new roster order for the daemon.
 *
 * Moving a row re-parents its button, which drops keyboard focus, so a
 * keyboard move hands focus back to the row once the new order renders.
 */
export function useRowReorder({
  projectId,
  bots,
  rows,
  canControl,
  onReorderBots,
}: RowReorderOptions): RowReorderApi {
  const order = bots.map((bot) => bot.id);
  const visible = rows.map((bot) => bot.id);
  const sectionRef = useRef<HTMLElement>(null);
  // A fresh object per move, so stepping the same row twice refocuses twice.
  const [focusRequest, setFocusRequest] = useState<{ readonly botId: string } | null>(null);

  const commit = (next: readonly string[]): void => {
    if (!sameOrder(next, order)) {
      onReorderBots(projectId, next);
    }
  };

  const drag = useBotDrag(canControl && rows.length > 1, (botId, target) => {
    commit(moveBot(order, botId, target.botId, target.place));
  });

  useLayoutEffect(() => {
    if (focusRequest === null) {
      return;
    }
    sectionRef.current
      ?.querySelector<HTMLElement>(`[data-bot-id="${focusRequest.botId}"]`)
      ?.focus();
  }, [focusRequest]);

  const mover = (botId: string, delta: -1 | 1): (() => void) | null => {
    if (!canControl) {
      return null;
    }
    const next = stepBot(order, visible, botId, delta);
    if (next === null) {
      return null;
    }
    return () => {
      setFocusRequest({ botId });
      commit(next);
    };
  };

  return { drag, mover, sectionRef };
}
