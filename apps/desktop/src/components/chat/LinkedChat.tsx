import { useCallback, useEffect, useState } from "react";
import type { ReactElement } from "react";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { DaemonApi } from "../../protocol/api";
import type { Bot, BusMessage } from "../../protocol/entities";
import { errText, fmtTimestamp } from "../../util";
import ChatComposer from "./ChatComposer";
import ChatMarkdown from "./ChatMarkdown";

interface LinkedChatProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly connected: boolean;
  readonly writeBlocked: string | null;
}

/**
 * A bot that runs on a peer has no transcript here; what this machine knows
 * of it is its bus conversation, so that is what the chat shows.
 */
export default function LinkedChat({
  client,
  bot,
  connected,
  writeBlocked,
}: LinkedChatProps): ReactElement {
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [messages, setMessages] = useState<readonly BusMessage[]>([]);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (): Promise<void> => {
    try {
      const list = await client.request(
        { type: "list_conversations", project_id: bot.project_id },
        "conversations",
      );
      const conversation = list.conversations.find((c) => c.bot_id === bot.id);
      if (conversation === undefined) {
        return;
      }
      setConversationId(conversation.id);
      const reply = await client.request(
        { type: "list_messages", conversation_id: conversation.id, limit: 100 },
        "messages",
      );
      setMessages(reply.messages);
      setError(null);
    } catch (failure) {
      setError(errText(failure));
    }
  }, [client, bot.id, bot.project_id]);

  useLoadOnConnect(connected, load);

  useEffect(
    () =>
      client.on("message_new", (push) => {
        if (push.message.conversation_id === conversationId) {
          setMessages((current) => [...current, push.message]);
        }
      }),
    [client, conversationId],
  );

  const machine = bot.peer?.name ?? "another machine";
  return (
    <div className="chat-pane">
      <div className="chat-scroll">
        <div className="chat-note">
          {bot.name} runs on {machine}
          {bot.peer?.online === false ? " (offline)" : ""}. This is its conversation on the bus.
        </div>
        {error === null ? null : <div className="chat-note chat-error">{error}</div>}
        {messages.map((message) => {
          const own = message.sender.kind === "user";
          return (
            <div
              key={message.id}
              className={own ? "chat-message chat-message-own" : "chat-message"}
            >
              <div className="chat-turn-head">
                <span className="chat-turn-label">{own ? "You" : message.sender.name}</span>
                <span className="chat-turn-meta">
                  {message.kind} · {fmtTimestamp(message.created_at)}
                </span>
              </div>
              <div className={own ? "chat-bubble chat-bubble-own" : "chat-bubble chat-bubble-in"}>
                {own ? message.body : <ChatMarkdown>{message.body}</ChatMarkdown>}
              </div>
            </div>
          );
        })}
      </div>
      <ChatComposer
        disabledReason={writeBlocked}
        placeholder={`Message ${bot.name} on ${machine}`}
        onSend={async (text) => {
          await client.request(
            { type: "send_user_message", to_bot_id: bot.id, body: text },
            "message",
          );
        }}
      />
    </div>
  );
}
