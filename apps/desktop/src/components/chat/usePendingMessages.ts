import { useEffect, useState } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { ChatTurn } from "../../protocol/chat";
import type { DeliveryState } from "../../protocol/entities";

export interface PendingMessage {
  readonly id: string;
  readonly text: string;
  readonly state: DeliveryState | "sending";
}

/** Owner messages already in the transcript, by text. */
function delivered(turns: readonly ChatTurn[]): ReadonlySet<string> {
  return new Set(
    turns
      .map((turn) => turn.trigger)
      .filter((trigger) => trigger.kind === "owner" && trigger.via === "chat")
      .map((trigger) => (trigger.kind === "owner" ? trigger.text.trim() : "")),
  );
}

/**
 * Messages the owner sent that the transcript does not show yet. A message
 * reaches the transcript only once the bot reads it, which can be minutes
 * while it works, so until then the pane shows it with its delivery state.
 */
export function usePendingMessages(
  client: DaemonApi,
  botId: string,
  turns: readonly ChatTurn[],
): {
  readonly pending: readonly PendingMessage[];
  readonly send: (text: string) => Promise<void>;
} {
  const [sent, setSent] = useState<readonly PendingMessage[]>([]);

  useEffect(
    () =>
      client.on("delivery_update", (push) => {
        setSent((current) =>
          current.map((message) =>
            message.id === push.delivery.message_id
              ? { ...message, state: push.delivery.state }
              : message,
          ),
        );
      }),
    [client],
  );

  const send = async (text: string): Promise<void> => {
    const reply = await client.request(
      { type: "send_user_message", to_bot_id: botId, body: text },
      "message",
    );
    setSent((current) => [...current, { id: reply.message.id, text, state: "queued" }]);
  };

  const seen = delivered(turns);
  const pending = sent.filter((message) => !seen.has(message.text.trim()));
  return { pending, send };
}
