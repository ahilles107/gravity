import type { ReactElement } from "react";
import type { Decision } from "../../protocol/decisions";
import { fmtDay } from "./decisions";

interface HeldCardProps {
  readonly decision: Decision;
  readonly canControl: boolean;
  readonly onResume: () => void;
}

/**
 * A decision the owner parked.
 *
 * The asking bot was told it was parked, so the line names it: holding is an
 * answer of sorts, and the bot is not left waiting on silence.
 */
export default function HeldCard({ decision, canControl, onResume }: HeldCardProps): ReactElement {
  const until = decision.held_until;

  return (
    <div className="cc-held">
      {until === undefined ? (
        <span>Held. {decision.raised_by.name} was told.</span>
      ) : (
        <span>
          Held until <strong>{fmtDay(until)}</strong>. {decision.raised_by.name} was told.
        </span>
      )}
      <button
        type="button"
        className="cc-link cc-link-accent"
        disabled={!canControl}
        onClick={onResume}
      >
        Take it up now
      </button>
    </div>
  );
}
