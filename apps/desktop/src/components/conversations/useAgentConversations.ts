import { useCallback, useEffect, useRef, useState } from "react";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { AgentBot, AgentConversation, AgentMessage } from "../../protocol/agents";
import type { DaemonApi } from "../../protocol/api";
import { errText } from "../../util";
import { mergeMessages } from "./conversationModel";

/** Waits this long after a burst of bus traffic before refetching. */
const REFRESH_DELAY_MS = 600;
const PAGE = 50;

/** Calls `refresh` once a burst of bus messages settles. */
function useOnBusTraffic(client: DaemonApi, refresh: () => void): void {
  useEffect(() => {
    let timer: number | undefined;
    const off = client.on("message_new", (push) => {
      if (push.message.sender.kind !== "bot") {
        return;
      }
      window.clearTimeout(timer);
      timer = window.setTimeout(refresh, REFRESH_DELAY_MS);
    });
    return () => {
      window.clearTimeout(timer);
      off();
    };
  }, [client, refresh]);
}

function botMap(
  bots: readonly AgentBot[],
  into = new Map<string, AgentBot>(),
): Map<string, AgentBot> {
  for (const bot of bots) {
    into.set(bot.id, bot);
  }
  return into;
}

export interface Conversations {
  readonly conversations: readonly AgentConversation[];
  readonly bots: ReadonlyMap<string, AgentBot>;
  readonly loaded: boolean;
  readonly error: string | null;
}

/** The pairs of bots in a project that have talked, kept current. */
export function useAgentConversations(
  client: DaemonApi,
  projectId: string,
  connected: boolean,
): Conversations {
  const [state, setState] = useState<Conversations>({
    conversations: [],
    bots: new Map(),
    loaded: false,
    error: null,
  });
  const load = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request(
        { type: "list_agent_conversations", project_id: projectId },
        "agent_conversations",
      );
      setState({
        conversations: reply.conversations,
        bots: botMap(reply.bots),
        loaded: true,
        error: null,
      });
    } catch (failure) {
      setState((prev) => ({ ...prev, loaded: true, error: errText(failure) }));
    }
  }, [client, projectId]);
  useLoadOnConnect(connected, load);
  useOnBusTraffic(
    client,
    useCallback(() => void load(), [load]),
  );
  return state;
}

export interface Thread {
  /** Oldest first. */
  readonly messages: readonly AgentMessage[];
  readonly bots: ReadonlyMap<string, AgentBot>;
  readonly hasMore: boolean;
  readonly error: string | null;
  readonly loadOlder: () => void;
}

/**
 * One pair's messages: the newest page, kept current as they talk, and
 * older pages on request. Mount it keyed by pair.
 */
export function useAgentThread(
  client: DaemonApi,
  projectId: string,
  pair: readonly [string, string],
  connected: boolean,
): Thread {
  const [messages, setMessages] = useState<readonly AgentMessage[]>([]);
  const [bots, setBots] = useState<ReadonlyMap<string, AgentBot>>(new Map());
  const [hasMore, setHasMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Set once the first page is in: later refreshes of the newest page do not know what precedes them. */
  const started = useRef(false);
  const [a, b] = pair;

  const fetchPage = useCallback(
    async (before: number | undefined): Promise<void> => {
      try {
        const reply = await client.request(
          {
            type: "list_agent_conversation",
            project_id: projectId,
            bot_ids: [a, b],
            limit: PAGE,
            ...(before === undefined ? {} : { before }),
          },
          "agent_conversation",
        );
        setMessages((loaded) => mergeMessages(loaded, reply.messages));
        setBots((known) => botMap(reply.bots, new Map(known)));
        // Only the oldest page loaded knows whether anything precedes it.
        if (before !== undefined || !started.current) {
          started.current = true;
          setHasMore(reply.has_more);
        }
        setError(null);
      } catch (failure) {
        setError(errText(failure));
      }
    },
    [client, projectId, a, b],
  );

  const loadNewest = useCallback(() => fetchPage(undefined), [fetchPage]);
  useLoadOnConnect(connected, loadNewest);
  useOnBusTraffic(
    client,
    useCallback(() => void loadNewest(), [loadNewest]),
  );

  const loadOlder = useCallback((): void => {
    const oldest = messages[0];
    if (oldest !== undefined) {
      void fetchPage(oldest.num);
    }
  }, [messages, fetchPage]);

  return { messages, bots, hasMore, error, loadOlder };
}
