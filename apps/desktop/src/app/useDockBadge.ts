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

/** Mirrors the unread total onto the dock icon whenever it moves. */
export function useDockBadge(unreadBots: Readonly<Record<string, number>>, enabled: boolean): void {
  const total = enabled ? totalUnread(unreadBots) : 0;
  useEffect(() => {
    void setDockBadge(total);
  }, [total]);
}
