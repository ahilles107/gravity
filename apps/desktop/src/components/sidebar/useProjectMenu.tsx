import type { Bot, Project } from "../../protocol/entities";
import { revealProject } from "../../reveal";
import { deletionBody } from "../ProjectView";
import type { RowMenuApi } from "./useRowMenu";
import { useRowMenu } from "./useRowMenu";

interface ProjectMenuOptions {
  readonly project: Project;
  readonly bots: readonly Bot[];
  readonly canControl: boolean;
  /** Opens the project's settings view, where the name is edited. */
  readonly onOpenSettings: () => void;
  /**
   * Creates a bot in this project, so a project other than the selected one is
   * still one click away from a new bot.
   */
  readonly onCreateBot: () => void;
  readonly onDelete: () => void;
}

/**
 * The menu on a project header, shared by its right-click and its ⋯ button.
 * Deleting archives every bot in the project, so it is confirmed with the same
 * count the settings view shows.
 */
export function useProjectMenu({
  project,
  bots,
  canControl,
  onOpenSettings,
  onCreateBot,
  onDelete,
}: ProjectMenuOptions): RowMenuApi {
  return useRowMenu({
    items: [
      ...(canControl ? [{ label: "New Bot", onSelect: onCreateBot }] : []),
      { label: "Project settings", onSelect: onOpenSettings },
      {
        label: "Reveal in Finder",
        onSelect: () => {
          void revealProject(project.dir_name);
        },
      },
    ],
    deletion: canControl
      ? {
          label: "Delete project",
          title: "Delete project",
          body: deletionBody(project, bots.length),
          confirmLabel: "Delete project",
          onConfirm: onDelete,
        }
      : null,
  });
}
