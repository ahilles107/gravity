import type { ReactElement } from "react";
import type { NotifyCandidate } from "./publishPlan";

interface NotifyChipsProps {
  readonly candidates: readonly NotifyCandidate[];
  readonly chosen: ReadonlySet<string>;
  readonly onToggle: (botId: string) => void;
  readonly disabled?: boolean;
}

/**
 * Who a ruling will be sent to.
 *
 * The asking bot's chip is locked on rather than absent: the owner should see
 * that it is being told, and see that this is not theirs to turn off.
 */
export default function NotifyChips({
  candidates,
  chosen,
  onToggle,
  disabled = false,
}: NotifyChipsProps): ReactElement {
  return (
    <>
      {candidates.map((candidate) => {
        const on = chosen.has(candidate.botId);
        const className = `cc-chip${on ? " cc-chip-on" : ""}${candidate.locked ? " cc-chip-locked" : ""}`;
        return (
          <button
            key={candidate.botId}
            type="button"
            className={className}
            title={candidate.title}
            aria-disabled={candidate.locked || undefined}
            disabled={disabled}
            onClick={() => {
              if (!candidate.locked) {
                onToggle(candidate.botId);
              }
            }}
          >
            {candidate.name}
          </button>
        );
      })}
    </>
  );
}
