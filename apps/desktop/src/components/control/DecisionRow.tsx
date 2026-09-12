import type { ReactElement } from "react";
import type { Decision } from "../../protocol/decisions";
import BotAvatar from "../BotAvatar";
import { age, deadline, fmtDay, viaChain } from "./decisions";

interface DecisionRowProps {
  readonly decision: Decision;
  readonly selected: boolean;
  /** A parked decision: no urgency, no deadline, just when it comes back. */
  readonly held?: boolean;
  readonly now: number;
  readonly projectName: (projectId: string) => string;
  readonly botName: (botId: string) => string | undefined;
  readonly onSelect: () => void;
}

/** Who is waiting on the answer, and who they are waiting for. */
function askerLine(decision: Decision, botName: (botId: string) => string | undefined): string {
  const behalf =
    decision.on_behalf_of_bot_id === undefined
      ? ""
      : ` for ${botName(decision.on_behalf_of_bot_id) ?? "a deleted bot"}`;
  const via = viaChain(decision, botName);
  return `${decision.raised_by.name}${behalf}${via.length > 0 ? ` via ${via.join(" → ")}` : ""}`;
}

/** The right-hand column: when a held decision returns, or how long an open one has. */
function Side({
  decision,
  held,
  now,
}: {
  readonly decision: Decision;
  readonly held: boolean;
  readonly now: number;
}): ReactElement {
  if (held) {
    return (
      <span>until {decision.held_until === undefined ? "later" : fmtDay(decision.held_until)}</span>
    );
  }
  const due = deadline(decision.deadline_at, now);
  return (
    <>
      <span className="cc-row-deadline">
        {decision.state === "answered" ? (
          <span className="cc-badge cc-badge-draft">Draft</span>
        ) : null}
        {due === undefined ? null : <span className={`cc-tone-${due.tone}`}>{due.short}</span>}
      </span>
      <span className="cc-row-age">raised {age(decision.created_at, now)}</span>
    </>
  );
}

/** One line in the waiting list: what is being asked, by whom, and how long it has. */
export default function DecisionRow({
  decision,
  selected,
  held = false,
  now,
  projectName,
  botName,
  onSelect,
}: DecisionRowProps): ReactElement {
  const className = `cc-row ${selected ? "cc-row-selected" : ""} ${held ? "cc-row-held" : ""}`;

  return (
    <button
      type="button"
      className={className}
      aria-current={selected ? "true" : undefined}
      onClick={onSelect}
    >
      <span className="cc-row-main">
        <span className="cc-row-title-line">
          {decision.priority === "urgent" && !held ? (
            <span className="cc-badge cc-badge-urgent">Urgent</span>
          ) : null}
          <span className="cc-row-title">{decision.title}</span>
        </span>
        <span className="cc-row-meta">
          <BotAvatar
            size="sm"
            avatar={decision.raised_by.avatar}
            name={decision.raised_by.name}
            id={decision.raised_by.bot_id}
          />
          <span>{askerLine(decision, botName)}</span>
          {held ? null : (
            <>
              <span className="cc-byline-sep">·</span>
              <span>{projectName(decision.project_id)}</span>
            </>
          )}
        </span>
      </span>
      <span className="cc-row-side">
        <Side decision={decision} held={held} now={now} />
      </span>
    </button>
  );
}
