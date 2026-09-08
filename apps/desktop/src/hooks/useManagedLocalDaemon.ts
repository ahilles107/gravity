import { useEffect, useState } from "react";
import type { Endpoint } from "../protocol/connection";
import { isManagedLocalDaemon } from "../setup";

/**
 * True once we know the daemon at `endpoint` is the app-managed launchd agent
 * on this machine. Starts false so a control that only makes sense for a
 * managed daemon never flashes up for a remote or dev one.
 */
export function useManagedLocalDaemon(endpoint: Endpoint): boolean {
  const [managed, setManaged] = useState(false);

  useEffect(() => {
    let disposed = false;
    const check = async (): Promise<void> => {
      const result = await isManagedLocalDaemon(endpoint);
      if (!disposed) {
        // oxlint-disable-next-line react/set-state-in-effect
        setManaged(result);
      }
    };
    void check();
    return (): void => {
      disposed = true;
    };
  }, [endpoint]);

  return managed;
}
