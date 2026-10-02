// Pure helpers for the Conversations view: naming pairs, merging pages, and
// which side of the thread each bot sits on.

import type { AgentBot, AgentConversation, AgentMessage } from "../../protocol/agents";

/** A stable key for a pair, whatever order its ids come in. */
export function pairKey([a, b]: readonly [string, string]): string {
  return a < b ? `${a}|${b}` : `${b}|${a}`;
}

/** Merges a fetched page into what is loaded: one copy of each, by number. */
export function mergeMessages(
  loaded: readonly AgentMessage[],
  page: readonly AgentMessage[],
): readonly AgentMessage[] {
  const byId = new Map<string, AgentMessage>();
  for (const message of [...loaded, ...page]) {
    byId.set(message.id, message);
  }
  const merged = [...byId.values()];
  // `sort` on a fresh copy, not `toSorted`: the build targets ES2021.
  // oxlint-disable-next-line unicorn/no-array-sort
  merged.sort((a, b) => a.num - b.num);
  return merged;
}

/**
 * The pair as [left, right]: the bot higher in the project's list sits on
 * the left, so a lead keeps its side however far back the thread is read.
 * Bots no longer listed (archived) go after those that are.
 */
export function sides(
  ids: readonly [string, string],
  order: readonly string[],
): readonly [string, string] {
  const rank = (id: string): number => {
    const index = order.indexOf(id);
    return index === -1 ? Number.MAX_SAFE_INTEGER : index;
  };
  const [a, b] = ids;
  return rank(b) < rank(a) ? [b, a] : [a, b];
}

/** A bot by id, or a placeholder for one the daemon did not describe. */
export function botOf(bots: ReadonlyMap<string, AgentBot>, id: string): AgentBot {
  return bots.get(id) ?? { id, name: "unknown bot", avatar: "", deleted: true };
}

/** "lead ↔ windev @ win-pc". */
export function pairTitle(
  bots: ReadonlyMap<string, AgentBot>,
  [left, right]: readonly [string, string],
): string {
  const name = (id: string): string => {
    const bot = botOf(bots, id);
    return bot.machine == null ? bot.name : `${bot.name} @ ${bot.machine}`;
  };
  return `${name(left)} ↔ ${name(right)}`;
}

/** The list's preview line: who said the last thing, and what. */
export function previewLine(
  bots: ReadonlyMap<string, AgentBot>,
  conversation: AgentConversation,
): string {
  const last = conversation.last;
  if (last === null) {
    return "";
  }
  return `${botOf(bots, last.from_bot_id).name}: ${last.body.replace(/\s+/g, " ").trim()}`;
}

const KIND_LABEL: Readonly<Record<AgentMessage["kind"], string>> = {
  task: "task",
  reply: "reply",
  done: "result",
  note: "note",
  chat: "message",
};

export function kindLabel(kind: AgentMessage["kind"]): string {
  return KIND_LABEL[kind];
}
