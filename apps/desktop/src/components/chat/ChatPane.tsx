import { useLayoutEffect, useRef } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { Bot } from "../../protocol/entities";
import ChatComposer from "./ChatComposer";
import TurnView from "./TurnView";
import { useChat } from "./useChat";
import { usePendingMessages } from "./usePendingMessages";

interface ChatPaneProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly connected: boolean;
  /** Why the owner cannot write to this bot, or null when they can. */
  readonly writeBlocked: string | null;
  readonly onOpenFile: (path: string) => void;
}

/** How close to the bottom still counts as following the conversation. */
const STICK_PX = 80;

/** A bot's conversation read from its transcript, with a composer. */
export default function ChatPane(props: ChatPaneProps): ReactElement {
  const { client, bot, connected } = props;
  const chat = useChat(client, bot.id, connected);
  const { pending, send } = usePendingMessages(client, bot.id, chat.turns);
  const scroller = useRef<HTMLDivElement | null>(null);
  const stick = useRef(true);

  // Follow the bottom while the owner is there; leave them be once they scroll up.
  useLayoutEffect(() => {
    const element = scroller.current;
    if (element !== null && stick.current) {
      element.scrollTop = element.scrollHeight;
    }
    // New content is the trigger, not an input: the effect only reads the DOM.
    // oxlint-disable-next-line react/exhaustive-effect-dependencies
  }, [chat.turns, pending]);

  const onScroll = (): void => {
    const element = scroller.current;
    if (element !== null) {
      stick.current = element.scrollHeight - element.scrollTop - element.clientHeight < STICK_PX;
    }
  };

  const empty = !chat.loading && chat.turns.length === 0 && pending.length === 0;
  return (
    <div className="chat-pane">
      <div className="chat-scroll" ref={scroller} onScroll={onScroll}>
        {chat.hasMore ? (
          <button
            type="button"
            className="btn btn-small chat-older"
            onClick={() => {
              stick.current = false;
              void chat.loadOlder();
            }}
          >
            Load earlier turns
          </button>
        ) : null}
        {chat.error === null ? null : <div className="chat-note chat-error">{chat.error}</div>}
        {empty ? (
          <div className="chat-empty">
            Nothing here yet. Messages you send, and everything {bot.name} does, show up here.
          </div>
        ) : null}
        {chat.turns.map((turn) => (
          <TurnView key={turn.id} client={client} turn={turn} onOpenFile={props.onOpenFile} />
        ))}
        {pending.map((message) => (
          <div key={message.id} className="chat-pending">
            <div className="chat-bubble chat-bubble-own">{message.text}</div>
            <div className="chat-pending-state">{pendingLabel(message.state)}</div>
          </div>
        ))}
      </div>
      <ChatComposer
        disabledReason={props.writeBlocked}
        placeholder={`Message ${bot.name}`}
        onSend={async (text) => {
          stick.current = true;
          await send(text);
        }}
      />
    </div>
  );
}

function pendingLabel(state: string): string {
  switch (state) {
    case "failed":
      return "Not delivered";
    case "delivered":
    case "acknowledged":
      return "Delivered — waiting for the bot to read it";
    default:
      return "Queued";
  }
}
