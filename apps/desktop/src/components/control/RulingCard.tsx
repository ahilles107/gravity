import type { ReactElement } from "react";
import type { Decision } from "../../protocol/decisions";
import BotAvatar from "../BotAvatar";
import { fmtDay, fmtTime, hoursOpen, isRelayed } from "./decisions";

interface RulingCardProps {
  readonly decision: Decision;
  readonly canControl: boolean;
  readonly onConfirm: () => void;
}

function WithdrawnCard({ decision }: { readonly decision: Decision }): ReactElement {
  const reason = decision.withdrawn_reason?.trim() ?? "";
  return (
    <div className="cc-card">
      <div className="cc-card-label">Withdrawn</div>
      <div className="cc-ruling-words">
        {decision.raised_by.name} withdrew this
        {reason === "" ? undefined : ` — “${reason}”`}
      </div>
    </div>
  );
}

/**
 * The ruling as it stands in the record.
 *
 * The owner's words are quoted verbatim and the bots that were told are named,
 * because a decision nobody can trace back is the thing the registry exists to
 * replace. A relayed ruling says so until the owner confirms it.
 */
export default function RulingCard({
  decision,
  canControl,
  onConfirm,
}: RulingCardProps): ReactElement | null {
  if (decision.state === "withdrawn") {
    return <WithdrawnCard decision={decision} />;
  }

  const ruling = decision.ruling;
  if (ruling === undefined) {
    return null;
  }

  const ruledAt = decision.published_at ?? ruling.answered_at;
  const option = decision.options.find((item) => item.key === ruling.option);
  const hours = hoursOpen(decision);
  const told = decision.notifications ?? [];

  return (
    <div className="cc-card">
      <div className="cc-card-label">
        Ruled {fmtDay(ruledAt)} · {fmtTime(ruledAt)}
      </div>
      <div className="cc-ruling-words">“{ruling.text}”</div>
      {option === undefined ? undefined : (
        <div className="cc-option-pill">
          <span className="cc-option-key">{option.key}</span>
          <span>{option.label}</span>
        </div>
      )}
      {isRelayed(decision) ? (
        <div className="cc-relayed">
          <span>Recorded by {decision.raised_by.name} as something you said at its terminal.</span>
          <button
            type="button"
            className="cc-link cc-link-accent"
            disabled={!canControl}
            onClick={onConfirm}
          >
            Confirm this is what I said
          </button>
        </div>
      ) : undefined}
      <div className="cc-card-foot">
        Told
        {told.map((entry) => (
          <span key={entry.bot_id} className="cc-told">
            <BotAvatar size="sm" avatar="" name={entry.bot_name} id={entry.bot_id} />
            {entry.bot_name}
          </span>
        ))}
        {hours === undefined ? undefined : (
          <span className="cc-card-foot-right">open {hours}h</span>
        )}
      </div>
    </div>
  );
}
