import { invoke } from "@tauri-apps/api/core";
import { isLocalEndpoint } from "./protocol/connection";
import type { Endpoint } from "./protocol/connection";
import { DEFAULT_ENDPOINT } from "./settings";
import { isTauri } from "./tauri";

export interface DaemonHealth {
  readonly status: string;
  readonly version: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/**
 * Asks the Tauri shell to probe the daemon's `/health` endpoint. Runs in Rust
 * because the webview origin cannot make cross-origin requests to the daemon.
 * `null` means nothing answered (or we are outside the Tauri shell).
 */
export async function probeDaemon(endpoint: Endpoint): Promise<DaemonHealth | null> {
  if (!isTauri()) {
    return null;
  }
  try {
    const result: unknown = await invoke("daemon_health", {
      host: endpoint.host,
      port: endpoint.port,
    });
    if (
      isRecord(result) &&
      typeof result["status"] === "string" &&
      typeof result["version"] === "string"
    ) {
      return { status: result["status"], version: result["version"] };
    }
  } catch {
    // fall through to null
  }
  return null;
}

/**
 * The endpoint the daemon on this machine actually serves on. An install
 * predating the 49777 default keeps serving on its configured port across an
 * upgrade, so the wizard asks the daemon's config instead of assuming the
 * current default. Falls back to the default outside the Tauri shell.
 */
export async function localDaemonEndpoint(): Promise<Endpoint> {
  if (!isTauri()) {
    return DEFAULT_ENDPOINT;
  }
  try {
    const port: unknown = await invoke("local_daemon_port");
    if (typeof port === "number" && Number.isInteger(port) && port > 0) {
      return { host: DEFAULT_ENDPOINT.host, port };
    }
  } catch {
    // fall through to the default
  }
  return DEFAULT_ENDPOINT;
}

/**
 * The daemon's most recent stderr, for explaining an install that never came
 * up. Empty when there is nothing to show.
 */
export async function daemonLogTail(): Promise<string> {
  if (!isTauri()) {
    return "";
  }
  try {
    const tail: unknown = await invoke("daemon_log_tail");
    return typeof tail === "string" ? tail : "";
  } catch {
    return "";
  }
}

/**
 * Installs and starts the bundled daemon as a launchd user agent on this
 * machine. Throws with a human-readable message on failure.
 */
export async function installLocalDaemon(currentVersion?: string): Promise<void> {
  try {
    if (currentVersion === undefined) {
      await invoke("install_local_daemon");
    } else {
      await invoke("install_local_daemon", { currentVersion });
    }
  } catch (error) {
    throw new Error(typeof error === "string" ? error : "daemon install failed", {
      cause: error,
    });
  }
}

/**
 * Whether the daemon at `endpoint` is the app-managed launchd agent on this
 * machine — the only one we can restart. False for a remote daemon, for the
 * workspace-private daemon `scripts/dev.sh` starts, and outside the Tauri
 * shell.
 */
export async function isManagedLocalDaemon(endpoint: Endpoint): Promise<boolean> {
  if (!isTauri() || !isLocalEndpoint(endpoint)) {
    return false;
  }
  try {
    const managed: unknown = await invoke("local_daemon_is_managed", { port: endpoint.port });
    return managed === true;
  } catch {
    return false;
  }
}

/**
 * Bounces the launchd agent running the daemon on this machine, stopping every
 * bot session it is running. Throws with a human-readable message on failure.
 */
export async function restartLocalDaemon(): Promise<void> {
  try {
    await invoke("restart_local_daemon");
  } catch (error) {
    throw new Error(typeof error === "string" ? error : "daemon restart failed", {
      cause: error,
    });
  }
}

/**
 * How the wizard reached a daemon: the bundled local one it found or installed,
 * or an address the user typed, optionally with a device token.
 */
export type SetupConnection =
  | { readonly method: "local"; readonly endpoint: Endpoint }
  | { readonly method: "remote"; readonly endpoint: Endpoint; readonly token?: string };
