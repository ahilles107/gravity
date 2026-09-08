import { useEffect, useRef } from "react";
import type { MutableRefObject } from "react";

/**
 * Keeps the selected sidebar row in view.
 *
 * Startup reopens whichever bot was last used, which can sit below the fold of
 * a long tree, so the highlight would otherwise land off-screen with the
 * sidebar still scrolled to the top. `nearest` leaves an already-visible row
 * where it is, so picking a row by hand never scrolls under the pointer.
 */
export function useScrollSelectedIntoView<T extends HTMLElement>(
  selected: boolean,
): MutableRefObject<T | null> {
  const ref = useRef<T | null>(null);

  useEffect(() => {
    if (selected) {
      ref.current?.scrollIntoView({ block: "nearest" });
    }
  }, [selected]);

  return ref;
}
