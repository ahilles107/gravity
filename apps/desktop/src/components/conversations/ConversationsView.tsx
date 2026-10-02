import { useState } from "react";
import type { ReactElement } from "react";
import type { AgentBot, AgentConversation } from "../../protocol/agents";
import type { DaemonApi } from "../../protocol/api";
import type { Bot, Project } from "../../protocol/entities";
import { fmtTimestamp } from "../../util";
import BotAvatar from "../BotAvatar";
import ConversationThread from "./ConversationThread";
import { botOf, pairKey, pairTitle, previewLine, sides } from "./conversationModel";
import { useAgentConversations } from "./useAgentConversations";

interface ConversationsViewProps {
  readonly client: DaemonApi;
  readonly project: Project;
  /** The project's bots in sidebar order, which decides each pair's sides. */
  readonly bots: readonly Bot[];
  readonly connected: boolean;
}

/** One pair in the list: both faces, who they are, and the latest word. */
function PairRow({
  conversation,
  bots,
  order,
  selected,
  onSelect,
}: {
  readonly conversation: AgentConversation;
  readonly bots: ReadonlyMap<string, AgentBot>;
  readonly order: readonly string[];
  readonly selected: boolean;
  readonly onSelect: () => void;
}): ReactElement {
  const [left, right] = sides(conversation.bot_ids, order);
  const face = (id: string): ReactElement => {
    const bot = botOf(bots, id);
    return <BotAvatar avatar={bot.avatar} name={bot.name} id={bot.id} size="sm" />;
  };
  return (
    <li>
      <button
        type="button"
        className={`conv-pair ${selected ? "conv-pair-selected" : ""}`}
        aria-current={selected ? "true" : undefined}
        onClick={onSelect}
      >
        <span className="conv-faces">
          {face(left)}
          {face(right)}
        </span>
        <span className="conv-pair-text">
          <span className="conv-pair-head">
            <span className="conv-pair-names">{pairTitle(bots, [left, right])}</span>
            <span className="conv-time">{fmtTimestamp(conversation.last_at)}</span>
          </span>
          <span className="conv-preview">{previewLine(bots, conversation)}</span>
        </span>
        <span className="conv-count" title="Messages">
          {conversation.message_count}
        </span>
      </button>
    </li>
  );
}

/**
 * What the project's bots have said to each other: the pairs that talked,
 * and each pair's messages as a chat, every bot in its own bubble.
 */
export default function ConversationsView({
  client,
  project,
  bots,
  connected,
}: ConversationsViewProps): ReactElement {
  const list = useAgentConversations(client, project.id, connected);
  const [picked, setPicked] = useState<string | null>(null);
  const order = bots.map((bot) => bot.id);
  const shown =
    list.conversations.find((c) => pairKey(c.bot_ids) === picked) ?? list.conversations[0];
  return (
    <div className="conversations-view">
      <nav className="conv-list" aria-label="Conversations">
        <h2 className="view-title conv-list-title">{`${project.name} · Conversations`}</h2>
        {list.error === null ? null : <div className="chat-note chat-error">{list.error}</div>}
        {list.loaded && list.conversations.length === 0 ? (
          <div className="muted conv-empty">No conversations between bots yet.</div>
        ) : null}
        <ul className="conv-pairs">
          {list.conversations.map((conversation) => {
            const key = pairKey(conversation.bot_ids);
            return (
              <PairRow
                key={key}
                conversation={conversation}
                bots={list.bots}
                order={order}
                selected={shown !== undefined && pairKey(shown.bot_ids) === key}
                onSelect={() => {
                  setPicked(key);
                }}
              />
            );
          })}
        </ul>
      </nav>
      {shown === undefined ? (
        <div className="conv-thread conv-empty muted">
          {list.loaded ? "When bots message each other, their conversations appear here." : ""}
        </div>
      ) : (
        <ConversationThread
          key={pairKey(shown.bot_ids)}
          client={client}
          projectId={project.id}
          pair={sides(shown.bot_ids, order)}
          knownBots={list.bots}
          connected={connected}
        />
      )}
    </div>
  );
}
