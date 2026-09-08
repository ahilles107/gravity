import type { ReactElement } from "react";
import { useScrollSelectedIntoView } from "../../hooks/useScrollSelectedIntoView";
import type { Bot } from "../../protocol/entities";
import BotAvatar from "../BotAvatar";
import { useBotMenu } from "./useBotMenu";

interface PinnedBotProps {
  readonly bot: Bot;
  readonly unread: number;
  readonly selected: boolean;
  readonly canControl: boolean;
  readonly onClick: () => void;
  readonly onDelete: () => void;
  readonly onTogglePin: () => void;
}

/** One pinned bot: a large avatar tile with a state dot and the name underneath. */
function PinnedBot({
  bot,
  unread,
  selected,
  canControl,
  onClick,
  onDelete,
  onTogglePin,
}: PinnedBotProps): ReactElement {
  const menu = useBotMenu({ bot, pinned: true, canControl, onTogglePin, onDelete });
  const tileRef = useScrollSelectedIntoView<HTMLButtonElement>(selected);

  return (
    <>
      <button
        ref={tileRef}
        type="button"
        className={`pin-tile ${selected ? "pin-tile-selected" : ""}`}
        onClick={onClick}
        onContextMenu={menu.onContextMenu}
        title={bot.state_reason.length > 0 ? `${bot.state}: ${bot.state_reason}` : bot.state}
      >
        <span className="pin-avatar">
          <BotAvatar avatar={bot.avatar} name={bot.name} id={bot.id} size="lg" />
          <span className={`dot dot-${bot.state}`} />
          {unread > 0 ? <span className="badge badge-unread pin-badge">{unread}</span> : null}
        </span>
        <span className="pin-name">{bot.name}</span>
      </button>

      {menu.overlays}
    </>
  );
}

interface PinnedBotsProps {
  readonly bots: readonly Bot[];
  readonly unreadBots: Readonly<Record<string, number>>;
  readonly selectedBotId: string | null;
  readonly canControl: boolean;
  readonly onSelect: (botId: string) => void;
  readonly onDelete: (botId: string) => void;
  readonly onTogglePin: (botId: string) => void;
}

/** The strip of pinned bots above a project's rows. Renders nothing when empty. */
export default function PinnedBots({
  bots,
  unreadBots,
  selectedBotId,
  canControl,
  onSelect,
  onDelete,
  onTogglePin,
}: PinnedBotsProps): ReactElement | null {
  if (bots.length === 0) {
    return null;
  }

  return (
    <div className={`pin-strip ${bots.length === 1 ? "pin-strip-single" : ""}`}>
      {bots.map((bot) => (
        <PinnedBot
          key={bot.id}
          bot={bot}
          unread={unreadBots[bot.id] ?? 0}
          selected={selectedBotId === bot.id}
          canControl={canControl}
          onClick={() => {
            onSelect(bot.id);
          }}
          onDelete={() => {
            onDelete(bot.id);
          }}
          onTogglePin={() => {
            onTogglePin(bot.id);
          }}
        />
      ))}
    </div>
  );
}
