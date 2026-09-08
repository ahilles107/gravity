import { useCallback } from "react";
import { captureException } from "../analytics";
import type { DaemonApi } from "../protocol/api";
import type { ConnectionStatus, Endpoint } from "../protocol/connection";
import type {
  Bot,
  BotActivity,
  Conversation,
  Delivery,
  Project,
  Routine,
} from "../protocol/entities";
import { errText } from "../util";
import type { Selection } from "./selection";
import { fetchActivity, fetchSnapshot } from "./snapshot";
import { useConnection } from "./useConnection";
import { useDaemonPushes } from "./useDaemonPushes";
import { useEntities } from "./useEntities";
import { useLatestRef } from "./useLatestRef";
import { useSelection } from "./useSelection";
import type { AddToast } from "./useToasts";
import { useUnread } from "./useUnread";

export interface DaemonState {
  readonly status: ConnectionStatus;
  readonly endpoint: Endpoint;
  readonly serverVersion: string;
  readonly canControl: boolean;
  readonly connected: boolean;
  readonly projects: readonly Project[];
  readonly bots: readonly Bot[];
  readonly conversations: readonly Conversation[];
  readonly failedDeliveries: readonly Delivery[];
  readonly unreadBots: Readonly<Record<string, number>>;
  readonly nextRun: Readonly<Record<string, string>>;
  readonly activityByBot: Readonly<Record<string, BotActivity>>;
  readonly selection: Selection;
  readonly select: (next: Selection) => void;
  readonly changeEndpoint: (next: Endpoint) => void;
  readonly refreshAll: () => Promise<void>;
  readonly updateBotRoutines: (botId: string, routines: readonly Routine[]) => void;
  readonly applyBotUpdate: (bot: Bot) => void;
  readonly applyProjectUpdate: (project: Project) => void;
  readonly findConversation: (conversationId: string) => Conversation | undefined;
}

/**
 * Everything the daemon connection produces, composed from focused hooks:
 * connection status, entity lists, unread counters and the current selection,
 * kept in sync by the initial snapshot plus server pushes.
 */
export function useDaemonState(client: DaemonApi, addToast: AddToast): DaemonState {
  const entities = useEntities();
  const unread = useUnread();
  const { selection, selectionRef, select, selectBot } = useSelection(
    entities.projects,
    entities.bots,
    unread,
  );

  const conversationsRef = useLatestRef(entities.conversations);
  const botsRef = useLatestRef(entities.bots);

  const refreshAll = useCallback(async (): Promise<void> => {
    try {
      const snapshot = await fetchSnapshot(client);
      entities.applySnapshot(snapshot);
      unread.syncBots(snapshot.bots, snapshot.activity);
      // Whatever the open bot said while the connection was down is already on
      // screen, so it must not come back as a badge.
      unread.clearFor(selectionRef.current);
    } catch (error) {
      captureException(error, "daemon_state_load");
      addToast("error", "Failed to load daemon state", errText(error));
    }
    // Both apis expose stable callbacks.
  }, [addToast, client, entities, selectionRef, unread]);

  const refreshRef = useLatestRef(refreshAll);
  const onUp = useCallback((): void => {
    void refreshRef.current();
  }, [refreshRef]);

  const connection = useConnection(client, onUp);

  const findConversation = useCallback(
    (conversationId: string): Conversation | undefined =>
      conversationsRef.current.find((item) => item.id === conversationId),
    [conversationsRef],
  );

  const botName = useCallback(
    (botId: string): string => botsRef.current.find((item) => item.id === botId)?.name ?? botId,
    [botsRef],
  );

  const refreshActivity = useCallback((): void => {
    void fetchActivity(client).then(entities.applyActivity);
    // `applyActivity` is stable.
  }, [client, entities.applyActivity]);

  useDaemonPushes({
    client,
    addToast,
    entities,
    unread,
    selection,
    findConversation,
    botName,
    onSelectBot: selectBot,
    refreshActivity,
  });

  return {
    status: connection.status,
    endpoint: connection.endpoint,
    serverVersion: connection.serverVersion,
    canControl: connection.canControl,
    connected: connection.connected,
    changeEndpoint: connection.changeEndpoint,
    projects: entities.projects,
    bots: entities.bots,
    conversations: entities.conversations,
    failedDeliveries: entities.failedDeliveries,
    nextRun: entities.nextRun,
    activityByBot: entities.activityByBot,
    applyBotUpdate: entities.applyBotUpdate,
    applyProjectUpdate: entities.applyProjectUpdate,
    updateBotRoutines: entities.setBotRoutines,
    unreadBots: unread.bots,
    selection,
    select,
    refreshAll,
    findConversation,
  };
}
