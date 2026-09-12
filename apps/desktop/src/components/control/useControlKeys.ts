import { useEffect, useRef } from "react";
import type { RefObject } from "react";
import type { Decision } from "../../protocol/decisions";
import { NO_FILTER, matchesRegistry } from "./decisions";
import type { RegistryFilter } from "./decisions";
import type { ControlTab } from "./useControlState";
import type { DecisionsApi } from "./useDecisions";

export interface ControlKeyDeps {
  readonly tab: ControlTab;
  readonly setTab: (tab: ControlTab) => void;
  /** Ids in display order on the current tab. */
  readonly navList: readonly string[];
  /** The highlighted row the arrows move from. */
  readonly cursorId: string | undefined;
  /** Highlight a row; on the ledger this does not open it. */
  readonly moveCursor: (decisionId: string) => void;
  /** Open a row in the reading pane. */
  readonly select: (decisionId: string) => void;
  readonly reading: Decision | undefined;
  readonly readingOpen: boolean;
  readonly back: () => void;
  readonly trayOpen: boolean;
  readonly setTrayOpen: (open: boolean) => void;
  readonly hasDrafts: boolean;
  readonly holdOpen: boolean;
  readonly setHoldOpen: (open: boolean) => void;
  readonly togglePick: (key: string) => void;
  readonly saveRuling: () => void;
  readonly askInThread: () => void;
  readonly textareaRef: RefObject<HTMLTextAreaElement>;
  readonly searchRef: RefObject<HTMLInputElement>;
}

const TABS: readonly ControlTab[] = ["waiting", "settled", "tags"];

/** Ids in display order on a tab: waiting rows, then held rows once the group is open. */
export function navListFor(
  tab: ControlTab,
  lists: Pick<DecisionsApi, "waiting" | "held" | "registry">,
  heldOpen: boolean,
  filter: RegistryFilter = NO_FILTER,
): readonly string[] {
  if (tab === "waiting") {
    return [...lists.waiting, ...(heldOpen ? lists.held : [])].map((item) => item.id);
  }
  if (tab === "settled") {
    return lists.registry.filter((item) => matchesRegistry(item, filter)).map((item) => item.id);
  }
  return [];
}

function isEditable(target: EventTarget | null): target is HTMLElement {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  const tag = target.tagName.toLowerCase();
  return tag === "textarea" || tag === "input" || tag === "select" || target.isContentEditable;
}

/** ⌘↩ save, ⇧⌘↩ ask, ⌘⇧P tray, ⌘1–3 tabs. True when the chord was one of ours. */
function metaChord(event: KeyboardEvent, d: ControlKeyDeps): boolean {
  if (event.key === "Enter") {
    if (event.shiftKey) {
      d.askInThread();
    } else {
      d.saveRuling();
    }
    return true;
  }
  if (event.shiftKey && event.key.toLowerCase() === "p") {
    if (d.hasDrafts) {
      d.setTrayOpen(!d.trayOpen);
    }
    return true;
  }
  const tab = TABS[Number(event.key) - 1];
  if (["1", "2", "3"].includes(event.key) && tab !== undefined) {
    d.setTab(tab);
    return true;
  }
  return false;
}

/** Leave a field, close the tray, close the hold strip, or go back — first match wins. */
function escape(event: KeyboardEvent, d: ControlKeyDeps): void {
  if (isEditable(event.target)) {
    event.target.blur();
  } else if (d.trayOpen) {
    d.setTrayOpen(false);
  } else if (d.holdOpen) {
    d.setHoldOpen(false);
  } else if (d.tab === "settled" && d.readingOpen) {
    d.back();
  }
}

/** ↑↓ / j k through the current list; ↩ opens the highlighted row. */
function navigate(event: KeyboardEvent, d: ControlKeyDeps): boolean {
  if (["ArrowDown", "j", "ArrowUp", "k"].includes(event.key)) {
    event.preventDefault();
    const index = d.navList.indexOf(d.cursorId ?? "");
    const down = event.key === "ArrowDown" || event.key === "j";
    const next =
      index < 0 ? 0 : down ? Math.min(d.navList.length - 1, index + 1) : Math.max(0, index - 1);
    const id = d.navList[next];
    if (id !== undefined) {
      d.moveCursor(id);
    }
    return true;
  }
  if (event.key === "Enter") {
    // Without this the focused tab or row button is clicked as well, which on
    // the ledger switches tabs again and closes what ↩ just opened.
    event.preventDefault();
    if (d.cursorId !== undefined) {
      d.select(d.cursorId);
    }
    return true;
  }
  return false;
}

/** 1–8 pick, r write, h hold — only while an open decision is being read. */
function compose(event: KeyboardEvent, d: ControlKeyDeps): void {
  if (d.reading === undefined || d.reading.state !== "open") {
    return;
  }
  if (/^[1-8]$/.test(event.key)) {
    const option = d.reading.options[Number(event.key) - 1];
    if (option !== undefined) {
      d.togglePick(option.key);
    }
  } else if (event.key === "r") {
    event.preventDefault();
    d.textareaRef.current?.focus();
  } else if (event.key === "h") {
    event.preventDefault();
    d.setHoldOpen(!d.holdOpen);
  }
}

/**
 * The bindings that make getting to zero fast: move, pick, write, rule.
 *
 * One window listener, subscribed once; the current deps are read through a
 * ref. The app's ⌘K and ⌘, run in the capture phase and stop propagation, so
 * they never reach this handler, and any other ⌘ chord is left alone.
 */
export function useControlKeys(deps: ControlKeyDeps): void {
  const ref = useRef(deps);
  useEffect(() => {
    ref.current = deps;
  });

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.defaultPrevented) {
        return;
      }
      const d = ref.current;
      if (event.metaKey || event.ctrlKey) {
        if (metaChord(event, d)) {
          event.preventDefault();
        }
        return;
      }
      if (event.key === "Escape") {
        escape(event, d);
        return;
      }
      if (isEditable(event.target)) {
        return;
      }
      if (navigate(event, d)) {
        return;
      }
      if (event.key === "/" && d.tab === "settled") {
        event.preventDefault();
        d.searchRef.current?.focus();
        return;
      }
      compose(event, d);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);
}
