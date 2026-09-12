import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { isTauri } from "./tauri";

/**
 * Raise a native OS notification.
 *
 * Reserved for the two things the owner cannot discover any other way: a bot
 * marking a decision urgent, and a deadline coming within a day. Everything
 * else is a toast, which does not interrupt.
 *
 * Does nothing in a plain browser (vite dev), and nothing if the user declined
 * the permission — a notification the shell refused is not worth a second
 * prompt, and the sidebar badge still carries the same count.
 */
export async function notifyNatively(title: string, body: string): Promise<void> {
  if (!isTauri()) {
    return;
  }
  try {
    const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
    if (granted) {
      sendNotification({ title, body });
    }
  } catch {
    // Never let a notification failure surface as an error the owner has to
    // deal with; they are already being told in the window.
  }
}
