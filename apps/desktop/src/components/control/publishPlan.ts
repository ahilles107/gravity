// Who gets told when a ruling is published.

import type { PublishItem } from "../../protocol/decisionRequests";
import type { Decision } from "../../protocol/decisions";
import type { Bot } from "../../protocol/entities";

/** A bot the notify chips offer, and how it starts. */
export interface NotifyCandidate {
  readonly botId: string;
  readonly name: string;
  readonly checked: boolean;
  /** The asking bot is always told; its chip cannot be turned off. */
  readonly locked: boolean;
  /** Why it is offered, for the chip's tooltip. */
  readonly title: string;
}

/**
 * The chips' starting state.
 *
 * The asker is locked on and whoever the ask was raised on behalf of is
 * checked, because they cannot act without the answer. The project lead is
 * checked too — a lead whose record goes stale is what the hand-kept ledgers
 * were compensating for. Every other bot in the project is offered unticked.
 * The daemon sends to exactly the ids it is given, so the locked chip is what
 * guarantees the asker hears.
 */
export function notifyCandidates(
  decision: Decision,
  bots: readonly Bot[],
  leadBotId: string | undefined,
): readonly NotifyCandidate[] {
  const out: NotifyCandidate[] = [];
  const seen = new Set<string>();
  const offer = (botId: string, checked: boolean, locked: boolean, title: string): void => {
    if (seen.has(botId)) {
      return;
    }
    const bot = bots.find((item) => item.id === botId);
    if (bot === undefined || (bot.deleted_at !== undefined && bot.deleted_at !== null)) {
      return;
    }
    seen.add(botId);
    out.push({ botId, name: bot.name, checked, locked, title });
  };

  offer(decision.raised_by.bot_id, true, true, "The asking bot is always told");
  if (decision.on_behalf_of_bot_id !== undefined) {
    offer(decision.on_behalf_of_bot_id, true, false, "Waiting on this answer");
  }
  if (leadBotId !== undefined) {
    offer(leadBotId, true, false, "Project lead");
  }
  for (const bot of bots) {
    if (bot.project_id === decision.project_id) {
      offer(bot.id, false, false, "");
    }
  }
  return out;
}

/** The ids that start checked, for a notify set that has not been touched. */
export function defaultNotify(candidates: readonly NotifyCandidate[]): ReadonlySet<string> {
  return new Set(candidates.filter((item) => item.checked).map((item) => item.botId));
}

/** Build the batch payload from the drafts and their notify sets. */
export function buildPublishItems(
  drafts: readonly Decision[],
  notify: (decisionId: string) => ReadonlySet<string>,
): readonly PublishItem[] {
  return drafts
    .filter((decision) => (decision.ruling?.text ?? "").trim().length > 0)
    .map((decision) => ({
      decision_id: decision.id,
      notify_bot_ids: [...notify(decision.id)],
    }));
}
