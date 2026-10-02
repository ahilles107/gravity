import type { ReactElement } from "react";
import type { AgentBot, AgentMessage } from "../../protocol/agents";
import type { DaemonApi } from "../../protocol/api";
import { fmtTimestamp } from "../../util";
import BotAvatar from "../BotAvatar";
import ChatMarkdown from "../chat/ChatMarkdown";
import { botOf, kindLabel, pairTitle } from "./conversationModel";
import { useAgentThread } from "./useAgentConversations";

/** One message: the sender's avatar beside its own bubble, on its side. */
function MessageBubble({
  message,
  sender,
  side,
}: {
  readonly message: AgentMessage;
  readonly sender: AgentBot;
  readonly side: "left" | "right";
}): ReactElement {
  return (
    <li className={`conv-message conv-${side}`}>
      <BotAvatar avatar={sender.avatar} name={sender.name} id={sender.id} size="md" />
      <div className="conv-bubble">
        <div className="conv-meta">
          <span className="conv-sender">
            {sender.machine == null ? sender.name : `${sender.name} @ ${sender.machine}`}
          </span>
          <span className={`conv-kind conv-kind-${message.kind}`}>{kindLabel(message.kind)}</span>
          {message.task == null ? null : (
            <span className={`task-badge task-badge-${message.task.state}`}>
              {message.task.state}
            </span>
          )}
          <span className="conv-time">{fmtTimestamp(message.created_at)}</span>
        </div>
        <div className="conv-body">
          <ChatMarkdown>{message.body}</ChatMarkdown>
        </div>
      </div>
    </li>
  );
}

/** The pair's messages, oldest at the top, as a chat between the two. */
export function ThreadMessages({
  messages,
  bots,
  left,
}: {
  readonly messages: readonly AgentMessage[];
  readonly bots: ReadonlyMap<string, AgentBot>;
  readonly left: string;
}): ReactElement {
  return (
    <ol className="conv-messages">
      {messages.map((message) => (
        <MessageBubble
          key={message.id}
          message={message}
          sender={botOf(bots, message.from_bot_id)}
          side={message.from_bot_id === left ? "left" : "right"}
        />
      ))}
    </ol>
  );
}

interface ConversationThreadProps {
  readonly client: DaemonApi;
  readonly projectId: string;
  /** [left, right]. */
  readonly pair: readonly [string, string];
  /** Names known from the list, until the thread brings its own. */
  readonly knownBots: ReadonlyMap<string, AgentBot>;
  readonly connected: boolean;
}

/** One pair's conversation, live. Mount it keyed by pair. */
export default function ConversationThread({
  client,
  projectId,
  pair,
  knownBots,
  connected,
}: ConversationThreadProps): ReactElement {
  const thread = useAgentThread(client, projectId, pair, connected);
  const bots = new Map([...knownBots, ...thread.bots]);
  return (
    <section className="conv-thread" aria-label={pairTitle(bots, pair)}>
      <header className="conv-thread-head">
        <h2 className="view-title">{pairTitle(bots, pair)}</h2>
      </header>
      <div className="conv-scroll">
        {thread.hasMore ? (
          <button type="button" className="btn btn-small conv-older" onClick={thread.loadOlder}>
            Load earlier messages
          </button>
        ) : null}
        {thread.error === null ? null : <div className="chat-note chat-error">{thread.error}</div>}
        <ThreadMessages messages={thread.messages} bots={bots} left={pair[0]} />
      </div>
    </section>
  );
}
