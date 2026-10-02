import { useEffect } from "react";
import type { DaemonApi } from "../protocol/api";

/**
 * Calls `refresh` once a burst of `chat_turns` pushes for this bot settles:
 * the bot's turns changed, so whatever is read from its transcript did too.
 */
export function useOnBotTurns(
  client: DaemonApi,
  botId: string,
  refresh: () => Promise<void>,
  delayMs: number,
): void {
  useEffect(() => {
    let timer: number | undefined;
    const off = client.on("chat_turns", (push) => {
      if (push.bot_id !== botId) {
        return;
      }
      window.clearTimeout(timer);
      timer = window.setTimeout(() => void refresh(), delayMs);
    });
    return () => {
      window.clearTimeout(timer);
      off();
    };
  }, [client, botId, refresh, delayMs]);
}
