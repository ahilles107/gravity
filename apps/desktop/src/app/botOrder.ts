import type { Bot } from "../protocol/entities";

/**
 * The daemon's roster order: `sort_order`, then name. Names compare by code
 * unit, as SQLite's default collation does, so a pushed bot lands exactly
 * where a fresh `list_bots` would put it. A daemon that predates ordering
 * sends no `sort_order`, which leaves plain name order.
 */
function compareBots(a: Bot, b: Bot): number {
  const byOrder = (a.sort_order ?? 0) - (b.sort_order ?? 0);
  if (byOrder !== 0) {
    return byOrder;
  }
  if (a.name === b.name) {
    return 0;
  }
  return a.name < b.name ? -1 : 1;
}

/**
 * `sort` (not `toSorted`) because the build targets ES2021; it runs on a
 * fresh copy, so nothing shared is mutated.
 */
export function sortBots(bots: readonly Bot[]): readonly Bot[] {
  const sorted = [...bots];
  // oxlint-disable-next-line unicorn/no-array-sort
  sorted.sort(compareBots);
  return sorted;
}

/**
 * Applies a project's new order: `botIds` first, in that order, and the
 * project's other bots after them in the order they already had — the same
 * rule the daemon uses, so an optimistic update matches its reply.
 */
export function withBotOrder(
  bots: readonly Bot[],
  projectId: string,
  botIds: readonly string[],
): readonly Bot[] {
  const rest = sortBots(bots).filter(
    (bot) => bot.project_id === projectId && !botIds.includes(bot.id),
  );
  const order = [...botIds, ...rest.map((bot) => bot.id)];
  return sortBots(
    bots.map((bot) => {
      const at = order.indexOf(bot.id);
      return bot.project_id === projectId && at !== -1 ? { ...bot, sort_order: at } : bot;
    }),
  );
}
