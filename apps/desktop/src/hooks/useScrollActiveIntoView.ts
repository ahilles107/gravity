import { useEffect } from "react";
import type { RefObject } from "react";

/** Keeps the highlighted option of a keyboard-navigated list in view. */
export function useScrollActiveIntoView(
  listRef: RefObject<HTMLUListElement | null>,
  activeIndex: number,
): void {
  useEffect(() => {
    const list = listRef.current;
    if (list === null) {
      return;
    }
    const active = list.children.item(activeIndex);
    if (active instanceof HTMLElement) {
      active.scrollIntoView({ block: "nearest" });
    }
  }, [activeIndex, listRef]);
}
