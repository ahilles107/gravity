import { useEffect } from "react";
import type { DaemonApi } from "../protocol/api";
import { isLocalEndpoint } from "../protocol/connection";
import type { ConnectionStatus, Endpoint } from "../protocol/connection";
import { localDaemonEndpoint } from "../setup";
import { isTauri } from "../tauri";
import { useLatestRef } from "./useLatestRef";

const DISCOVERY_INTERVAL_MS = 1_000;

/**
 * Follows the daemon on this machine when its port moves.
 *
 * The managed daemon negotiates a fallback port when the configured one is
 * occupied and publishes the result, so a client that only knows the configured
 * port would sit disconnected forever. Only runs while a local connection is
 * down: a healthy connection, and any remote endpoint, is left alone.
 */
export function useLocalDaemonDiscovery(
  client: DaemonApi,
  changeEndpoint: (endpoint: Endpoint) => void,
): void {
  const changeEndpointRef = useLatestRef(changeEndpoint);

  useEffect(() => {
    let cancelled = false;
    let discovering = false;
    const rediscover = async (status: ConnectionStatus): Promise<void> => {
      if (
        discovering ||
        !isTauri() ||
        status === "connected" ||
        !isLocalEndpoint(client.getEndpoint())
      ) {
        return;
      }
      discovering = true;
      try {
        const discovered = await localDaemonEndpoint();
        const current = client.getEndpoint();
        if (
          !cancelled &&
          client.status !== "connected" &&
          isLocalEndpoint(current) &&
          discovered.port !== current.port
        ) {
          changeEndpointRef.current(discovered);
        }
      } finally {
        discovering = false;
      }
    };
    void rediscover(client.status);
    const unsubscribe = client.onStatus((status) => {
      void rediscover(status);
    });
    const interval = setInterval(() => {
      void rediscover(client.status);
    }, DISCOVERY_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(interval);
      unsubscribe();
    };
    // `useLatestRef` is stable; include it for the conventional hooks rule.
    // oxlint-disable-next-line react/exhaustive-effect-dependencies
  }, [changeEndpointRef, client]);
}
