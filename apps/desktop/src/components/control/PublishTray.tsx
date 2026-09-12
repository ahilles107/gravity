import type { ReactElement } from "react";
import type { Decision } from "../../protocol/decisions";
import NotifyChips from "./NotifyChips";
import type { NotifyCandidate } from "./publishPlan";

interface PublishTrayProps {
  readonly drafts: readonly Decision[];
  readonly candidatesFor: (decisionId: string) => readonly NotifyCandidate[];
  readonly chosenFor: (decisionId: string) => ReadonlySet<string>;
  readonly onToggle: (decisionId: string, botId: string) => void;
  readonly onEdit: (decisionId: string) => void;
  readonly onPublish: () => void;
  readonly onClose: () => void;
  readonly busy: boolean;
  readonly canControl: boolean;
}

/** "start · Start today", when the ruling picked one of the offered options. */
function optionLine(decision: Decision): string | undefined {
  const picked = decision.options.find((option) => option.key === decision.ruling?.option);
  return picked === undefined ? undefined : `${picked.key} · ${picked.label}`;
}

/**
 * Every ruling written but not yet sent, published in one go.
 *
 * A draft is invisible to the bots until this, so the tray is the last point
 * at which a ruling can be changed — and the note says so.
 */
export default function PublishTray({
  drafts,
  candidatesFor,
  chosenFor,
  onToggle,
  onEdit,
  onPublish,
  onClose,
  busy,
  canControl,
}: PublishTrayProps): ReactElement {
  return (
    <div className="cc-tray">
      <div className="cc-tray-title">
        <strong>Ready to publish</strong>
        <span>Bots cannot see these yet. Pick who is told about each, then publish once.</span>
      </div>

      <div className="cc-tray-items">
        {drafts.map((decision) => {
          const option = optionLine(decision);
          return (
            <div key={decision.id} className="cc-tray-item">
              <div className="cc-tray-item-head">
                <div className="cc-tray-item-body">
                  <div className="cc-tray-item-title">
                    <strong>{decision.title}</strong>
                    {option === undefined ? null : (
                      <span className="cc-tray-item-option">{option}</span>
                    )}
                  </div>
                  <div className="cc-tray-item-words">“{decision.ruling?.text ?? ""}”</div>
                </div>
                <button
                  type="button"
                  className="cc-link cc-tray-edit"
                  onClick={() => onEdit(decision.id)}
                >
                  Edit
                </button>
              </div>
              <div className="cc-tray-tell">
                <span>Tell</span>
                <NotifyChips
                  candidates={candidatesFor(decision.id)}
                  chosen={chosenFor(decision.id)}
                  onToggle={(botId) => onToggle(decision.id, botId)}
                  disabled={busy || !canControl}
                />
              </div>
            </div>
          );
        })}
      </div>

      <div className="cc-tray-actions">
        <button
          type="button"
          className="cc-btn-green"
          disabled={busy || !canControl || drafts.length === 0}
          onClick={onPublish}
        >
          Publish {drafts.length} {drafts.length === 1 ? "ruling" : "rulings"}
        </button>
        <button type="button" className="cc-btn-outline" onClick={onClose}>
          Not yet
        </button>
        <span className="cc-tray-note">
          Rulings are quoted verbatim. Nothing here can be unsaid once published.
        </span>
      </div>
    </div>
  );
}
