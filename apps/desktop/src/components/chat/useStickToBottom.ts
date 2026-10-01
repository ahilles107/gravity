import { useLayoutEffect, useRef } from "react";
import type { MutableRefObject } from "react";

/** How close to the bottom still counts as following the conversation. */
const STICK_PX = 80;

export interface StickToBottom {
  /** The scroller to keep at the bottom. */
  readonly ref: MutableRefObject<HTMLDivElement | null>;
  readonly onScroll: () => void;
  /** Follow the bottom again, e.g. after the owner sends. */
  readonly follow: () => void;
  /** Stop following, e.g. before loading older turns above. */
  readonly release: () => void;
}

/**
 * Keeps a scroller at the bottom while the owner is there, and leaves them be
 * once they scroll up. New `turns` or `pending` are what trigger a follow.
 */
export function useStickToBottom(turns: unknown, pending: unknown): StickToBottom {
  const ref = useRef<HTMLDivElement | null>(null);
  const stick = useRef(true);
  useLayoutEffect(() => {
    const element = ref.current;
    if (element !== null && stick.current) {
      element.scrollTop = element.scrollHeight;
    }
    // New content is the trigger, not an input: the effect only reads the DOM.
    // oxlint-disable-next-line react/exhaustive-effect-dependencies
  }, [turns, pending]);
  return {
    ref,
    onScroll: () => {
      const element = ref.current;
      if (element !== null) {
        stick.current = element.scrollHeight - element.scrollTop - element.clientHeight < STICK_PX;
      }
    },
    follow: () => {
      stick.current = true;
    },
    release: () => {
      stick.current = false;
    },
  };
}
