import type { ReactElement } from "react";
import type { Decision } from "../../protocol/decisions";
import BotAvatar from "../BotAvatar";
import DecisionThread from "./DecisionThread";
import DraftCard from "./DraftCard";
import HeldCard from "./HeldCard";
import Markdown from "./Markdown";
import OptionList from "./OptionList";
import RulingCard from "./RulingCard";
import { age, deadline, fmtDay, fmtTime, viaChain } from "./decisions";
import type { NotifyCandidate } from "./publishPlan";

export interface DecisionReaderProps {
  readonly decision: Decision;
  readonly now: number;
  readonly canControl: boolean;
  /** The Settled tab reaches the reader from a list it can go back to. */
  readonly showBack: boolean;
  readonly onBack: () => void;
  readonly showQuestion: boolean;
  readonly onToggleQuestion: () => void;
  readonly picked?: string;
  readonly onPick: (key: string) => void;
  readonly projectName: (projectId: string) => string;
  readonly botName: (botId: string) => string | undefined;
  readonly botAvatar: (botId: string) => { avatar: string; name: string } | undefined;
  readonly candidates: readonly NotifyCandidate[];
  readonly chosen: ReadonlySet<string>;
  readonly onToggleNotify: (botId: string) => void;
  readonly onEditDraft: () => void;
  readonly onOpenTray: () => void;
  readonly onResume: () => void;
  readonly onConfirm: () => void;
  readonly onDelete: () => void;
}

type BylineProps = Pick<DecisionReaderProps, "decision" | "now" | "botName" | "projectName">;

function Byline(props: BylineProps): ReactElement {
  const { decision, now, botName, projectName } = props;
  const via = viaChain(decision, botName);
  const behalf = decision.on_behalf_of_bot_id;

  return (
    <div className="cc-byline">
      <BotAvatar
        size="md"
        avatar={decision.raised_by.avatar}
        name={decision.raised_by.name}
        id={decision.raised_by.bot_id}
      />
      <span className="cc-byline-name">{decision.raised_by.name}</span>
      {behalf === undefined ? undefined : (
        <span>
          for <span className="cc-byline-name">{botName(behalf) ?? "a deleted bot"}</span>
        </span>
      )}
      {via.length === 0 ? undefined : <span>via {via.join(" → ")}</span>}
      <span className="cc-byline-sep">·</span>
      <span>{projectName(decision.project_id)}</span>
      <span className="cc-byline-sep">·</span>
      <span title={`${fmtDay(decision.created_at)} ${fmtTime(decision.created_at)}`}>
        raised {age(decision.created_at, now)}
      </span>
      <span className="cc-byline-tags">
        {decision.tags.map((name) => (
          <span key={name} className="cc-tag">
            {name}
          </span>
        ))}
      </span>
    </div>
  );
}

/** The urgency badge and the deadline sentence, both only while it is open. */
function TitleMeta({
  decision,
  now,
}: {
  readonly decision: Decision;
  readonly now: number;
}): ReactElement | null {
  const open = decision.state === "open";
  const urgent = open && decision.priority === "urgent";
  const due = open ? deadline(decision.deadline_at, now) : undefined;
  const long = due?.long;
  if (!urgent && long === undefined) {
    return null;
  }
  return (
    <div className="cc-title-meta">
      {urgent ? <span className="cc-badge cc-badge-urgent">Urgent</span> : undefined}
      {long === undefined || due === undefined ? undefined : (
        <span className={`cc-tone-${due.tone}`}>{long}</span>
      )}
    </div>
  );
}

type StateBlockProps = Pick<
  DecisionReaderProps,
  | "decision"
  | "canControl"
  | "onConfirm"
  | "candidates"
  | "chosen"
  | "onToggleNotify"
  | "onEditDraft"
  | "onOpenTray"
  | "onResume"
>;

function StateBlock(props: StateBlockProps): ReactElement | null {
  const { decision } = props;
  switch (decision.state) {
    case "settled":
    case "withdrawn": {
      return (
        <RulingCard decision={decision} canControl={props.canControl} onConfirm={props.onConfirm} />
      );
    }
    case "answered": {
      return (
        <DraftCard
          decision={decision}
          candidates={props.candidates}
          chosen={props.chosen}
          onToggle={props.onToggleNotify}
          onEdit={props.onEditDraft}
          onPublish={props.onOpenTray}
          canControl={props.canControl}
        />
      );
    }
    case "held": {
      return (
        <HeldCard decision={decision} canControl={props.canControl} onResume={props.onResume} />
      );
    }
    default: {
      return null;
    }
  }
}

/**
 * One decision, read.
 *
 * A ruled decision leads with the ruling and folds the question away: what the
 * owner needs back from the record months later is the answer, and the ask
 * that produced it is one click below it rather than in front of it.
 */
export default function DecisionReader(props: DecisionReaderProps): ReactElement {
  const { decision, now, canControl, showQuestion } = props;
  const ruled = decision.state === "settled" || decision.state === "withdrawn";
  const showBody = !ruled || showQuestion;

  return (
    <article className="cc-article">
      {props.showBack ? (
        <button type="button" className="cc-link cc-back" onClick={props.onBack}>
          ‹ Back <span>esc</span>
        </button>
      ) : undefined}

      <Byline
        decision={decision}
        now={now}
        botName={props.botName}
        projectName={props.projectName}
      />
      <h1 className="cc-title">{decision.title}</h1>
      <TitleMeta decision={decision} now={now} />
      <StateBlock
        decision={decision}
        canControl={canControl}
        onConfirm={props.onConfirm}
        candidates={props.candidates}
        chosen={props.chosen}
        onToggleNotify={props.onToggleNotify}
        onEditDraft={props.onEditDraft}
        onOpenTray={props.onOpenTray}
        onResume={props.onResume}
      />

      {ruled ? (
        <button
          type="button"
          className="cc-link cc-question-toggle"
          onClick={props.onToggleQuestion}
        >
          <span className={`cc-chevron${showQuestion ? " cc-chevron-open" : ""}`}>›</span>
          <span>The question as {decision.raised_by.name} put it</span>
        </button>
      ) : undefined}

      {showBody ? (
        <>
          <div className="cc-body">
            <Markdown>{decision.body}</Markdown>
          </div>
          <OptionList decision={decision} picked={props.picked} onPick={props.onPick} />
        </>
      ) : undefined}

      <DecisionThread comments={decision.comments ?? []} now={now} botAvatar={props.botAvatar} />

      {canControl ? (
        <button type="button" className="cc-link cc-delete" onClick={props.onDelete}>
          Delete record
        </button>
      ) : undefined}
    </article>
  );
}
