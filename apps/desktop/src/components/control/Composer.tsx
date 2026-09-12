import type { ReactElement, RefObject } from "react";
import type { Decision } from "../../protocol/decisions";
import type { Composer as ComposerState } from "./useComposer";

interface ComposerProps {
  readonly decision: Decision;
  readonly composer: ComposerState;
  readonly canControl: boolean;
  readonly busy: boolean;
  readonly textareaRef: RefObject<HTMLTextAreaElement>;
  readonly onSave: () => void;
  readonly onAsk: () => void;
  readonly onHold: () => void;
}

/**
 * Where the ruling is written.
 *
 * Picking an option is never enough on its own: the bots quote the words, so
 * the button stays dead until there are some.
 */
export default function Composer({
  decision,
  composer,
  canControl,
  busy,
  textareaRef,
  onSave,
  onAsk,
  onHold,
}: ComposerProps): ReactElement {
  const picked = decision.options.find((option) => option.key === composer.pick);
  const asker = decision.raised_by.name;
  const placeholder =
    picked === undefined
      ? `${asker} will quote this verbatim. Pick an option, write, or both.`
      : `Why ${picked.key}, or any conditions (optional). ${asker} will quote this verbatim.`;
  const hasWords = composer.text.trim().length > 0;
  // A picked option is a ruling on its own: its label becomes the words.
  const ready = hasWords || picked !== undefined;

  return (
    <div className="cc-composer">
      <div className="cc-composer-inner">
        <div className="cc-composer-box">
          {picked === undefined ? null : (
            <div className="cc-pick">
              <span className="cc-pick-chip">
                <span className="cc-option-key">{picked.key}</span>
                {picked.label}
              </span>
              <button type="button" className="cc-link cc-pick-clear" onClick={composer.clearPick}>
                clear
              </button>
            </div>
          )}
          <textarea
            ref={textareaRef}
            rows={4}
            value={composer.text}
            onChange={(event) => composer.setText(event.target.value)}
            placeholder={placeholder}
            disabled={!canControl}
            aria-label="Your ruling"
          />
        </div>

        {composer.holdOpen ? (
          <div className="cc-hold">
            <span className="cc-hold-label">Hold until</span>
            <input
              type="date"
              className="cc-hold-date"
              aria-label="Hold until"
              value={composer.holdDate}
              onChange={(event) => composer.setHoldDate(event.target.value)}
            />
            <input
              className="cc-hold-note"
              placeholder="What you want to know first (optional; sent to the bot)"
              value={composer.holdNote}
              onChange={(event) => composer.setHoldNote(event.target.value)}
            />
            <button
              type="button"
              className="cc-hold-btn"
              disabled={!canControl || busy}
              onClick={onHold}
            >
              Hold
            </button>
          </div>
        ) : null}

        <div className="cc-composer-actions">
          <button
            type="button"
            className={`cc-link ${hasWords ? "cc-link-ready" : ""}`}
            disabled={!canControl || busy || !hasWords}
            onClick={onAsk}
          >
            Ask in thread
          </button>
          <button
            type="button"
            className="cc-link"
            onClick={() => composer.setHoldOpen(!composer.holdOpen)}
          >
            Later
          </button>
          <span className="cc-composer-spacer" />
          <button
            type="button"
            className="cc-btn-rule"
            disabled={!canControl || busy || !ready}
            onClick={onSave}
          >
            Save ruling <span className="cc-hint">⌘↩</span>
          </button>
        </div>
      </div>
    </div>
  );
}
