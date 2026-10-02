import { useCallback, useEffect, useRef, useState } from "react";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import { errText } from "../../util";
import type { DaemonApi } from "../../protocol/api";
import type { ChatTurn } from "../../protocol/chat";
import { mergeTurns } from "./chatModel";

/** Turns fetched per page: enough to fill the pane and scroll a little. */
const PAGE = 30;

export interface ChatState {
  readonly turns: readonly ChatTurn[];
  readonly hasMore: boolean;
  readonly loading: boolean;
  readonly error: string | null;
  readonly loadOlder: () => Promise<void>;
}

/**
 * A bot's chat: the newest page on every connect, then the daemon's
 * `chat_turns` pushes as the bot works, and older pages on demand. The caller
 * keys it by bot, so a different bot is a fresh hook.
 */
export function useChat(client: DaemonApi, botId: string, connected: boolean): ChatState {
  const [turns, setTurns] = useState<readonly ChatTurn[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const oldestRef = useRef<string | undefined>(undefined);
  const pagingRef = useRef(false);

  const load = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request({ type: "list_chat", bot_id: botId, limit: PAGE }, "chat");
      setTurns((current) => mergeTurns(current, reply.turns));
      setHasMore(reply.has_more);
      oldestRef.current ??= reply.turns[0]?.id;
      setError(null);
    } catch (failure) {
      setError(errText(failure));
    } finally {
      setLoading(false);
    }
  }, [client, botId]);

  useLoadOnConnect(connected, load);

  useEffect(
    () =>
      client.on("chat_turns", (push) => {
        if (push.bot_id === botId) {
          setTurns((current) => mergeTurns(current, push.turns));
        }
      }),
    [client, botId],
  );

  const loadOlder = useCallback(async (): Promise<void> => {
    const before = oldestRef.current;
    if (before === undefined || pagingRef.current) {
      return;
    }
    pagingRef.current = true;
    try {
      const reply = await client.request(
        { type: "list_chat", bot_id: botId, before, limit: PAGE },
        "chat",
      );
      setTurns((current) => mergeTurns(current, reply.turns));
      setHasMore(reply.has_more);
      oldestRef.current = reply.turns[0]?.id ?? before;
    } catch (failure) {
      setError(errText(failure));
    } finally {
      pagingRef.current = false;
    }
  }, [client, botId]);

  return { turns, hasMore, loading, error, loadOlder };
}
