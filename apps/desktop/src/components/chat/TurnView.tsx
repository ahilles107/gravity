import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { ChatTurn } from "../../protocol/chat";
import { fmtTimestamp } from "../../util";
import ChatMarkdown from "./ChatMarkdown";
import { durationLabel, groupItems, statsLine, triggerView } from "./chatModel";
import type { ChatBlock } from "./chatModel";
import StepGroup from "./StepGroup";

interface TurnViewProps {
  readonly client: DaemonApi;
  readonly turn: ChatTurn;
  /** Opens a file a result names in the Files panel. */
  readonly onOpenFile: (path: string) => void;
}

/** One turn: what woke the bot, what it did, and how it ended. */
export default function TurnView({ client, turn, onOpenFile }: TurnViewProps): ReactElement {
  const trigger = triggerView(turn.trigger);
  const stats = statsLine(turn.stats);
  const meta = [fmtTimestamp(turn.started_at), durationLabel(turn.duration_ms)]
    .filter((part) => part !== "")
    .join(" · ");
  return (
    <article
      className={`chat-turn${turn.open ? " chat-turn-open" : ""}`}
      aria-label={trigger.label}
    >
      <header className={`chat-turn-head${trigger.own ? " chat-turn-head-own" : ""}`}>
        <span className="chat-turn-label">{trigger.label}</span>
        <span className="chat-turn-meta">{meta}</span>
      </header>
      {trigger.text === "" ? null : (
        <div className={trigger.own ? "chat-bubble chat-bubble-own" : "chat-bubble chat-bubble-in"}>
          {trigger.own ? trigger.text : <ChatMarkdown>{trigger.text}</ChatMarkdown>}
        </div>
      )}
      {groupItems(turn.items).map((block) => (
        <Block
          key={block.kind === "steps" ? block.id : block.item.id}
          client={client}
          botId={turn.bot_id}
          turnOpen={turn.open}
          block={block}
          onOpenFile={onOpenFile}
        />
      ))}
      {turn.open ? <div className="chat-working">Working…</div> : null}
      {stats === "" || turn.open ? null : <footer className="chat-turn-stats">{stats}</footer>}
    </article>
  );
}

interface BlockProps {
  readonly client: DaemonApi;
  readonly botId: string;
  readonly turnOpen: boolean;
  readonly block: ChatBlock;
  readonly onOpenFile: (path: string) => void;
}

function Block({ client, botId, turnOpen, block, onOpenFile }: BlockProps): ReactElement {
  if (block.kind === "steps") {
    return <StepGroup client={client} botId={botId} steps={block.steps} turnOpen={turnOpen} />;
  }
  const { item } = block;
  switch (item.type) {
    case "text":
      return (
        <div className="chat-text">
          <ChatMarkdown>{item.markdown}</ChatMarkdown>
        </div>
      );
    case "sent":
      return (
        <div className="chat-aside chat-sent">
          <span className="chat-aside-label">
            To {item.to} · {item.msg_kind}
          </span>
          <div className="chat-aside-body">{item.body}</div>
        </div>
      );
    case "completed":
      return (
        <div className="chat-card chat-completed">
          <div className="chat-card-label">Completed task</div>
          <ChatMarkdown>{item.result}</ChatMarkdown>
          {item.artifacts.length === 0 ? null : (
            <div className="chat-files">
              {item.artifacts.map((file) => (
                <button
                  key={file.path}
                  type="button"
                  className="chat-file-chip"
                  title={file.path}
                  onClick={() => {
                    onOpenFile(file.path);
                  }}
                >
                  {file.name}
                </button>
              ))}
            </div>
          )}
        </div>
      );
    case "decision":
      return (
        <div className="chat-card chat-decision">
          <div className="chat-card-label">Asked you to decide</div>
          <div>{item.title}</div>
        </div>
      );
    case "aside":
      return <div className={`chat-note chat-note-${item.kind}`}>{item.text}</div>;
    default:
      return item satisfies never;
  }
}
