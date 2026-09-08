import { useCallback, useEffect, useState } from "react";
import { capture } from "../analytics";
import type { DaemonApi } from "../protocol/api";
import type { Endpoint } from "../protocol/connection";
import { loadSetupComplete, markSetupComplete, saveDeviceToken } from "../settings";
import type { SetupConnection } from "../setup";
import { isTauri } from "../tauri";

export interface FirstRunSetup {
  /** True while the first-run wizard should replace the main UI. */
  readonly showWizard: boolean;
  /** Stores any device token, then points the client at the daemon and reconnects. */
  readonly connectToDaemon: (connection: SetupConnection) => void;
}

/**
 * First-run wizard state: shown in the Tauri shell until this install has
 * connected to a daemon once. The flag latches on the first connect (state
 * for this session, localStorage for the next launch), so a later disconnect
 * never resurfaces the wizard.
 */
export function useFirstRunSetup(
  client: DaemonApi,
  changeEndpoint: (endpoint: Endpoint) => void,
): FirstRunSetup {
  const [setupDone, setSetupDone] = useState(loadSetupComplete);

  useEffect(() => {
    if (setupDone) {
      return undefined;
    }
    return client.onStatus((status) => {
      if (status === "connected") {
        markSetupComplete();
        setSetupDone(true);
      }
    });
  }, [client, setupDone]);

  // The wizard resolves the port from the daemon's own config, so an install
  // predating the current default is still reachable after an upgrade.
  const connectToDaemon = useCallback(
    (connection: SetupConnection): void => {
      capture("setup_method_selected", { method: connection.method });
      if (connection.method === "remote" && connection.token !== undefined) {
        saveDeviceToken(connection.token);
      }
      changeEndpoint(connection.endpoint);
    },
    [changeEndpoint],
  );

  return { showWizard: isTauri() && !setupDone, connectToDaemon };
}
