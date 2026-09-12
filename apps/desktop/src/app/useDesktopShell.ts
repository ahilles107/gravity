import { useDockBadgePref } from "../prefs";
import { useDockBadge } from "./useDockBadge";
import type { AddToast } from "./useToasts";
import { useWindowShortcut } from "./useWindowShortcut";

/** What the desktop shell mirrors from app state: the dock badge and the global shortcut. */
export function useDesktopShell(
  unreadBots: Readonly<Record<string, number>>,
  pendingDecisions: number,
  addToast: AddToast,
): void {
  useDockBadge(unreadBots, pendingDecisions, useDockBadgePref());
  useWindowShortcut(addToast);
}
