import { useEffect } from "react";
import { captureException } from "../analytics";
import { getPrefs, updatePrefs } from "../prefs";
import { applyToggleWindowShortcut, formatShortcut } from "../windowShortcut";
import type { AddToast } from "./useToasts";

/**
 * Registers the saved show/hide shortcut once at launch. Settings applies
 * later changes itself, where a refusal can be shown inline; a refusal here
 * (another app claimed the keys since last run) clears the preference so
 * Settings does not present a dead shortcut as live.
 */
export function useWindowShortcut(addToast: AddToast): void {
  useEffect(() => {
    const shortcut = getPrefs().toggleWindowShortcut;
    if (shortcut.length === 0) {
      return;
    }
    applyToggleWindowShortcut(shortcut).catch((error: unknown) => {
      captureException(error, "window_shortcut");
      updatePrefs({ toggleWindowShortcut: "" });
      addToast(
        "warn",
        "Shortcut unavailable",
        `${formatShortcut(shortcut)} could not be registered; another app may be using it.`,
      );
    });
  }, [addToast]);
}
