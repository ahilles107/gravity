import { useLayoutEffect, useRef } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { Bot } from "../../protocol/entities";
import ChatComposer from "./ChatComposer";
import TurnView from "./TurnView";
import { typeIntoTerminal, uploadAttachment } from "./upload";
import { useChat } from "./useChat";
import { matchTurns, useChatSearch } from "./useChatSearch";
import { usePendingMessages } from "./usePendingMessages";
import { ChatSearchBar, PendingMessages } from "./ChatPaneParts";

interface ChatPaneProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly connected: boolean;
  /** Whether the chat is the tab on screen, for its keyboard shortcuts. */
  readonly active?: boolean;
  /** Why the owner cannot write to this bot, or null when they can. */
  readonly writeBlocked: string | null;
  readonly onOpenFile: (path: string) => void;
  readonly onOpenDecision?: (decisionId: string) => void;
  /** A line above the conversation, e.g. which machine a linked bot runs on. */
  readonly note?: string;
}

/** How close to the bottom still counts as following the conversation. */
const STICK_PX = 80;

/** A bot's conversation read from its transcript, with a composer. */
export default function ChatPane(props: ChatPaneProps): ReactElement {
  const { client, bot, connected } = props;
  const linked = bot.peer != null;
  const chat = useChat(client, bot.id, connected);
  const { pending, send } = usePendingMessages(client, bot.id, chat.turns);
  const search = useChatSearch(props.active ?? true);
  const scroller = useRef<HTMLDivElement | null>(null);
  const stick = useRef(true);
  const shown = matchTurns(chat.turns, search.query);

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

  const onSend = async (text: string): Promise<void> => {
    stick.current = true;
    // A slash command is the terminal's: type it there, as the owner would.
    if (text.startsWith("/") && !linked) {
      typeIntoTerminal(client, bot.id, text);
      return;
    }
    await send(text);
  };

  const empty = !chat.loading && chat.turns.length === 0 && pending.length === 0;
  return (
    <div className="chat-pane">
      {search.open ? (
        <ChatSearchBar
          search={search}
          shown={shown.length}
          total={chat.turns.length}
          hasMore={chat.hasMore}
        />
      ) : null}
      <div className="chat-scroll" ref={scroller} onScroll={onScroll}>
        {props.note === undefined ? null : <div className="chat-note">{props.note}</div>}
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
        {shown.map((turn) => (
          <TurnView
            key={turn.id}
            client={client}
            turn={turn}
            connected={connected}
            onOpenFile={props.onOpenFile}
            onOpenDecision={props.onOpenDecision}
          />
        ))}
        <PendingMessages pending={pending} />
      </div>
      <ChatComposer
        disabledReason={props.writeBlocked}
        placeholder={`Message ${bot.name}`}
        slashHint={linked ? null : "Runs in the terminal, as if you typed it there."}
        onAttach={linked ? undefined : (file) => uploadAttachment(client, bot.project_id, file)}
        onSend={onSend}
      />
    </div>
  );
}
