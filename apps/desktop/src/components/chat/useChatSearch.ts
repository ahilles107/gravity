import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { RefObject } from "react";

/** Highlight names the chat's stylesheet paints (`::highlight(...)`). */
const ALL = "chat-search";
const CURRENT = "chat-search-current";

/**
 * Every case-insensitive occurrence of `query` in the text under `root`, in
 * document order.
 */
export function findMatches(root: HTMLElement, query: string): Range[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") {
    return [];
  }
  const ranges: Range[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
    const text = (node.textContent ?? "").toLowerCase();
    for (let at = text.indexOf(needle); at !== -1; at = text.indexOf(needle, at + needle.length)) {
      const range = document.createRange();
      range.setStart(node, at);
      range.setEnd(node, at + needle.length);
      ranges.push(range);
    }
  }
  return ranges;
}

/** Paints the matches, the current one apart, where the webview can. */
function paint(ranges: readonly Range[], current: number): void {
  if (typeof CSS === "undefined" || !("highlights" in CSS) || typeof Highlight === "undefined") {
    return;
  }
  CSS.highlights.set(ALL, new Highlight(...ranges.filter((_, index) => index !== current)));
  const focused = ranges[current];
  if (focused === undefined) {
    CSS.highlights.delete(CURRENT);
  } else {
    CSS.highlights.set(CURRENT, new Highlight(focused));
  }
}

function clear(): void {
  if (typeof CSS !== "undefined" && "highlights" in CSS) {
    CSS.highlights.delete(ALL);
    CSS.highlights.delete(CURRENT);
  }
}

export interface ChatSearch {
  readonly open: boolean;
  readonly query: string;
  readonly setQuery: (query: string) => void;
  readonly toggle: (open: boolean) => void;
  /** How many matches the loaded turns hold. */
  readonly count: number;
  /** The match on screen, from 0. */
  readonly current: number;
  readonly next: () => void;
  readonly previous: () => void;
}

/**
 * Find in the chat: ⌘F opens it while the chat is the active tab, matches are
 * highlighted in place, and next / previous scroll from one to the other.
 * `content` changes whenever the turns do, so the matches follow new text.
 */
export function useChatSearch(
  active: boolean,
  root: RefObject<HTMLElement | null>,
  content: unknown,
): ChatSearch {
  const [open, setOpen] = useState(false);
  const [query, setQueryState] = useState("");
  const [count, setCount] = useState(0);
  const [current, setCurrent] = useState(0);
  const ranges = useRef<Range[]>([]);

  useEffect(() => {
    if (!active) {
      return undefined;
    }
    const onKey = (event: KeyboardEvent): void => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        setOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [active]);

  // Re-find after every render that could have moved the text.
  useLayoutEffect(() => {
    const element = root.current;
    ranges.current = open && element !== null ? findMatches(element, query) : [];
    setCount(ranges.current.length);
    setCurrent((index) => Math.min(index, Math.max(ranges.current.length - 1, 0)));
    // `content` is the trigger: new turns mean new text to search.
    // oxlint-disable-next-line react/exhaustive-effect-dependencies
  }, [open, query, content, root]);

  // Repaint when the current match moves, or when re-finding changed the set.
  useLayoutEffect(() => {
    paint(ranges.current, current);
    const focused = ranges.current[current]?.startContainer.parentElement;
    focused?.scrollIntoView?.({ block: "center" });
    // `count` changes when the matches were re-found; the ranges live in a ref.
    // oxlint-disable-next-line react/exhaustive-effect-dependencies
  }, [current, count]);

  useEffect(() => clear, []);

  const setQuery = useCallback((next: string): void => {
    setQueryState(next);
    setCurrent(0);
  }, []);

  const toggle = useCallback((next: boolean): void => {
    setOpen(next);
    if (!next) {
      setQueryState("");
      setCurrent(0);
      clear();
    }
  }, []);

  const next = useCallback((): void => {
    setCurrent((index) => (count === 0 ? 0 : (index + 1) % count));
  }, [count]);

  const previous = useCallback((): void => {
    setCurrent((index) => (count === 0 ? 0 : (index - 1 + count) % count));
  }, [count]);

  return { open, query, setQuery, toggle, count, current, next, previous };
}
