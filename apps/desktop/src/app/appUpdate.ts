import { captureException } from "../analytics";
import { installAppUpdate, relaunchApp } from "../updater";
import type { AddToast } from "./useToasts";

async function install(updateLocalDaemon: boolean, addToast: AddToast): Promise<void> {
  let daemonError: string | null;
  try {
    daemonError = await installAppUpdate(updateLocalDaemon);
  } catch (error) {
    captureException(error, "app_update");
    const body = error instanceof Error ? error.message : "update install failed";
    addToast("error", "Update failed", body, { sticky: true });
    return;
  }
  if (daemonError === null) {
    return;
  }
  captureException(new Error(daemonError), "daemon_update");
  addToast(
    "error",
    "Daemon not updated",
    `The app updated but its local daemon did not: ${daemonError}`,
    {
      sticky: true,
      action: {
        label: "Relaunch",
        run: () => {
          relaunchApp().catch((error: unknown) => {
            captureException(error, "app_update");
          });
        },
      },
    },
  );
}

/**
 * Installs an app update and reports both halves of it. A successful install
 * restarts the app and nothing below runs. A daemon refresh that failed after
 * the app itself updated becomes a sticky toast whose action performs the
 * relaunch, so the message is read before the window goes away.
 */
export function runAppUpdate(updateLocalDaemon: boolean, addToast: AddToast): void {
  void install(updateLocalDaemon, addToast);
}
