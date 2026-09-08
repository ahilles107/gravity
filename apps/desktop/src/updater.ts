import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tauri";

/**
 * Asks the shell whether a newer app build is published. Returns the new
 * version, or null when this build is current, the endpoint is unreachable,
 * or we are outside the Tauri shell — an update check must never surface an
 * error to the user.
 */
export async function checkForAppUpdate(): Promise<string | null> {
  if (!isTauri()) {
    return null;
  }
  try {
    const result: unknown = await invoke("check_for_update");
    if (
      typeof result === "object" &&
      result !== null &&
      "version" in result &&
      typeof result.version === "string" &&
      result.version.length > 0
    ) {
      return result.version;
    }
  } catch {
    // fall through to null
  }
  return null;
}

/**
 * Downloads and installs the pending update, refreshes an app-managed local
 * daemon when requested, then relaunches the app. Resolves only when the app
 * updated but the daemon refresh failed, with that failure's message — a full
 * success restarts the app instead of returning. Throws with a human-readable
 * message when the app update itself fails.
 */
export async function installAppUpdate(updateLocalDaemon: boolean): Promise<string | null> {
  try {
    const daemonError: unknown = await invoke("install_update", { updateLocalDaemon });
    return typeof daemonError === "string" ? daemonError : null;
  } catch (error) {
    throw new Error(typeof error === "string" ? error : "update install failed", {
      cause: error,
    });
  }
}

/** Relaunches the app into an update already installed on disk. */
export async function relaunchApp(): Promise<void> {
  await invoke("relaunch_app");
}
