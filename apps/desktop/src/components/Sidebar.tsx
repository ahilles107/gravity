import { useState } from "react";
import type { ReactElement } from "react";
import { usePinnedBots } from "../app/usePinnedBots";
import type { ConnectionStatus, Endpoint } from "../protocol/connection";
import type { PendingCounts } from "../protocol/decisions";
import type { Bot, Project } from "../protocol/entities";
import { PlusIcon, SearchIcon } from "./sidebar/icons";
import NewProjectForm from "./sidebar/NewProjectForm";
import ControlCenterRow from "./sidebar/ControlCenterRow";
import ProjectSection from "./sidebar/ProjectSection";
import SidebarFooter from "./sidebar/SidebarFooter";
import type { SidebarTreeProps } from "./sidebar/tree";
import type { SettingsCategory } from "./settings/categories";

type SidebarProps = Omit<SidebarTreeProps, "pinnedBotIds" | "onTogglePin" | "onCreateBot"> & {
  readonly status: ConnectionStatus;
  readonly endpoint: Endpoint;
  readonly projects: readonly Project[];
  readonly bots: readonly Bot[];
  readonly onOpenSearch: () => void;
  readonly pendingDecisions: PendingCounts;
  readonly onCreateProject: (name: string) => Promise<unknown>;
  /** The tree gets the sidebar's wrapper around this, which tracks the request. */
  readonly onCreateBot: (projectId: string) => Promise<void>;
  readonly onOpenSettings: (category?: SettingsCategory) => void;
};

export default function Sidebar(props: SidebarProps): ReactElement {
  const { projects, bots, selection, canControl, onSelect } = props;
  const [creatingProject, setCreatingProject] = useState(false);
  const [creatingBot, setCreatingBot] = useState(false);
  const { pinnedBotIds, onTogglePin } = usePinnedBots();

  const closeForm = (): void => {
    setCreatingProject(false);
  };

  // One-click creation has no form to stand in for it, so the request needs a
  // marker of its own: without one the click looks like nothing happened, and
  // an impatient second click quietly creates a second bot.
  const startCreateBot = (projectId: string): void => {
    if (creatingBot) {
      return;
    }
    setCreatingBot(true);
    void props.onCreateBot(projectId).finally(() => {
      setCreatingBot(false);
    });
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-header" data-tauri-drag-region="deep">
        {canControl ? (
          <button
            type="button"
            className="sidebar-add"
            title="New Project"
            aria-label="New Project"
            onClick={() => {
              setCreatingProject(true);
            }}
          >
            <PlusIcon />
          </button>
        ) : null}
      </div>

      <div className="sidebar-search">
        <button
          type="button"
          className="search-field"
          title="Search bots and messages (or press ⌘K for the palette)"
          onClick={props.onOpenSearch}
        >
          <SearchIcon />
          Search
        </button>
      </div>

      <ControlCenterRow
        counts={props.pendingDecisions}
        selected={selection.kind === "control"}
        onSelect={() => onSelect({ kind: "control" })}
      />

      {creatingProject ? (
        <NewProjectForm onCreate={props.onCreateProject} onClose={closeForm} />
      ) : null}
      {creatingBot ? (
        <div className="muted project-empty" role="status">
          Creating bot…
        </div>
      ) : null}

      <div className="sidebar-scroll">
        {projects.map((project) => (
          <ProjectSection
            key={project.id}
            project={project}
            bots={bots.filter((bot) => bot.project_id === project.id)}
            unreadBots={props.unreadBots}
            failedByBot={props.failedByBot}
            nextRun={props.nextRun}
            pinnedBotIds={pinnedBotIds}
            activityByBot={props.activityByBot}
            selection={selection}
            canControl={canControl}
            onSelect={onSelect}
            onCreateBot={startCreateBot}
            onDeleteBot={props.onDeleteBot}
            onDeleteProject={props.onDeleteProject}
            onTogglePin={onTogglePin}
          />
        ))}
        {projects.length === 0 ? (
          <div className="muted project-empty">No projects. Create one to begin.</div>
        ) : null}
      </div>

      <SidebarFooter
        status={props.status}
        endpoint={props.endpoint}
        canControl={canControl}
        onOpenSettings={props.onOpenSettings}
      />
    </aside>
  );
}
