import { useCallback, useState } from "react";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import { useOnBotTurns } from "../../hooks/useOnBotTurns";
import type { BrowserAction } from "../../protocol/agents";
import type { DaemonApi } from "../../protocol/api";
import { errText } from "../../util";

/** Waits this long after the bot's last turn update before refetching. */
const REFRESH_DELAY_MS = 600;

export interface BrowserActivity {
  /** Newest first. */
  readonly items: readonly BrowserAction[];
  readonly error: string | null;
}

/** The bot's browser actions, refetched as its turns change. */
export function useBrowserActivity(
  client: DaemonApi,
  botId: string,
  connected: boolean,
): BrowserActivity {
  const [items, setItems] = useState<readonly BrowserAction[]>([]);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request(
        { type: "list_browser_activity", bot_id: botId },
        "browser_activity",
      );
      setItems(reply.activity);
      setError(null);
    } catch (failure) {
      setError(errText(failure));
    }
  }, [client, botId]);

  useLoadOnConnect(connected, load);

  useOnBotTurns(client, botId, load, REFRESH_DELAY_MS);

  return { items, error };
}
