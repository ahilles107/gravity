import { useCallback, useState } from "react";
import { defaultNotify } from "./publishPlan";
import type { NotifyCandidate } from "./publishPlan";

export interface NotifySets {
  /** Who a draft will tell: what the owner chose, else the candidates' defaults. */
  readonly get: (decisionId: string) => ReadonlySet<string>;
  readonly toggle: (decisionId: string, botId: string) => void;
}

/**
 * Per-draft notify sets, kept at view level so they survive moving through the
 * list. The tray and the draft box read the same set.
 */
export function useNotifySets(
  candidatesFor: (decisionId: string) => readonly NotifyCandidate[],
): NotifySets {
  const [chosen, setChosen] = useState<ReadonlyMap<string, ReadonlySet<string>>>(new Map());

  const get = useCallback(
    (decisionId: string): ReadonlySet<string> =>
      chosen.get(decisionId) ?? defaultNotify(candidatesFor(decisionId)),
    [candidatesFor, chosen],
  );

  const toggle = useCallback(
    (decisionId: string, botId: string): void => {
      const candidates = candidatesFor(decisionId);
      if (candidates.find((item) => item.botId === botId)?.locked === true) {
        return;
      }
      setChosen((prev) => {
        const current = prev.get(decisionId) ?? defaultNotify(candidates);
        const next = new Set(current);
        if (!next.delete(botId)) {
          next.add(botId);
        }
        return new Map(prev).set(decisionId, next);
      });
    },
    [candidatesFor],
  );

  return { get, toggle };
}
