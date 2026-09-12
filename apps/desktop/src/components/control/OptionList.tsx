import type { ReactElement } from "react";
import type { Decision, DecisionOption } from "../../protocol/decisions";

interface OptionListProps {
  readonly decision: Decision;
  /** The composer's current pick, before anything is filed. */
  readonly picked?: string;
  readonly onPick?: (key: string) => void;
  readonly now?: number;
}

/** "3 options · auction recommends start" while it is still the owner's to answer. */
function heading(decision: Decision): string {
  if (decision.state !== "open") {
    return "Options offered";
  }
  const count = `${decision.options.length} options`;
  if (decision.recommendation === undefined) {
    return count;
  }
  return `${count} · ${decision.raised_by.name} recommends ${decision.recommendation}`;
}

function rowClass(decision: Decision, option: DecisionOption, picked?: string): string {
  const open = decision.state === "open";
  const chosen = !open && decision.ruling?.option === option.key;
  const classes = ["cc-option"];
  if (open) {
    classes.push("cc-option-pickable");
    if (picked === option.key) {
      classes.push("cc-option-picked");
    }
  } else {
    classes.push(chosen ? "cc-option-chosen" : "cc-option-passed");
  }
  return classes.join(" ");
}

/**
 * What the bot offered.
 *
 * The rows stay visible after a ruling so the record shows what was on the
 * table, not just what was taken; the ones that were passed over are dimmed
 * rather than dropped.
 */
export default function OptionList({
  decision,
  picked,
  onPick,
}: OptionListProps): ReactElement | null {
  if (decision.options.length === 0) {
    return null;
  }
  const open = decision.state === "open";

  return (
    <div className="cc-options">
      <div className="cc-section-label">{heading(decision)}</div>
      <div className="cc-options-list">
        {decision.options.map((option, index) => {
          const chosen = !open && decision.ruling?.option === option.key;
          const inner = (
            <>
              <div className="cc-option-main">
                <span className="cc-option-key">{option.key}</span>
                <div className="cc-option-head">
                  <span className="cc-option-label">{option.label}</span>
                  {open && decision.recommendation === option.key ? (
                    <span className="cc-option-rec">recommended</span>
                  ) : undefined}
                  {chosen ? <span className="cc-option-chosen-mark">chosen</span> : undefined}
                </div>
                {option.description === undefined ? undefined : (
                  <div className="cc-option-desc">{option.description}</div>
                )}
              </div>
              {open ? <span className="cc-option-num">{index + 1}</span> : undefined}
            </>
          );
          const className = rowClass(decision, option, picked);
          return open ? (
            <button
              key={option.key}
              type="button"
              className={className}
              onClick={() => onPick?.(option.key)}
            >
              {inner}
            </button>
          ) : (
            <div key={option.key} className={className}>
              {inner}
            </div>
          );
        })}
      </div>
    </div>
  );
}
