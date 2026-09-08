import { useEffect } from "react";
import type { MutableRefObject } from "react";
import type { DaemonApi } from "../protocol/api";
import type { Conversation } from "../protocol/entities";
import type { Selection } from "./selection";
import type { EntitiesApi } from "./useEntities";
import { useLatestRef } from "./useLatestRef";
import type { AddToast } from "./useToasts";
import type { UnreadApi } from "./useUnread";

export interface PushDeps {
  readonly client: DaemonApi;
  readonly addToast: AddToast;
  readonly entities: EntitiesApi;
  readonly unread: UnreadApi;
  readonly selection: Selection;
  readonly findConversation: (conversationId: string) => Conversation | undefined;
  readonly botName: (botId: string) => string;
  readonly onSelectBot: (botId: string) => void;
  /** Refetches the sidebar preview lines; a finished turn changes them. */
  readonly refreshActivity: () => void;
}

/** True when new activity on `botId` happened out of the user's sight. */
function isUnreadWorthy(botId: string, selection: Selection): boolean {
  return selection.kind !== "bot" || selection.botId !== botId;
}

/**
 * Counts one item for a bot, as a badge when the user is looking elsewhere and
 * as read otherwise. Either way the bot's read cursor moves past it, so the
 * same item cannot raise a badge again after a reload.
 */
function noteItem(deps: PushDeps, botId: string, at: string): void {
  if (isUnreadWorthy(botId, deps.selection)) {
    deps.unread.bumpBot(botId, at);
  } else {
    deps.unread.markSeen(botId, at);
  }
}

/**
 * Subscribes every push handler against a ref, so handlers always see the
 * current render's state without resubscribing on each render.
 */
function subscribe(ref: MutableRefObject<PushDeps>): Array<() => void> {
  const { client } = ref.current;
  return [
    client.on("bot_state", (push) => {
      ref.current.entities.applyBotState(push.bot_id, push.state, push.reason);
    }),
    client.on("bot_updated", (push) => {
      ref.current.entities.applyBotUpdate(push.bot);
    }),
    client.on("project_updated", (push) => {
      ref.current.entities.applyProjectUpdate(push.project);
    }),
    // The daemon sends this once a finished turn is readable in the transcript.
    // Reacting to `bot_state` going ready instead would race that write and
    // leave the preview showing the previous turn.
    client.on("activity_update", (push) => {
      const deps = ref.current;
      deps.entities.applyBotActivity(push.activity);
      // A bot's own turn only ever surfaces here: its terminal never touches
      // the bus, so this is the badge signal for everything a bot says. A
      // preview attributed to someone else restates a bus message that
      // `message_new` already counted.
      if (push.activity.from === "") {
        noteItem(deps, push.activity.bot_id, push.activity.at);
      }
    }),
    client.on("message_new", (push) => {
      const deps = ref.current;
      const conversation = deps.findConversation(push.message.conversation_id);
      if (conversation === undefined) {
        return;
      }
      // The user's own message is never unread, but it still moves the cursor.
      if (push.message.sender.kind === "user") {
        deps.unread.advanceSeen(conversation.bot_id, push.message.created_at);
      } else {
        noteItem(deps, conversation.bot_id, push.message.created_at);
      }
      deps.refreshActivity();
    }),
    client.on("delivery_update", (push) => {
      ref.current.entities.applyDelivery(push.delivery);
    }),
    client.on("approval_pending", (push) => {
      const deps = ref.current;
      deps.addToast("warn", `${deps.botName(push.bot_id)} is waiting for approval`, push.detail, {
        action: {
          label: "View",
          run: () => {
            deps.onSelectBot(push.bot_id);
          },
        },
      });
    }),
    client.on("notify", (push) => {
      ref.current.addToast(push.level, push.title, push.body);
    }),
  ];
}

/**
 * Wires every server push into the state hooks. The subscription is made once:
 * the client instance lives as long as the app, and everything that varies is
 * read through `ref` at push time.
 */
export function useDaemonPushes(deps: PushDeps): void {
  const ref = useLatestRef(deps);

  useEffect(() => {
    const unsubs = subscribe(ref);
    return () => {
      for (const unsub of unsubs) {
        unsub();
      }
    };
  }, [ref]);
}
