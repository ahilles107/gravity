import { useCallback, useMemo, useState } from "react";
import type { Bot, BotActivity } from "../protocol/entities";
import type { UnreadEntry, UnreadState } from "../settings";
import { loadUnread, saveUnread } from "../settings";
import type { Selection } from "./selection";

type Counters = Readonly<Record<string, number>>;

export interface UnreadApi {
  readonly bots: Counters;
  /**
   * Reconciles the stored badges against a fresh snapshot. A bot whose newest
   * activity postdates what we last accounted for gets one badge, however much
   * it said while the client was closed; a bot we have never seen starts read.
   */
  readonly syncBots: (bots: readonly Bot[], activity: readonly BotActivity[]) => void;
  /** Counts one new item for a bot the user is not looking at. */
  readonly bumpBot: (botId: string, at: string) => void;
  /** Accounts for an item the user is already looking at, raising no badge. */
  readonly markSeen: (botId: string, at: string) => void;
  /** Accounts for an item that is never unread, leaving any badge standing. */
  readonly advanceSeen: (botId: string, at: string) => void;
  /** Clears the badge for whatever the given selection points at. */
  readonly clearFor: (selection: Selection) => void;
}

const UNSEEN: UnreadEntry = { count: 0, seenAt: 0 };

/** Epoch milliseconds of an RFC 3339 stamp; 0 when it cannot be read. */
function stamp(at: string): number {
  const ms = Date.parse(at);
  return Number.isNaN(ms) ? 0 : ms;
}

function counters(state: UnreadState): Counters {
  const counts: Record<string, number> = {};
  for (const [botId, entry] of Object.entries(state)) {
    if (entry.count > 0) {
      counts[botId] = entry.count;
    }
  }
  return counts;
}

export function useUnread(): UnreadApi {
  const [state, setState] = useState<UnreadState>(loadUnread);

  const update = useCallback((change: (prev: UnreadState) => UnreadState): void => {
    setState((prev) => {
      const next = change(prev);
      saveUnread(next);
      return next;
    });
  }, []);

  const syncBots = useCallback(
    (bots: readonly Bot[], activity: readonly BotActivity[]): void => {
      const latest = new Map(activity.map((item) => [item.bot_id, stamp(item.at)]));
      update((prev) => {
        // Rebuilt rather than merged, so badges for archived bots do not linger.
        const next: Record<string, UnreadEntry> = {};
        for (const bot of bots) {
          const before = prev[bot.id];
          const at = latest.get(bot.id) ?? 0;
          if (before === undefined) {
            next[bot.id] = { count: 0, seenAt: at };
          } else if (at > before.seenAt) {
            next[bot.id] = { count: before.count + 1, seenAt: at };
          } else {
            next[bot.id] = before;
          }
        }
        return next;
      });
    },
    [update],
  );

  /** Moves one bot's cursor to `seenAt` and sets its badge from `nextCount`. */
  const apply = useCallback(
    (botId: string, seenAt: number, nextCount: (before: number) => number): void => {
      update((prev) => {
        const before = prev[botId] ?? UNSEEN;
        return {
          ...prev,
          [botId]: { count: nextCount(before.count), seenAt: Math.max(before.seenAt, seenAt) },
        };
      });
    },
    [update],
  );

  const bumpBot = useCallback(
    (botId: string, at: string): void => {
      apply(botId, stamp(at), (count) => count + 1);
    },
    [apply],
  );

  const markSeen = useCallback(
    (botId: string, at: string): void => {
      apply(botId, stamp(at), () => 0);
    },
    [apply],
  );

  const advanceSeen = useCallback(
    (botId: string, at: string): void => {
      apply(botId, stamp(at), (count) => count);
    },
    [apply],
  );

  const clearFor = useCallback(
    (selection: Selection): void => {
      if (selection.kind !== "bot") {
        return;
      }
      apply(selection.botId, Date.now(), () => 0);
    },
    [apply],
  );

  const bots = useMemo(() => counters(state), [state]);

  return { bots, syncBots, bumpBot, markSeen, advanceSeen, clearFor };
}
