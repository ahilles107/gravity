import { useCallback } from "react";
import type { Bot } from "../../protocol/entities";
import { notifyCandidates } from "./publishPlan";
import type { NotifyCandidate } from "./publishPlan";
import type { DecisionsApi } from "./useDecisions";

export interface BotLookups {
  /** Who a decision's notify chips offer. */
  readonly candidatesFor: (decisionId: string) => readonly NotifyCandidate[];
  readonly botAvatar: (botId: string) => { avatar: string; name: string } | undefined;
}

/** Bot-shaped answers the reader, the tray and the thread all need. */
export function useBotLookups(
  bots: readonly Bot[],
  api: DecisionsApi,
  leadFor: (projectId: string) => string | undefined,
): BotLookups {
  const candidatesFor = useCallback(
    (decisionId: string): readonly NotifyCandidate[] => {
      const decision = api.byId.get(decisionId);
      if (decision === undefined) {
        return [];
      }
      return notifyCandidates(decision, bots, leadFor(decision.project_id));
    },
    [api.byId, bots, leadFor],
  );

  const botAvatar = useCallback(
    (botId: string): { avatar: string; name: string } | undefined => {
      const bot = bots.find((item) => item.id === botId);
      if (bot === undefined) {
        return undefined;
      }
      return { avatar: bot.avatar, name: bot.name };
    },
    [bots],
  );

  return { candidatesFor, botAvatar };
}
