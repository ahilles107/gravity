import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tauri";

/**
 * Shows a project's directory in the OS file manager. The daemon does not
 * report project paths, so the shell derives it from the immutable `dir_name`.
 * A no-op outside the Tauri shell, where there is no file manager to reach.
 */
export async function revealProject(dirName: string): Promise<void> {
  if (!isTauri()) {
    return;
  }
  try {
    await invoke("reveal_project", { dirName });
  } catch {
    // Nothing to fall back to: the menu entry simply does nothing.
  }
}

/** Shows the selected bot's workspace in the OS file manager. */
export async function revealBotWorkspace(workspacePath: string): Promise<void> {
  if (!isTauri()) {
    return;
  }
  try {
    await invoke("reveal_bot_workspace", { workspacePath });
  } catch {
    // Nothing to fall back to: the button simply does nothing.
  }
}
