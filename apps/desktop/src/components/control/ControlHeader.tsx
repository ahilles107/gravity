import type { ReactElement } from "react";
import type { ControlTab } from "./useControlState";

interface ControlHeaderProps {
  readonly tab: ControlTab;
  readonly onTab: (tab: ControlTab) => void;
  readonly counts: { readonly waiting: number; readonly settled: number; readonly tags: number };
  readonly draftCount: number;
  readonly trayOpen: boolean;
  readonly onToggleTray: () => void;
  readonly canControl: boolean;
}

const TABS: readonly { readonly id: ControlTab; readonly label: string }[] = [
  { id: "waiting", label: "Waiting" },
  { id: "settled", label: "Settled" },
  { id: "tags", label: "Tags" },
];

/**
 * Tabs and the one action the header carries.
 *
 * Publish only appears once something is drafted, so the header is quiet
 * whenever there is nothing held back from the bots.
 */
export default function ControlHeader({
  tab,
  onTab,
  counts,
  draftCount,
  trayOpen,
  onToggleTray,
  canControl,
}: ControlHeaderProps): ReactElement {
  return (
    <header className="cc-header">
      <div className="cc-tabs">
        {TABS.map((item) => (
          <button
            key={item.id}
            type="button"
            className={`cc-tab ${tab === item.id ? "cc-tab-active" : ""}`}
            // The tabs stay buttons so they read and behave as buttons; the
            // state is still announced.
            aria-current={tab === item.id ? "page" : undefined}
            onClick={() => onTab(item.id)}
          >
            {item.label}
            <span className="cc-tab-count">{counts[item.id]}</span>
          </button>
        ))}
      </div>
      <div className="cc-header-actions">
        {draftCount > 0 ? (
          <button
            type="button"
            className={`cc-btn-green ${trayOpen ? "cc-btn-green-active" : ""}`}
            disabled={!canControl}
            onClick={onToggleTray}
          >
            <span>Publish</span> <span className="cc-btn-green-count">{draftCount}</span>
            <span className="cc-hint">⌘⇧P</span>
          </button>
        ) : null}
      </div>
    </header>
  );
}
