import type { ReactElement } from "react";
import type { DaemonState } from "../app/useDaemonState";
import type { AddToast } from "../app/useToasts";
import heroArt from "../assets/empty/hero.png";
import quietArt from "../assets/empty/quiet.png";
import type { DaemonApi } from "../protocol/api";
import { connectionStatusLabel } from "../protocol/connection";
import BotView from "./BotView";
import ProjectView from "./ProjectView";
import NewProjectForm from "./sidebar/NewProjectForm";

interface MainPaneProps {
  readonly client: DaemonApi;
  readonly daemon: DaemonState;
  readonly addToast: AddToast;
  readonly onCreateProject: (name: string) => Promise<void>;
  readonly onRenameProject: (projectId: string, name: string) => Promise<void>;
  readonly onDeleteProject: (projectId: string) => Promise<void>;
}

interface EmptyStateProps {
  readonly daemon: DaemonState;
  readonly onCreateProject: (name: string) => Promise<void>;
}

/** The first-launch pane: nothing exists yet, so it carries the onboarding. */
function WelcomeState({ daemon, onCreateProject }: EmptyStateProps): ReactElement {
  return (
    // No header here, so the whole pane doubles as the window drag strip.
    <div className="empty-pane" data-tauri-drag-region="deep">
      <div className="empty-state">
        <img className="empty-art-hero" src={heroArt} alt="" draggable={false} />
        <h1>Welcome to Gravity</h1>
        <p>Projects hold your bots. Create one to get started.</p>
        {daemon.canControl ? <NewProjectForm onCreate={onCreateProject} /> : null}
      </div>
    </div>
  );
}

function EmptyState(props: EmptyStateProps): ReactElement {
  const { daemon } = props;
  const { endpoint, status, connected } = daemon;
  if (connected && daemon.projects.length === 0) {
    return <WelcomeState {...props} />;
  }
  return (
    // No header here, so the whole pane doubles as the window drag strip.
    <div className="empty-pane" data-tauri-drag-region="deep">
      <div className="empty-state">
        <img className="empty-art-quiet" src={quietArt} alt="" draggable={false} />
        <h1>Gravity</h1>
        <p>Select a bot from the sidebar, or create one to get started.</p>
        {connected ? null : (
          <p className={`conn-hint conn-${status}`}>
            {`Daemon: ${connectionStatusLabel(status)} (${endpoint.host}:${endpoint.port})`}
          </p>
        )}
      </div>
    </div>
  );
}

/** Renders whichever view the current selection points at. */
export default function MainPane(props: MainPaneProps): ReactElement {
  const { client, daemon, addToast } = props;
  const { selection, bots, connected, canControl } = daemon;

  if (selection.kind === "project") {
    const project = daemon.projects.find((item) => item.id === selection.projectId);
    return project === undefined ? (
      <EmptyState daemon={daemon} onCreateProject={props.onCreateProject} />
    ) : (
      <ProjectView
        key={project.id}
        project={project}
        bots={bots.filter((item) => item.project_id === project.id)}
        connected={connected}
        canControl={canControl}
        onRename={props.onRenameProject}
        onDelete={props.onDeleteProject}
      />
    );
  }

  if (selection.kind === "bot") {
    const bot = bots.find((item) => item.id === selection.botId);
    return bot === undefined ? (
      <EmptyState daemon={daemon} onCreateProject={props.onCreateProject} />
    ) : (
      <BotView
        key={bot.id}
        client={client}
        bot={bot}
        bots={bots}
        connected={connected}
        canControl={canControl}
        onBotUpdated={daemon.applyBotUpdate}
        onRoutinesChanged={daemon.updateBotRoutines}
        onToast={addToast}
      />
    );
  }

  return <EmptyState daemon={daemon} onCreateProject={props.onCreateProject} />;
}
