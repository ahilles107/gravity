import type { ReactElement } from "react";
import { isStopped } from "../../app/bots";
import type { AddToast } from "../../app/useToasts";
import type { DaemonApi } from "../../protocol/api";
import type { Bot, Routine } from "../../protocol/entities";
import ChatPane from "../chat/ChatPane";
import LinkedChat from "../chat/LinkedChat";
import PermissionCards from "../permissions/PermissionCards";
import { usePermissions } from "../permissions/usePermissions";
import RoutinesPanel from "../RoutinesPanel";
import TerminalPane from "../TerminalPane";
import type { BotTab } from "./BotTabs";

interface BotPanesProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly bots: readonly Bot[];
  readonly tabs: readonly BotTab[];
  readonly active: BotTab;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onOpenFile: (path: string) => void;
  readonly onOpenDecision?: (decisionId: string) => void;
  readonly onRoutinesChanged: (botId: string, routines: readonly Routine[]) => void;
  readonly onToast: AddToast;
}

/** Why the owner cannot write to a bot right now, or null when they can. */
function writeBlockedReason(connected: boolean, canControl: boolean, bot: Bot): string | null {
  if (!connected) {
    return "Not connected to the daemon";
  }
  if (!canControl) {
    return "Read-only connection";
  }
  return isStopped(bot) ? `${bot.name} is stopped` : null;
}

/** Where a linked bot runs, said above its chat. */
function machineNote(bot: Bot): string {
  const machine = bot.peer?.name ?? "another machine";
  const offline =
    bot.peer?.online === false ? " It is offline, so its chat loads once it is back." : "";
  return `${bot.name} runs on ${machine}; its chat is read from there.${offline}`;
}

function paneClass(shown: boolean): string {
  return shown ? "tab-pane" : "tab-pane tab-pane-hidden";
}

/**
 * The bot's tab bodies. Chat and terminal stay mounted when hidden, so
 * switching back keeps the loaded conversation and the terminal buffer.
 */
export default function BotPanes(props: BotPanesProps): ReactElement {
  const { client, bot, tabs, active, connected, canControl, onToast } = props;
  const writeBlocked = writeBlockedReason(connected, canControl, bot);
  const linked = bot.peer != null;
  const permissions = usePermissions(client, bot.id, connected && !linked);
  return (
    <>
      {/* Above every tab: a prompt waits whether the owner reads the chat or the terminal. */}
      <PermissionCards permissions={permissions} canAnswer={connected && canControl} />
      <div className="bot-view-body">
        {tabs.includes("chat") ? (
          <div className={paneClass(active === "chat")}>
            {linked && !client.capabilities.includes("peer_chat") ? (
              <LinkedChat
                client={client}
                bot={bot}
                connected={connected}
                writeBlocked={writeBlocked}
              />
            ) : (
              <ChatPane
                client={client}
                bot={bot}
                connected={connected}
                writeBlocked={writeBlocked}
                onOpenFile={props.onOpenFile}
                onOpenDecision={props.onOpenDecision}
                active={active === "chat"}
                note={linked ? machineNote(bot) : undefined}
              />
            )}
          </div>
        ) : null}
        {tabs.includes("terminal") ? (
          <div className={paneClass(active === "terminal")}>
            {/* The terminal belongs to the user: any `control` connection may type while the bot runs. */}
            <TerminalPane
              client={client}
              botId={bot.id}
              canWrite={canControl && !isStopped(bot)}
              onToast={onToast}
            />
          </div>
        ) : null}
        {active === "routines" ? (
          <div className="tab-pane tab-pane-scroll">
            <RoutinesPanel
              client={client}
              bot={bot}
              bots={props.bots}
              connected={connected}
              canControl={canControl}
              onRoutinesChanged={props.onRoutinesChanged}
              onToast={onToast}
            />
          </div>
        ) : null}
      </div>
    </>
  );
}
