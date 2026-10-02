import type { ReactElement } from "react";
import type { AddToast } from "../../app/useToasts";
import type { DaemonApi } from "../../protocol/api";
import type { Bot } from "../../protocol/entities";
import CommandsPanel from "../commands/CommandsPanel";
import FilesPanel from "../files/FilesPanel";
import InfoPanel from "../InfoPanel";
import MemoryPanel from "../memory/MemoryPanel";
import TasksPanel from "../tasks/TasksPanel";

export type SideTab = "info" | "tasks" | "files" | "commands" | "memory";

const SIDE_TABS: readonly SideTab[] = ["info", "tasks", "files", "commands", "memory"];

const SIDE_LABEL: Readonly<Record<SideTab, string>> = {
  info: "Info",
  tasks: "Tasks",
  files: "Files",
  commands: "Commands",
  memory: "Memory",
};

/** The side tabs this daemon serves. */
function sideTabs(capabilities: readonly string[]): readonly SideTab[] {
  return SIDE_TABS.filter((tab) => tab !== "commands" || capabilities.includes("bot_commands"));
}

interface BotSidePanelProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly side: SideTab;
  readonly onSide: (side: SideTab) => void;
  readonly openFile: string | null;
  readonly onOpenFile: (path: string | null) => void;
  readonly onBotUpdated: (bot: Bot) => void;
  readonly onToast: AddToast;
}

/** The right-hand panel: the bot's info, or the project's files. */
export default function BotSidePanel(props: BotSidePanelProps): ReactElement {
  const { client, bot, connected, side, onSide } = props;
  return (
    <>
      <nav className="tabs side-tabs">
        {sideTabs(client.capabilities).map((name) => (
          <button
            key={name}
            type="button"
            className={`tab ${side === name ? "tab-active" : ""}`}
            onClick={() => {
              onSide(name);
            }}
          >
            {SIDE_LABEL[name]}
          </button>
        ))}
      </nav>
      {side === "info" ? (
        <InfoPanel
          client={client}
          bot={bot}
          connected={connected}
          canControl={props.canControl}
          onBotUpdated={props.onBotUpdated}
          onToast={props.onToast}
        />
      ) : null}
      {side === "tasks" ? <TasksPanel client={client} bot={bot} connected={connected} /> : null}
      {side === "commands" ? (
        <CommandsPanel key={bot.id} client={client} bot={bot} connected={connected} />
      ) : null}
      {side === "memory" ? (
        <MemoryPanel key={bot.id} client={client} bot={bot} connected={connected} />
      ) : null}
      {side === "files" ? (
        <FilesPanel
          client={client}
          bot={bot}
          connected={connected}
          selected={props.openFile}
          onSelect={props.onOpenFile}
        />
      ) : null}
    </>
  );
}
