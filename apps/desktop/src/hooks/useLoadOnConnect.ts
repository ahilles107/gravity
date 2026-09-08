import { useEffect } from "react";

/**
 * Runs `load` whenever the daemon connection comes up.
 *
 * Fetching daemon state is exactly the "synchronize with an external system"
 * case effects exist for, but the loader resolves asynchronously and eventually
 * calls setState, which oxlint's `set-state-in-effect` cannot see through.
 * Keeping the pattern in one hook keeps the suppression to a single place.
 */
export function useLoadOnConnect(connected: boolean, load: () => Promise<void>): void {
  useEffect(() => {
    if (connected) {
      // oxlint-disable-next-line react/set-state-in-effect
      void load();
    }
  }, [connected, load]);
}
