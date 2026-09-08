import { useCallback, useState } from "react";
import type { KeyboardEvent } from "react";

export interface ListKeyboardNav<T> {
  readonly activeIndex: number;
  readonly setActiveIndex: (index: number) => void;
  readonly onKeyDown: (event: KeyboardEvent<T>) => void;
}

interface NavOptions<Item> {
  readonly items: readonly Item[];
  readonly onChoose: (item: Item) => void;
  readonly onEscape: () => void;
}

/** Wraps `index` into `[0, length)`, treating an empty list as index 0. */
function wrap(index: number, length: number): number {
  return length === 0 ? 0 : (index + length) % length;
}

/**
 * Arrow/Enter/Escape navigation over a list rendered next to a text input.
 * Owns the highlighted index so callers only render it.
 */
export function useListKeyboardNav<Element, Item>(
  options: NavOptions<Item>,
): ListKeyboardNav<Element> {
  const { items, onChoose, onEscape } = options;
  const [activeIndex, setActiveIndex] = useState(0);

  const onKeyDown = useCallback(
    (event: KeyboardEvent<Element>): void => {
      const step = { ArrowDown: 1, ArrowUp: -1 }[event.key];
      if (step !== undefined) {
        event.preventDefault();
        setActiveIndex((prev) => wrap(prev + step, items.length));
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        onEscape();
        return;
      }
      if (event.key !== "Enter") {
        return;
      }
      event.preventDefault();
      const item = items[activeIndex];
      if (item !== undefined) {
        onChoose(item);
      }
    },
    [activeIndex, items, onChoose, onEscape],
  );

  return { activeIndex, setActiveIndex, onKeyDown };
}
