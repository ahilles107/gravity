import { useEffect, useState } from "react";

/** How often "3d ago" and "due in 6h" are recomputed. */
const TICK_MS = 60_000;

/**
 * The current time, as a value the render can depend on.
 *
 * Ages and deadlines are relative, so reading the clock during render would be
 * both impure and wrong — the label would freeze until something else changed.
 * Passing it in also makes both testable and lets stories pin it, so a visual
 * baseline does not drift a day at a time.
 */
export function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => {
      setNow(Date.now());
    }, TICK_MS);
    return () => {
      clearInterval(timer);
    };
  }, []);
  return now;
}
