import { useLayoutEffect, useRef } from "react";
import type { RefObject } from "react";
import { readerOffset, rememberReaderOffset } from "./controlSession";

/**
 * Remembers where the reader was left, per decision.
 *
 * Switching records is a read, not a navigation: coming back to a long thread
 * should land where it was left, while a record opened for the first time
 * starts at its title.
 */
export function useReaderScroll(decisionId: string | undefined): RefObject<HTMLDivElement> {
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const node = ref.current;
    if (node === null || decisionId === undefined) {
      return;
    }
    node.scrollTop = readerOffset(decisionId);
    return () => {
      rememberReaderOffset(decisionId, node.scrollTop);
    };
  }, [decisionId]);

  return ref;
}
