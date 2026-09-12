import type { ReactElement } from "react";
import type { Decision } from "../../protocol/decisions";
import NotifyChips from "./NotifyChips";
import type { NotifyCandidate } from "./publishPlan";

interface DraftCardProps {
  readonly decision: Decision;
  readonly candidates: readonly NotifyCandidate[];
  readonly chosen: ReadonlySet<string>;
  readonly onToggle: (botId: string) => void;
  readonly onEdit: () => void;
  readonly onPublish: () => void;
  readonly canControl: boolean;
}

/**
 * A ruling written but not sent.
 *
 * The dashed box and the "bots can't see this yet" note are the whole point:
 * nothing reaches a bot until the owner publishes, so a half-formed answer
 * cannot be acted on by mistake.
 */
export default function DraftCard({
  decision,
  candidates,
  chosen,
  onToggle,
  onEdit,
  onPublish,
  canControl,
}: DraftCardProps): ReactElement | null {
  const ruling = decision.ruling;
  if (ruling === undefined) {
    return null;
  }
  const option = decision.options.find((item) => item.key === ruling.option);

  return (
    <div className="cc-draft">
      <div className="cc-card-label">
        <span className="cc-card-label-accent">Draft ruling</span>
        <span className="cc-card-label-note">bots can&apos;t see this yet</span>
        <span className="cc-card-label-actions">
          <button type="button" className="cc-link" disabled={!canControl} onClick={onEdit}>
            Edit
          </button>
          <button
            type="button"
            className="cc-link cc-link-accent"
            disabled={!canControl}
            onClick={onPublish}
          >
            Publish…
          </button>
        </span>
      </div>
      <div className="cc-draft-words">“{ruling.text}”</div>
      {option === undefined ? undefined : (
        <div className="cc-draft-option">
          <span className="cc-option-key">{option.key}</span> {option.label}
        </div>
      )}
      <div className="cc-draft-tell">
        Will tell
        <NotifyChips
          candidates={candidates}
          chosen={chosen}
          onToggle={onToggle}
          disabled={!canControl}
        />
      </div>
    </div>
  );
}
