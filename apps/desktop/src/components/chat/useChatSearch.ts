import { useEffect, useState } from "react";
import type { ChatItem, ChatTurn } from "../../protocol/chat";

function itemText(item: ChatItem): string {
  switch (item.type) {
    case "text":
      return item.markdown;
    case "step":
      return `${item.title} ${item.subtitle ?? ""}`;
    case "sent":
      return `${item.to} ${item.body}`;
    case "completed":
      return `${item.result} ${item.artifacts.map((a) => a.name).join(" ")}`;
    case "decision":
      return item.title;
    case "aside":
      return item.text;
    default:
      return item satisfies never;
  }
}

function turnText(turn: ChatTurn): string {
  const trigger = turn.trigger;
  const head = "text" in trigger ? trigger.text : "";
  const from = trigger.kind === "bus" ? trigger.from : "";
  return [head, from, ...turn.items.map(itemText)].join(" ").toLowerCase();
}

/** The turns that mention `query`, case-insensitively; all of them for an empty query. */
export function matchTurns(turns: readonly ChatTurn[], query: string): readonly ChatTurn[] {
  const needle = query.trim().toLowerCase();
  return needle === "" ? turns : turns.filter((turn) => turnText(turn).includes(needle));
}

export interface ChatSearch {
  readonly open: boolean;
  readonly query: string;
  readonly setQuery: (query: string) => void;
  readonly toggle: (open: boolean) => void;
}

/** ⌘F opens the search while the chat is the active tab; Esc closes it. */
export function useChatSearch(active: boolean): ChatSearch {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
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
  const toggle = (next: boolean): void => {
    setOpen(next);
    if (!next) {
      setQuery("");
    }
  };
  return { open, query, setQuery, toggle };
}
