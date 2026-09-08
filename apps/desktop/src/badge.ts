import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tauri";

/**
 * Puts the unread total on the macOS dock icon; zero removes the badge.
 * Does nothing in a plain browser (vite dev), which has no dock.
 */
export async function setDockBadge(count: number): Promise<void> {
  if (!isTauri()) {
    return;
  }
  try {
    await invoke("set_dock_badge", { count: Math.max(0, Math.trunc(count)) });
  } catch {
    // A badge the shell refused is not worth interrupting the user over.
  }
}
