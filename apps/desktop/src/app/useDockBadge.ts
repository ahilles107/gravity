import { useEffect } from "react";
import { setDockBadge } from "../badge";

/** Total of every bot's unread counter. */
export function totalUnread(unreadBots: Readonly<Record<string, number>>): number {
  let total = 0;
  for (const count of Object.values(unreadBots)) {
    total += count;
  }
  return total;
}

/**
 * Mirrors what wants the owner onto the dock icon whenever it moves.
 *
 * Pending decisions count alongside unread messages: a bot waiting on a ruling
 * is asking for attention just as much as one that has said something, and the
 * whole point of the registry is that such a question is never invisible.
 */
export function useDockBadge(
  unreadBots: Readonly<Record<string, number>>,
  pendingDecisions: number,
  enabled: boolean,
): void {
  const total = enabled ? totalUnread(unreadBots) + pendingDecisions : 0;
  useEffect(() => {
    void setDockBadge(total);
  }, [total]);
}
