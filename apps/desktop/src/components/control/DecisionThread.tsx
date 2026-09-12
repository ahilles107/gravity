import type { ReactElement } from "react";
import type { DecisionComment } from "../../protocol/decisions";
import BotAvatar from "../BotAvatar";
import { age } from "./decisions";

interface DecisionThreadProps {
  readonly comments: readonly DecisionComment[];
  readonly now: number;
  readonly botAvatar: (botId: string) => { avatar: string; name: string } | undefined;
}

/**
 * The clarification exchange, read back.
 *
 * "Tell me more about the Backblaze caps first" belongs on the decision rather
 * than in a DM: the question and the answer stay attached to the record
 * instead of scrolling out of a conversation nobody rereads.
 */
export default function DecisionThread({
  comments,
  now,
  botAvatar,
}: DecisionThreadProps): ReactElement | null {
  if (comments.length === 0) {
    return null;
  }

  return (
    <div className="cc-thread">
      <div className="cc-section-label">Thread</div>
      {comments.map((comment) => {
        const id = comment.author_bot_id ?? comment.author_name;
        const bot =
          comment.author_bot_id === undefined ? undefined : botAvatar(comment.author_bot_id);
        return (
          <div key={comment.id} className="cc-thread-item">
            {comment.author_kind === "user" ? (
              <span className="cc-thread-owner">●</span>
            ) : (
              <BotAvatar
                size="md"
                avatar={bot?.avatar ?? ""}
                name={bot?.name ?? comment.author_name}
                id={id}
              />
            )}
            <div>
              <div className="cc-thread-who">
                <strong>{comment.author_name}</strong> · {age(comment.created_at, now)}
              </div>
              <div className="cc-thread-text">{comment.body}</div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
