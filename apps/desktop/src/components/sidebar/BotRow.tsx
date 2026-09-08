import type { ReactElement } from "react";
import { useScrollSelectedIntoView } from "../../hooks/useScrollSelectedIntoView";
import type { Bot, BotActivity } from "../../protocol/entities";
import { fmtShortTime } from "../../util";
import BotAvatar from "../BotAvatar";
import { useBotMenu } from "./useBotMenu";

interface BotRowProps {
  readonly bot: Bot;
  readonly unread: number;
  readonly failed: number;
  readonly next: string | undefined;
  /** The bot's newest turn or bus message, when it has said anything. */
  readonly activity: BotActivity | undefined;
  readonly selected: boolean;
  readonly canControl: boolean;
  readonly onClick: () => void;
  readonly onDelete: () => void;
  readonly onTogglePin: () => void;
}

/**
 * The preview line: what the bot last said, prefixed with the sender when
 * someone else said it, falling back to the description for a silent bot.
 */
function previewOf(bot: Bot, activity: BotActivity | undefined): string {
  if (activity === undefined) {
    return bot.description;
  }
  return activity.from.length > 0 ? `${activity.from}: ${activity.text}` : activity.text;
}

/** One bot entry in the sidebar tree, with state dot, badges and a right-click menu. */
export default function BotRow({
  bot,
  unread,
  failed,
  next,
  activity,
  selected,
  canControl,
  onClick,
  onDelete,
  onTogglePin,
}: BotRowProps): ReactElement {
  const menu = useBotMenu({ bot, pinned: false, canControl, onTogglePin, onDelete });
  const preview = previewOf(bot, activity);
  const rowRef = useScrollSelectedIntoView<HTMLButtonElement>(selected);

  return (
    <>
      <button
        ref={rowRef}
        type="button"
        className={`row bot-row ${selected ? "row-selected" : ""}`}
        onClick={onClick}
        onContextMenu={menu.onContextMenu}
        title={bot.state_reason.length > 0 ? `${bot.state}: ${bot.state_reason}` : bot.state}
      >
        <span className="bot-row-avatar">
          <BotAvatar avatar={bot.avatar} name={bot.name} id={bot.id} size="lg" />
          <span className={`dot dot-${bot.state}`} />
        </span>

        <span className="bot-row-body">
          <span className="bot-row-top">
            <span className="bot-row-name">{bot.name}</span>
            {activity !== undefined ? (
              <span className="bot-row-time">{fmtShortTime(activity.at)}</span>
            ) : null}
          </span>
          <span className="bot-row-bottom">
            <span className="bot-row-preview">{preview}</span>
            {next !== undefined ? <span className="row-next">⏱ {fmtShortTime(next)}</span> : null}
            {failed > 0 ? (
              <span className="badge badge-failed" title={`${failed} failed deliveries`}>
                {failed}
              </span>
            ) : null}
            {unread > 0 ? <span className="badge badge-unread">{unread}</span> : null}
          </span>
        </span>
      </button>

      {menu.overlays}
    </>
  );
}
