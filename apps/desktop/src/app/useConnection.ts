import { useCallback, useEffect, useState } from "react";
import type { DaemonContext } from "../analytics";
import { capture, registerDaemonContext } from "../analytics";
import type { DaemonApi } from "../protocol/api";
import { isLocalEndpoint } from "../protocol/connection";
import type { ConnectionStatus, Endpoint } from "../protocol/connection";
import { saveEndpoint } from "../settings";
import { useLatestRef } from "./useLatestRef";
import { useLocalDaemonDiscovery } from "./useLocalDaemonDiscovery";

export interface ConnectionApi {
  readonly status: ConnectionStatus;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly endpoint: Endpoint;
  /** The connected daemon's version, empty until a `hello_ok` lands. */
  readonly serverVersion: string;
  readonly changeEndpoint: (next: Endpoint) => void;
}

/**
 * Tracks the daemon connection and starts the client once. `onUp` runs on every
 * transition into `connected`, and a downed local connection follows the
 * daemon's published port.
 */
export function useConnection(client: DaemonApi, onUp: () => void): ConnectionApi {
  const [status, setStatus] = useState<ConnectionStatus>(client.status);
  const [canControl, setCanControl] = useState(false);
  const [serverVersion, setServerVersion] = useState(client.serverVersion);
  const [endpoint, setEndpoint] = useState<Endpoint>(client.getEndpoint());
  const onUpRef = useLatestRef(onUp);
  const endpointRef = useLatestRef(endpoint);

  useEffect(() => {
    const unsub = client.onStatus((next) => {
      setStatus(next);
      setCanControl(next === "connected" && client.hasGrant("control"));
      // The client sets this from `hello_ok` before announcing `connected`.
      // Do not let a previous endpoint's version suppress the disconnected
      // health probe used to recover from a protocol mismatch.
      setServerVersion(next === "connected" ? client.serverVersion : "");
      if (next === "connected") {
        const context = {
          can_control: client.hasGrant("control"),
          connection_type: isLocalEndpoint(endpointRef.current) ? "local" : "remote",
        } satisfies Omit<DaemonContext, "daemon_version">;
        capture("daemon_connected", context);
        registerDaemonContext({ ...context, daemon_version: client.serverVersion });
        onUpRef.current();
      }
    });
    client.start();
    return unsub;
  }, [client, endpointRef, onUpRef]);

  const changeEndpoint = useCallback(
    (next: Endpoint): void => {
      saveEndpoint(next);
      setEndpoint(next);
      client.setEndpoint(next);
    },
    [client],
  );

  useLocalDaemonDiscovery(client, changeEndpoint);

  return {
    status,
    connected: status === "connected",
    canControl,
    endpoint,
    serverVersion,
    changeEndpoint,
  };
}
