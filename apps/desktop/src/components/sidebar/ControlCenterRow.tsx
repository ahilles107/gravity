import type { ReactElement } from "react";
import type { PendingCounts } from "../../protocol/decisions";

interface ControlCenterRowProps {
  readonly counts: PendingCounts;
  readonly selected: boolean;
  readonly onSelect: () => void;
}

/**
 * The one place a question from any bot in any project is visible.
 *
 * Only urgency turns the row red: this is the last row that can raise an
 * alarm, so it has to mean something when it does.
 */
export default function ControlCenterRow({
  counts,
  selected,
  onSelect,
}: ControlCenterRowProps): ReactElement {
  const urgent = counts.urgent > 0;
  const tooltip =
    urgent || counts.due_soon > 0
      ? `${counts.urgent} urgent, ${counts.due_soon} due within a day`
      : "Decisions waiting on you";

  return (
    <div className="sidebar-control">
      <button
        type="button"
        className={`row control-row ${selected ? "row-selected" : ""}`}
        title={tooltip}
        onClick={onSelect}
      >
        <span className="control-row-ring" />
        <span className="control-row-name">Control center</span>
        {counts.total > 0 ? (
          <span className="control-row-count">
            {urgent ? <span className="control-row-dot" /> : null}
            <span className={`control-row-pill ${urgent ? "control-row-pill-urgent" : ""}`}>
              {counts.total}
            </span>
          </span>
        ) : null}
      </button>
    </div>
  );
}
