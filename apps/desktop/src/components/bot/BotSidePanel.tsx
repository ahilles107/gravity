import type { ReactElement } from "react";
import type { AddToast } from "../../app/useToasts";
import type { DaemonApi } from "../../protocol/api";
import type { Bot } from "../../protocol/entities";
import FilesPanel from "../files/FilesPanel";
import InfoPanel from "../InfoPanel";

export type SideTab = "info" | "files";

const SIDE_TABS: readonly SideTab[] = ["info", "files"];

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
        {SIDE_TABS.map((name) => (
          <button
            key={name}
            type="button"
            className={`tab ${side === name ? "tab-active" : ""}`}
            onClick={() => {
              onSide(name);
            }}
          >
            {name === "info" ? "Info" : "Files"}
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
      ) : (
        <FilesPanel
          client={client}
          bot={bot}
          connected={connected}
          selected={props.openFile}
          onSelect={props.onOpenFile}
        />
      )}
    </>
  );
}
