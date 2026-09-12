import type { ReactElement } from "react";
import type { Decision } from "../../protocol/decisions";
import DecisionRow from "./DecisionRow";

interface WaitingListProps {
  readonly waiting: readonly Decision[];
  readonly held: readonly Decision[];
  readonly settledCount: number;
  readonly loaded: boolean;
  readonly selectedId?: string;
  readonly heldOpen: boolean;
  readonly onToggleHeld: () => void;
  readonly onSelect: (decisionId: string) => void;
  readonly now: number;
  readonly projectName: (projectId: string) => string;
  readonly botName: (botId: string) => string | undefined;
}

/** "3 waiting on you · 1 urgent · 2 answered, unpublished" */
function summaryText(waiting: readonly Decision[]): string {
  if (waiting.length === 0) {
    return "Nothing open";
  }
  const urgent = waiting.filter((item) => item.priority === "urgent").length;
  const drafts = waiting.filter((item) => item.state === "answered").length;
  return [
    `${waiting.length} waiting on you`,
    urgent > 0 ? ` · ${urgent} urgent` : "",
    drafts > 0 ? ` · ${drafts} answered, unpublished` : "",
  ].join("");
}

function Empty({ settledCount }: { readonly settledCount: number }): ReactElement {
  return (
    <div className="cc-empty">
      <div className="cc-empty-ring" />
      <div className="cc-empty-title">Nothing is waiting on you.</div>
      <div className="cc-empty-body">Bots will raise the next decision here.</div>
      <div className="cc-empty-foot">{settledCount} rulings in the registry · ⌘2</div>
    </div>
  );
}

/** The parked decisions, behind a toggle so they stay out of the count to clear. */
function HeldSection({
  held,
  heldOpen,
  onToggleHeld,
  selectedId,
  now,
  projectName,
  botName,
  onSelect,
}: {
  readonly held: readonly Decision[];
  readonly heldOpen: boolean;
  readonly onToggleHeld: () => void;
  readonly selectedId?: string;
  readonly now: number;
  readonly projectName: (projectId: string) => string;
  readonly botName: (botId: string) => string | undefined;
  readonly onSelect: (decisionId: string) => void;
}): ReactElement | null {
  if (held.length === 0) {
    return null;
  }
  return (
    <>
      <button
        type="button"
        className="cc-held-toggle"
        aria-expanded={heldOpen}
        onClick={onToggleHeld}
      >
        <span className={`cc-chevron ${heldOpen ? "cc-chevron-open" : ""}`}>›</span>
        <span>Held</span>
        <span className="cc-count">{held.length}</span>
        <span className="cc-held-hint">come back on their own</span>
      </button>
      {heldOpen
        ? held.map((decision) => (
            <DecisionRow
              key={decision.id}
              decision={decision}
              selected={decision.id === selectedId}
              held
              now={now}
              projectName={projectName}
              botName={botName}
              onSelect={() => onSelect(decision.id)}
            />
          ))
        : null}
    </>
  );
}

/**
 * Everything still waiting on the owner, urgent first, with the parked ones
 * folded away underneath.
 *
 * Held decisions come back on their own, so they sit behind a toggle rather
 * than adding to the count the owner is being asked to clear.
 */
export default function WaitingList({
  waiting,
  held,
  settledCount,
  loaded,
  selectedId,
  heldOpen,
  onToggleHeld,
  onSelect,
  now,
  projectName,
  botName,
}: WaitingListProps): ReactElement {
  if (!loaded) {
    return <section className="cc-list" />;
  }

  if (waiting.length === 0 && held.length === 0) {
    return (
      <section className="cc-list">
        <Empty settledCount={settledCount} />
      </section>
    );
  }

  return (
    <section className="cc-list">
      <div className="cc-list-summary">
        <span>{summaryText(waiting)}</span>
        <span className="cc-list-keys">
          <span className="cc-key">↑</span>
          <span className="cc-key">↓</span>
          <span className="cc-key">↩</span>
        </span>
      </div>

      {waiting.map((decision) => (
        <DecisionRow
          key={decision.id}
          decision={decision}
          selected={decision.id === selectedId}
          now={now}
          projectName={projectName}
          botName={botName}
          onSelect={() => onSelect(decision.id)}
        />
      ))}

      <HeldSection
        held={held}
        heldOpen={heldOpen}
        onToggleHeld={onToggleHeld}
        selectedId={selectedId}
        now={now}
        projectName={projectName}
        botName={botName}
        onSelect={onSelect}
      />
    </section>
  );
}
