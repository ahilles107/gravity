import type { ReactElement } from "react";
import type { Bot } from "../../protocol/entities";
import BotAvatar from "../BotAvatar";

interface BotResultRowProps {
  readonly bot: Bot;
  /** Shown subtly beside the name; empty when the project is unknown. */
  readonly projectName: string;
  readonly active: boolean;
  readonly onHover: () => void;
  readonly onOpen: () => void;
}

/** One matching bot in the search overlay: avatar, name, and its project. */
export default function BotResultRow({
  bot,
  projectName,
  active,
  onHover,
  onOpen,
}: BotResultRowProps): ReactElement {
  return (
    <button
      type="button"
      className={`overlay-option search-bot ${active ? "overlay-active" : ""}`}
      onMouseEnter={onHover}
      onMouseDown={(event) => {
        event.preventDefault();
        onOpen();
      }}
    >
      <BotAvatar avatar={bot.avatar} name={bot.name} id={bot.id} size="md" />
      <span className="overlay-label">{bot.name}</span>
      {projectName.length > 0 ? <span className="search-bot-project">{projectName}</span> : null}
    </button>
  );
}
