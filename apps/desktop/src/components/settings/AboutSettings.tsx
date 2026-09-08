import { useEffect, useState } from "react";
import type { ReactElement } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { runAppUpdate } from "../../app/appUpdate";
import type { AddToast } from "../../app/useToasts";
import { captureException } from "../../analytics";
import { isLocalEndpoint } from "../../protocol/connection";
import type { Endpoint } from "../../protocol/connection";
import { installLocalDaemon, probeDaemon } from "../../setup";
import { isTauri } from "../../tauri";
import { checkForAppUpdate } from "../../updater";
import { compareVersions } from "../../version";

interface AboutSettingsProps {
  readonly endpoint: Endpoint;
  /** The connected daemon's version, empty while disconnected. */
  readonly daemonVersion: string;
  readonly addToast: AddToast;
}

/**
 * The local daemon's version even when the handshake never succeeded — a
 * daemon on a different protocol rejects `hello` but still answers `/health`,
 * and that is precisely when version-aware recovery has to be reachable.
 */
function useLocalDaemonVersion(
  endpoint: Endpoint,
  connectedVersion: string,
  isLocal: boolean,
): string {
  const [probed, setProbed] = useState("");
  const shouldProbe = isTauri() && isLocal && connectedVersion.length === 0;

  useEffect(() => {
    if (!shouldProbe) {
      return undefined;
    }
    let disposed = false;
    const probe = async (): Promise<void> => {
      const health = await probeDaemon(endpoint);
      if (!disposed) {
        setProbed(health === null ? "" : health.version);
      }
    };
    void probe();
    return (): void => {
      disposed = true;
    };
  }, [endpoint, shouldProbe]);

  if (connectedVersion.length > 0) {
    return connectedVersion;
  }
  // A stale probe from a previous endpoint must not leak into a row that no
  // longer probes.
  return shouldProbe ? probed : "";
}

type UpdateState =
  | { readonly kind: "idle" }
  | { readonly kind: "checking" }
  | { readonly kind: "current" }
  | { readonly kind: "available"; readonly version: string };

const UPDATE_HELP: Readonly<Record<"idle" | "checking" | "current", string>> = {
  idle: "Also checked automatically every few hours.",
  checking: "Also checked automatically every few hours.",
  current: "You're up to date.",
};

function updateHelp(update: UpdateState, updateLocalDaemon: boolean): string {
  if (update.kind === "available") {
    if (updateLocalDaemon) {
      return `Version ${update.version} is ready. Installing restarts the app, daemon, and running bots.`;
    }
    return `Version ${update.version} is ready. Installing relaunches the app.`;
  }
  return UPDATE_HELP[update.kind];
}

interface UpdateActionProps {
  readonly update: UpdateState;
  readonly onCheck: () => void;
  readonly onInstall: () => void;
}

function UpdateAction({ update, onCheck, onInstall }: UpdateActionProps): ReactElement {
  if (update.kind === "available") {
    return (
      <button type="button" className="btn btn-small btn-primary" onClick={onInstall}>
        Restart &amp; update
      </button>
    );
  }
  return (
    <button
      type="button"
      className="btn btn-small"
      disabled={update.kind === "checking"}
      onClick={onCheck}
    >
      {update.kind === "checking" ? "Checking…" : "Check for updates"}
    </button>
  );
}

/** Versions and the manual update check. */
export default function AboutSettings(props: AboutSettingsProps): ReactElement {
  const { endpoint, daemonVersion, addToast } = props;
  const [appVersion, setAppVersion] = useState<string | null>(null);
  const [update, setUpdate] = useState<UpdateState>({ kind: "idle" });
  const [daemonUpdating, setDaemonUpdating] = useState(false);
  const updateLocalDaemon = isLocalEndpoint(endpoint);
  const localDaemonVersion = useLocalDaemonVersion(endpoint, daemonVersion, updateLocalDaemon);
  const daemonNeedsUpdate =
    isTauri() &&
    updateLocalDaemon &&
    appVersion !== null &&
    localDaemonVersion.length > 0 &&
    compareVersions(appVersion, localDaemonVersion) === "newer";

  useEffect(() => {
    if (!isTauri()) {
      return;
    }
    getVersion().then(setAppVersion, () => undefined);
  }, []);

  const check = async (): Promise<void> => {
    setUpdate({ kind: "checking" });
    const version = await checkForAppUpdate();
    setUpdate(version === null ? { kind: "current" } : { kind: "available", version });
  };

  const install = (): void => {
    runAppUpdate(updateLocalDaemon, addToast);
  };

  const updateDaemon = async (): Promise<void> => {
    setDaemonUpdating(true);
    try {
      await installLocalDaemon(localDaemonVersion);
      addToast("info", "Daemon updated", "The local daemon is restarting.");
    } catch (error) {
      captureException(error, "daemon_update");
      const body = error instanceof Error ? error.message : "daemon update failed";
      addToast("error", "Daemon update failed", body, { sticky: true });
    } finally {
      setDaemonUpdating(false);
    }
  };

  return (
    <div className="settings-section">
      <div className="settings-row">
        <div className="settings-row-text">
          <div className="settings-row-label">Gravity</div>
          <div className="settings-row-help">Desktop app version.</div>
        </div>
        <span className="settings-value">{appVersion ?? "dev (browser)"}</span>
      </div>

      <div className="settings-row">
        <div className="settings-row-text">
          <div className="settings-row-label">Daemon</div>
          <div className="settings-row-help">The gravityd this app is connected to.</div>
        </div>
        <div className="settings-row-control">
          <span className="settings-value">
            {localDaemonVersion.length > 0
              ? `${localDaemonVersion}${daemonVersion.length > 0 ? "" : " · not connected"}`
              : "not connected"}
          </span>
          {daemonNeedsUpdate ? (
            <button
              type="button"
              className="btn btn-small"
              disabled={daemonUpdating}
              onClick={() => {
                void updateDaemon();
              }}
            >
              {daemonUpdating ? "Updating…" : "Update daemon"}
            </button>
          ) : null}
        </div>
      </div>

      {isTauri() ? (
        <div className="settings-row">
          <div className="settings-row-text">
            <div className="settings-row-label">Updates</div>
            <div className="settings-row-help">{updateHelp(update, updateLocalDaemon)}</div>
          </div>
          <UpdateAction
            update={update}
            onCheck={() => {
              void check();
            }}
            onInstall={install}
          />
        </div>
      ) : null}
    </div>
  );
}
