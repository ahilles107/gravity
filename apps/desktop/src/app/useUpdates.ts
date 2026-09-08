import { useEffect, useRef } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { captureException } from "../analytics";
import { isLocalEndpoint } from "../protocol/connection";
import type { ConnectionStatus, Endpoint } from "../protocol/connection";
import { installLocalDaemon, probeDaemon } from "../setup";
import { isTauri } from "../tauri";
import { checkForAppUpdate } from "../updater";
import { compareVersions } from "../version";
import { runAppUpdate } from "./appUpdate";
import { useLatestRef } from "./useLatestRef";
import type { AddToast } from "./useToasts";

const APP_CHECK_INTERVAL_MS = 4 * 60 * 60 * 1000;

/**
 * Update prompts, both halves of a release: a newer app build published to
 * the update endpoint, and a daemon older than the app that carries its
 * replacement as the bundled sidecar.
 *
 * Installing an app update also refreshes an app-managed local daemon as part
 * of the acknowledged restart. The standalone daemon prompt is recovery for
 * an earlier split update or a failed daemon refresh. Every prompt fires once
 * per session per version.
 */
export function useUpdates(addToast: AddToast, status: ConnectionStatus, endpoint: Endpoint): void {
  const prompted = useRef<Set<string>>(new Set());
  const endpointRef = useLatestRef(endpoint);

  // App updates: check on launch and every few hours thereafter.
  useEffect(() => {
    if (!isTauri()) {
      return undefined;
    }
    let disposed = false;
    const check = async (): Promise<void> => {
      const version = await checkForAppUpdate();
      if (disposed || version === null || prompted.current.has(`app:${version}`)) {
        return;
      }
      prompted.current.add(`app:${version}`);
      addToast(
        "info",
        "Update available",
        `Version ${version} is ready. Updating relaunches the app and refreshes a daemon it manages on this machine, stopping that daemon's running bots.`,
        {
          sticky: true,
          action: {
            label: "Restart & update",
            // The prompt outlives an endpoint change, so whether a local
            // daemon comes along is decided here rather than at check time.
            run: () => {
              runAppUpdate(isLocalEndpoint(endpointRef.current), addToast);
            },
          },
        },
      );
    };
    void check();
    const timer = setInterval(() => void check(), APP_CHECK_INTERVAL_MS);
    return (): void => {
      disposed = true;
      clearInterval(timer);
    };
    // `endpointRef` keeps an endpoint change from restarting the check timer.
  }, [addToast, endpointRef]);

  // Daemon update recovery: compare versions once the connection has settled
  // and offer the reinstall (`service install` keeps the config and reloads
  // launchd).
  //
  // Deliberately not gated on a live connection. A daemon left behind by an app
  // update can be a whole protocol version back, in which case it rejects the
  // handshake and the app never connects — exactly when this prompt is the only
  // way out. `/health` is plain HTTP and answers either way, so it, not the
  // socket, decides whether there is anything to say.
  useEffect(() => {
    if (
      !isTauri() ||
      (status !== "connected" && status !== "version_mismatch") ||
      !isLocalEndpoint(endpoint)
    ) {
      return undefined;
    }
    let disposed = false;
    const check = async (): Promise<void> => {
      const [health, appVersion] = await Promise.all([probeDaemon(endpoint), getVersion()]);
      if (disposed || health === null) {
        return;
      }
      const stranded = status === "version_mismatch";
      const appOrder = compareVersions(appVersion, health.version);
      const promptKey = `daemon:${appVersion}:${health.version}`;
      if (stranded && appOrder !== "newer") {
        if (prompted.current.has(promptKey)) {
          return;
        }
        prompted.current.add(promptKey);
        const appIsOlder = appOrder === "older";
        addToast(
          "error",
          appIsOlder ? "App too old to connect" : "Incompatible daemon version",
          appIsOlder
            ? `The daemon runs ${health.version}, which is newer than this app (${appVersion}). Update Gravity to connect without downgrading the daemon.`
            : `The daemon runs ${health.version}, which is not compatible with this app (${appVersion}). Reinstall matching app and daemon versions.`,
          { sticky: true },
        );
        return;
      }
      if (appOrder !== "newer" || prompted.current.has(promptKey)) {
        return;
      }
      prompted.current.add(promptKey);
      addToast(
        stranded ? "error" : "info",
        stranded ? "Daemon too old to connect" : "Daemon update available",
        stranded
          ? `The daemon runs ${health.version} and speaks an older protocol, so this app (${appVersion}) cannot talk to it. Updating restarts the daemon and stops running bots.`
          : `The daemon runs ${health.version}; this app bundles ${appVersion}. Updating restarts the daemon and stops running bots.`,
        {
          sticky: true,
          action: {
            label: "Update daemon",
            run: () => {
              installLocalDaemon(health.version).catch((error: unknown) => {
                captureException(error, "daemon_update");
                const body = error instanceof Error ? error.message : "daemon update failed";
                addToast("error", "Daemon update failed", body, { sticky: true });
              });
            },
          },
        },
      );
    };
    void check();
    return (): void => {
      disposed = true;
    };
  }, [addToast, status, endpoint]);
}
