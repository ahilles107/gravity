import { useCallback, useLayoutEffect, useRef, useState } from "react";
import type { ReactElement } from "react";
import type { Bot, Routine } from "../../protocol/entities";
import { fmtTimestamp } from "../../util";
import { triggerSummary } from "./triggers";

interface RoutineItemProps {
  readonly routine: Routine;
  readonly bots: readonly Bot[];
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly expanded: boolean;
  readonly onToggleEnabled: () => void;
  readonly onRunNow: () => void;
  readonly onToggleHistory: () => void;
  readonly children?: ReactElement | null;
}

function nextRunLabel(routine: Routine): string {
  const at = routine.next_run_at;
  return typeof at === "string" && at.length > 0 ? `next ${fmtTimestamp(at)}` : "no next run";
}

export default function RoutineItem(props: RoutineItemProps): ReactElement {
  const { routine, bots, connected, canControl, expanded } = props;
  const promptRef = useRef<HTMLDivElement | null>(null);
  const [promptExpanded, setPromptExpanded] = useState(false);
  const [promptOverflows, setPromptOverflows] = useState(false);

  // The prompt is clamped to a few lines; only offer the toggle when it truly overflows.
  // Re-measured whenever the prompt text, the clamp state, or the window width changes.
  const prompt = routine.prompt;
  const measure = useCallback((): void => {
    const el = promptRef.current;
    if (el === null || promptExpanded || el.textContent !== prompt) {
      return;
    }
    setPromptOverflows(el.scrollHeight - el.clientHeight > 1);
  }, [promptExpanded, prompt]);

  useLayoutEffect(() => {
    measure();
    window.addEventListener("resize", measure);
    return () => {
      window.removeEventListener("resize", measure);
    };
  }, [measure]);

  return (
    <li className="routine-item">
      <div className="routine-row">
        <label
          className="toggle"
          aria-label={`${routine.enabled ? "Disable" : "Enable"} ${routine.name}`}
          title={routine.enabled ? "Disable" : "Enable"}
        >
          <input
            type="checkbox"
            checked={routine.enabled}
            disabled={!connected || !canControl}
            onChange={props.onToggleEnabled}
          />
          <span className="toggle-track" />
        </label>
        <div className="routine-main">
          <span className="routine-name">{routine.name}</span>
          <span className="routine-trigger">{triggerSummary(routine.trigger, bots)}</span>
          <span className="routine-next">{nextRunLabel(routine)}</span>
        </div>
        <div className="routine-actions">
          {canControl ? (
            <button
              type="button"
              className="btn btn-small"
              disabled={!connected}
              onClick={props.onRunNow}
            >
              Run now
            </button>
          ) : null}
          <button type="button" className="btn btn-small" onClick={props.onToggleHistory}>
            {expanded ? "Hide history" : "History"}
          </button>
        </div>
      </div>
      <div ref={promptRef} className={`routine-prompt mono${promptExpanded ? "" : " clamped"}`}>
        {prompt}
      </div>
      {promptOverflows ? (
        <button
          type="button"
          className="btn btn-small routine-prompt-toggle"
          onClick={() => {
            setPromptExpanded((prev) => !prev);
          }}
        >
          {promptExpanded ? "Show less" : "Show more"}
        </button>
      ) : null}
      {props.children}
    </li>
  );
}
