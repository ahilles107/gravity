import type { Bot, Project } from "../../protocol/entities";
import { fuzzyScore } from "../../util";

/** A bot that matches the search query, with the project it lives in. */
export interface BotMatch {
  readonly bot: Bot;
  readonly projectName: string;
}

const MAX_BOTS = 6;

/**
 * Bots whose name fuzzy-matches the query, best first. Purely local: the bot
 * list is already in memory, so these appear without waiting on the daemon.
 *
 * `sort` (not `toSorted`) because the build targets ES2021; it runs on the
 * fresh array produced by `map`, so nothing shared is mutated.
 */
export function rankBots(
  bots: readonly Bot[],
  projects: readonly Project[],
  query: string,
): readonly BotMatch[] {
  if (query.length === 0) {
    return [];
  }
  const scored = bots
    .map((bot) => ({ bot, score: fuzzyScore(query, bot.name) }))
    .filter((item): item is { bot: Bot; score: number } => item.score !== null);
  // oxlint-disable-next-line unicorn/no-array-sort
  scored.sort((a, b) => b.score - a.score);
  return scored.slice(0, MAX_BOTS).map(({ bot }) => ({
    bot,
    projectName: projects.find((project) => project.id === bot.project_id)?.name ?? "",
  }));
}
