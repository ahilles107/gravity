import { useCallback, useMemo } from "react";
import { capture, captureException } from "../analytics";
import type { DaemonApi } from "../protocol/api";
import type { Bot } from "../protocol/entities";
import { errText } from "../util";
import type { Selection } from "./selection";
import type { AddToast } from "./useToasts";

export interface DaemonActions {
  /** Resolves to the new project's id, or undefined when creation failed. */
  readonly createProject: (name: string) => Promise<string | undefined>;
  readonly renameProject: (projectId: string, name: string) => Promise<void>;
  readonly deleteProject: (projectId: string) => Promise<void>;
  readonly createBot: (projectId: string) => Promise<void>;
  /** Creates a project and its first bot, so a fresh install lands on a bot. */
  readonly createProjectWithBot: (name: string) => Promise<void>;
  readonly deleteBot: (botId: string) => Promise<void>;
}

interface ActionDeps {
  readonly client: DaemonApi;
  readonly addToast: AddToast;
  readonly refreshAll: () => Promise<void>;
  readonly applyBotUpdate: (bot: Bot) => void;
  readonly select: (next: Selection) => void;
}

/** Mutating control-plane calls, each surfacing failures as a toast. */
export function useDaemonActions(deps: ActionDeps): DaemonActions {
  const { client, addToast, refreshAll, applyBotUpdate, select } = deps;

  const createProject = useCallback(
    async (name: string): Promise<string | undefined> => {
      try {
        const reply = await client.request({ type: "create_project", name }, "project");
        capture("project_created", {});
        await refreshAll();
        return reply.project.id;
      } catch (error) {
        captureException(error, "project_create");
        addToast("error", "Create project failed", errText(error));
        return undefined;
      }
    },
    [addToast, client, refreshAll],
  );

  const renameProject = useCallback(
    async (projectId: string, name: string): Promise<void> => {
      try {
        const reply = await client.request(
          { type: "update_project", project_id: projectId, name },
          "project",
        );
        capture("project_renamed", {});
        await refreshAll();
        addToast("info", "Project renamed", `Now called ${reply.project.name}.`);
      } catch (error) {
        captureException(error, "project_rename");
        addToast("error", "Rename project failed", errText(error));
      }
    },
    [addToast, client, refreshAll],
  );

  /**
   * Deleting a project archives every bot inside it, so anything the main pane
   * was showing is gone by the time this returns; the selection is cleared
   * rather than left pointing at an archived row.
   */
  const deleteProject = useCallback(
    async (projectId: string): Promise<void> => {
      try {
        await client.request({ type: "delete_project", project_id: projectId }, "ok");
        capture("project_deleted", {});
        select({ kind: "none" });
        await refreshAll();
      } catch (error) {
        captureException(error, "project_delete");
        addToast("error", "Delete project failed", errText(error));
      }
    },
    [addToast, client, refreshAll, select],
  );

  const createBot = useCallback(
    async (projectId: string): Promise<void> => {
      try {
        const reply = await client.request({ type: "create_bot", project_id: projectId }, "bot");
        capture("bot_created", {});
        applyBotUpdate(reply.bot);
        select({ kind: "bot", botId: reply.bot.id });
        await refreshAll();
      } catch (error) {
        captureException(error, "bot_create");
        addToast("error", "Create bot failed", errText(error));
      }
    },
    [addToast, applyBotUpdate, client, refreshAll, select],
  );

  const createProjectWithBot = useCallback(
    async (name: string): Promise<void> => {
      const projectId = await createProject(name);
      if (projectId !== undefined) {
        await createBot(projectId);
      }
    },
    [createBot, createProject],
  );

  const deleteBot = useCallback(
    async (botId: string): Promise<void> => {
      try {
        await client.request({ type: "delete_bot", bot_id: botId }, "ok");
        capture("bot_deleted", {});
        await refreshAll();
      } catch (error) {
        captureException(error, "bot_delete");
        addToast("error", "Delete bot failed", errText(error));
      }
    },
    [addToast, client, refreshAll],
  );

  return useMemo(
    () => ({
      createProject,
      renameProject,
      deleteProject,
      createBot,
      createProjectWithBot,
      deleteBot,
    }),
    [createBot, createProject, createProjectWithBot, deleteBot, deleteProject, renameProject],
  );
}
