import { useEffect, useRef } from "react";
import type { Bot, Project } from "../protocol/entities";
import { loadLastUsedBotId, loadPinnedBotIds } from "../settings";
import type { Selection } from "./selection";

/**
 * The bot the sidebar renders first: projects in order, and within the first
 * project that has any bots, the pinned strip before the plain rows.
 */
export function firstSidebarBot(
  projects: readonly Project[],
  bots: readonly Bot[],
  pinnedBotIds: readonly string[],
): Bot | undefined {
  for (const project of projects) {
    const own = bots.filter((bot) => bot.project_id === project.id);
    const pinnedId = pinnedBotIds.find((id) => own.some((bot) => bot.id === id));
    const first = pinnedId === undefined ? own[0] : own.find((bot) => bot.id === pinnedId);
    if (first !== undefined) {
      return first;
    }
  }
  return undefined;
}

/**
 * The last-used bot, when the sidebar still renders it.
 *
 * The tree groups bots under their project, so a bot whose project is gone has
 * no row to highlight; it falls through to the first bot rather than opening a
 * view with nothing selected beside it.
 */
export function lastUsedSidebarBot(
  projects: readonly Project[],
  bots: readonly Bot[],
  lastUsedId: string | undefined,
): Bot | undefined {
  if (lastUsedId === undefined) {
    return undefined;
  }
  const bot = bots.find((item) => item.id === lastUsedId);
  if (bot === undefined) {
    return undefined;
  }
  return projects.some((project) => project.id === bot.project_id) ? bot : undefined;
}

/**
 * Opens the last-used bot, or the first bot in the sidebar, once on startup.
 *
 * The main pane is empty until something is selected, so a fresh launch would
 * otherwise show a blank screen. This runs only while nothing is selected yet
 * and only once per session, so a later reconnect or refresh never yanks the
 * user away from the view they picked.
 */
export function useInitialBot(
  projects: readonly Project[],
  bots: readonly Bot[],
  selection: Selection,
  select: (next: Selection) => void,
): void {
  const done = useRef(false);

  useEffect(() => {
    if (done.current || selection.kind !== "none") {
      return;
    }
    const initial =
      lastUsedSidebarBot(projects, bots, loadLastUsedBotId()) ??
      firstSidebarBot(projects, bots, loadPinnedBotIds());
    if (initial === undefined) {
      return;
    }
    done.current = true;
    // oxlint-disable-next-line react/set-state-in-effect
    select({ kind: "bot", botId: initial.id });
  }, [bots, projects, select, selection]);
}
