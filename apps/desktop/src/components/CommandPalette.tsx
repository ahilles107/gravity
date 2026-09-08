import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactElement } from "react";
import { useListKeyboardNav } from "../hooks/useListKeyboardNav";
import { useScrollActiveIntoView } from "../hooks/useScrollActiveIntoView";
import { fuzzyScore } from "../util";
import OverlayShell from "./overlay/OverlayShell";

export interface PaletteAction {
  readonly id: string;
  readonly label: string;
  readonly hint: string;
  readonly run: () => void;
}

interface CommandPaletteProps {
  readonly actions: readonly PaletteAction[];
  readonly onSearch: (query: string) => void;
  readonly onClose: () => void;
}

const MAX_RESULTS = 12;

/**
 * Ranks actions by fuzzy match, best first, capped at `MAX_RESULTS`.
 * `sort` (not `toSorted`) because the build targets ES2021; it runs on the
 * fresh array produced by `map`, so nothing shared is mutated.
 */
function rank(actions: readonly PaletteAction[], query: string): readonly PaletteAction[] {
  const scored = actions
    .map((action) => ({ action, score: fuzzyScore(query, action.label) }))
    .filter((item): item is { action: PaletteAction; score: number } => item.score !== null);
  // oxlint-disable-next-line unicorn/no-array-sort
  scored.sort((a, b) => b.score - a.score);
  return scored.slice(0, MAX_RESULTS).map((item) => item.action);
}

/** Cmd+K palette: fuzzy-filterable actions plus a "Search: <query>" passthrough. */
export default function CommandPalette(props: CommandPaletteProps): ReactElement {
  const { actions, onSearch, onClose } = props;
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listRef = useRef<HTMLUListElement | null>(null);

  const entries = useMemo((): readonly PaletteAction[] => {
    const trimmed = query.trim();
    const matches = rank(actions, query);
    if (trimmed.length === 0) {
      return matches;
    }
    return [
      ...matches,
      {
        id: "__search__",
        label: `Search: ${trimmed}`,
        hint: "search bots and messages",
        run: () => {
          onSearch(trimmed);
        },
      },
    ];
  }, [actions, onSearch, query]);

  const nav = useListKeyboardNav<HTMLInputElement, PaletteAction>({
    items: entries,
    onChoose: (entry) => {
      onClose();
      entry.run();
    },
    onEscape: onClose,
  });
  const { activeIndex, setActiveIndex } = nav;

  useScrollActiveIntoView(listRef, activeIndex);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  return (
    <OverlayShell label="Command palette" onClose={onClose}>
      <input
        ref={inputRef}
        className="overlay-input"
        placeholder="Type a command or search…"
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setActiveIndex(0);
        }}
        onKeyDown={nav.onKeyDown}
      />
      {entries.length === 0 ? (
        <div className="muted overlay-empty">No matching commands.</div>
      ) : (
        <ul ref={listRef} className="overlay-list">
          {entries.map((entry, index) => (
            <li key={entry.id}>
              <button
                type="button"
                className={`overlay-option ${index === activeIndex ? "overlay-active" : ""}`}
                onMouseEnter={() => {
                  setActiveIndex(index);
                }}
                onMouseDown={(event) => {
                  event.preventDefault();
                  onClose();
                  entry.run();
                }}
              >
                <span className="overlay-label">{entry.label}</span>
                <span className="overlay-hint">{entry.hint}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </OverlayShell>
  );
}
