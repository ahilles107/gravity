import type { ReactElement } from "react";
import type { BrowserAction } from "../../protocol/agents";
import { fmtTimestamp } from "../../util";
import { triggerView } from "../chat/chatModel";

/** Consecutive actions of one turn, under what started it. */
interface TurnGroup {
  readonly turnId: string;
  readonly at: string;
  readonly why: string;
  readonly ask: string;
  readonly actions: readonly BrowserAction[];
}

/** Groups newest-first actions by turn, keeping that order. */
export function groupByTurn(actions: readonly BrowserAction[]): readonly TurnGroup[] {
  const groups: TurnGroup[] = [];
  for (const action of actions) {
    const last = groups.at(-1);
    if (last !== undefined && last.turnId === action.turn_id) {
      groups[groups.length - 1] = { ...last, actions: [...last.actions, action] };
      continue;
    }
    const view = triggerView(action.trigger);
    groups.push({
      turnId: action.turn_id,
      at: action.at,
      why: view.label,
      ask: view.text,
      actions: [action],
    });
  }
  return groups;
}

/**
 * Every browser action the bot took, under the request that led to it: the
 * answer to "why is it browsing, and for whom".
 */
export default function BrowserActivityList({
  activity,
  error,
}: {
  readonly activity: readonly BrowserAction[];
  readonly error: string | null;
}): ReactElement {
  const groups = groupByTurn(activity);
  return (
    <aside className="browser-activity" aria-label="Browser activity">
      <h3 className="tasks-section-title">Activity</h3>
      {error === null ? null : <div className="chat-note chat-error">{error}</div>}
      {groups.length === 0 ? (
        <div className="muted browser-activity-empty">No browsing yet.</div>
      ) : null}
      <ol className="browser-activity-list">
        {groups.map((group) => (
          <li key={group.turnId} className="browser-turn">
            <div className="browser-turn-head">
              <span className="browser-turn-why">{group.why}</span>
              <span className="task-when">{fmtTimestamp(group.at)}</span>
            </div>
            {group.ask === "" ? null : (
              <div className="browser-turn-ask" title={group.ask}>
                {group.ask}
              </div>
            )}
            <ul className="browser-actions">
              {group.actions.map((action) => (
                <li key={action.step_id} className={`browser-action step-${action.status}`}>
                  <span className="browser-action-title">{action.title}</span>
                  {action.browser === "owners_chrome" ? (
                    <span className="browser-badge" title="Used your own Chrome">
                      your Chrome
                    </span>
                  ) : null}
                  {action.subtitle == null ? null : (
                    <span className="browser-action-detail" title={action.subtitle}>
                      {action.subtitle}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </li>
        ))}
      </ol>
    </aside>
  );
}
