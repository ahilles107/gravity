import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { Bot } from "../../protocol/entities";
import ChatComposer from "./ChatComposer";
import TurnView from "./TurnView";
import { typeIntoTerminal, uploadAttachment } from "./upload";
import { useChat } from "./useChat";
import { useChatSearch } from "./useChatSearch";
import { usePendingMessages } from "./usePendingMessages";
import { useStickToBottom } from "./useStickToBottom";
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

/** A bot's conversation read from its transcript, with a composer. */
export default function ChatPane(props: ChatPaneProps): ReactElement {
  const { client, bot, connected } = props;
  const linked = bot.peer != null;
  const chat = useChat(client, bot.id, connected);
  const { pending, send } = usePendingMessages(client, bot.id, chat.turns);
  const { ref: scroller, onScroll, follow, release } = useStickToBottom(chat.turns, pending);
  const search = useChatSearch(props.active ?? true, scroller, chat.turns);
  // While searching, folded steps open so their text can be found.
  const searching = search.open && search.query.trim() !== "";

  const onSend = async (text: string): Promise<void> => {
    follow();
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
      {search.open ? <ChatSearchBar search={search} hasMore={chat.hasMore} /> : null}
      <div className="chat-scroll" ref={scroller} onScroll={onScroll}>
        {props.note === undefined ? null : <div className="chat-note">{props.note}</div>}
        {chat.hasMore ? (
          <button
            type="button"
            className="btn btn-small chat-older"
            onClick={() => {
              release();
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
          <TurnView
            key={turn.id}
            client={client}
            turn={turn}
            connected={connected}
            onOpenFile={props.onOpenFile}
            onOpenDecision={props.onOpenDecision}
            expandAll={searching}
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
