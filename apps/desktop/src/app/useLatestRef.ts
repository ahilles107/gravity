import { useEffect, useRef } from "react";
import type { MutableRefObject } from "react";

/**
 * Mirrors a rendered value into a ref for use by callbacks that fire outside
 * React — daemon push handlers, in this app. The write happens in an effect, so
 * nothing reads or mutates a ref during render.
 */
export function useLatestRef<T>(value: T): MutableRefObject<T> {
  const ref = useRef(value);
  useEffect(() => {
    ref.current = value;
  }, [value]);
  return ref;
}
