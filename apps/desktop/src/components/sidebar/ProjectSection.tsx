import { useState } from "react";
import type { ReactElement } from "react";
import type { Bot, Project } from "../../protocol/entities";
import BotRow from "./BotRow";
import { MoreIcon } from "./icons";
import PinnedBots from "./PinnedBots";
import type { SidebarTreeProps } from "./tree";
import { useProjectMenu } from "./useProjectMenu";

export interface ProjectSectionProps extends SidebarTreeProps {
  readonly project: Project;
  readonly bots: readonly Bot[];
}

/** One project and its collapsible list of bots. */
export default function ProjectSection(props: ProjectSectionProps): ReactElement {
  const { project, bots, selection, canControl, onSelect, pinnedBotIds } = props;
  const [collapsed, setCollapsed] = useState(false);
  const openSettings = (): void => {
    onSelect({ kind: "project", projectId: project.id });
  };
  const menu = useProjectMenu({
    project,
    bots,
    canControl,
    onOpenSettings: openSettings,
    onCreateBot: () => {
      props.onCreateBot(project.id);
    },
    onDelete: () => {
      void props.onDeleteProject(project.id);
    },
  });

  const pinnedBots = pinnedBotIds
    .map((id) => bots.find((bot) => bot.id === id))
    .filter((bot): bot is Bot => bot !== undefined);
  const unpinnedBots = bots.filter((bot) => !pinnedBotIds.includes(bot.id));
  const selectedBotId = selection.kind === "bot" ? selection.botId : null;

  return (
    <section className="project-section">
      <div
        className={`project-header ${
          selection.kind === "project" && selection.projectId === project.id
            ? "project-header-selected"
            : ""
        }`}
      >
        <button
          type="button"
          className="project-name"
          title={collapsed ? "Show bots" : "Hide bots"}
          aria-expanded={!collapsed}
          onClick={() => {
            setCollapsed((prev) => !prev);
          }}
          onContextMenu={menu.onContextMenu}
        >
          {project.name}
        </button>
        <button
          type="button"
          className="project-menu-btn"
          title="Project menu"
          aria-label="Project menu"
          aria-haspopup="menu"
          onClick={menu.onOpenFrom}
        >
          <MoreIcon />
        </button>
      </div>

      {collapsed ? null : (
        <>
          <PinnedBots
            bots={pinnedBots}
            unreadBots={props.unreadBots}
            selectedBotId={selectedBotId}
            canControl={canControl}
            onSelect={(botId) => {
              onSelect({ kind: "bot", botId });
            }}
            onDelete={(botId) => {
              void props.onDeleteBot(botId);
            }}
            onTogglePin={props.onTogglePin}
          />

          {unpinnedBots.map((bot) => (
            <BotRow
              key={bot.id}
              bot={bot}
              unread={props.unreadBots[bot.id] ?? 0}
              failed={props.failedByBot.get(bot.id) ?? 0}
              next={props.nextRun[bot.id]}
              activity={props.activityByBot[bot.id]}
              selected={selectedBotId === bot.id}
              canControl={canControl}
              onClick={() => {
                onSelect({ kind: "bot", botId: bot.id });
              }}
              onDelete={() => {
                void props.onDeleteBot(bot.id);
              }}
              onTogglePin={() => {
                props.onTogglePin(bot.id);
              }}
            />
          ))}
          {bots.length === 0 ? <div className="muted project-empty">No bots yet.</div> : null}
        </>
      )}

      {menu.overlays}
    </section>
  );
}
