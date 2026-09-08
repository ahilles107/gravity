import { useCallback, useState } from "react";
import { loadPinnedBotIds, savePinnedBotIds } from "../settings";

export interface PinnedBotsApi {
  /** Pinned bot ids in display order, oldest pin first. */
  readonly pinnedBotIds: readonly string[];
  readonly onTogglePin: (botId: string) => void;
}

/** Which bots sit in the pinned strip. A local view preference, not daemon state. */
export function usePinnedBots(): PinnedBotsApi {
  const [pinnedBotIds, setPinnedBotIds] = useState<readonly string[]>(loadPinnedBotIds);

  const onTogglePin = useCallback((botId: string): void => {
    setPinnedBotIds((prev) => {
      const next = prev.includes(botId) ? prev.filter((id) => id !== botId) : [...prev, botId];
      savePinnedBotIds(next);
      return next;
    });
  }, []);

  return { pinnedBotIds, onTogglePin };
}
