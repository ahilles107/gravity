import type { KeyboardEvent, ReactElement } from "react";
import { useScrollSelectedIntoView } from "../../hooks/useScrollSelectedIntoView";
import type { Bot, BotActivity } from "../../protocol/entities";
import { fmtShortTime } from "../../util";
import BotAvatar from "../BotAvatar";
import type { DropPlace } from "./reorder";
import type { RowDragHandlers } from "./useBotDrag";
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
  /** Null at the top of the list, or when the order cannot be changed. */
  readonly onMoveUp?: (() => void) | null;
  readonly onMoveDown?: (() => void) | null;
  readonly drag?: RowDragHandlers;
  readonly dragging?: boolean;
  /** Set while another row is dragged over this one. */
  readonly dropPlace?: DropPlace | null;
}

/** Alt/Option+Up and Alt/Option+Down move the focused row one step. */
function moveKey(
  event: KeyboardEvent<HTMLElement>,
  onMoveUp: (() => void) | null,
  onMoveDown: (() => void) | null,
): void {
  if (!event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) {
    return;
  }
  const move =
    event.key === "ArrowUp" ? onMoveUp : event.key === "ArrowDown" ? onMoveDown : undefined;
  if (move === undefined) {
    return;
  }
  event.preventDefault();
  move?.();
}

function rowClass(selected: boolean, dragging: boolean, dropPlace: DropPlace | null): string {
  return [
    "row",
    "bot-row",
    selected ? "row-selected" : "",
    dragging ? "bot-row-dragging" : "",
    dropPlace === null ? "" : `bot-row-drop-${dropPlace}`,
  ]
    .filter((name) => name.length > 0)
    .join(" ");
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

/** The name, time, preview and badges beside a row's avatar. */
function BotRowBody({
  bot,
  unread,
  failed,
  next,
  activity,
}: Pick<BotRowProps, "bot" | "unread" | "failed" | "next" | "activity">): ReactElement {
  return (
    <span className="bot-row-body">
      <span className="bot-row-top">
        <span className="bot-row-name">{bot.name}</span>
        {activity !== undefined ? (
          <span className="bot-row-time">{fmtShortTime(activity.at)}</span>
        ) : null}
      </span>
      <span className="bot-row-bottom">
        <span className="bot-row-preview">{previewOf(bot, activity)}</span>
        {next !== undefined ? <span className="row-next">⏱ {fmtShortTime(next)}</span> : null}
        {failed > 0 ? (
          <span className="badge badge-failed" title={`${failed} failed deliveries`}>
            {failed}
          </span>
        ) : null}
        {unread > 0 ? <span className="badge badge-unread">{unread}</span> : null}
      </span>
    </span>
  );
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
  onMoveUp = null,
  onMoveDown = null,
  drag,
  dragging = false,
  dropPlace = null,
}: BotRowProps): ReactElement {
  const menu = useBotMenu({
    bot,
    pinned: false,
    canControl,
    onTogglePin,
    onDelete,
    onMoveUp,
    onMoveDown,
  });
  const rowRef = useScrollSelectedIntoView<HTMLButtonElement>(selected);

  return (
    <>
      <button
        ref={rowRef}
        type="button"
        className={rowClass(selected, dragging, dropPlace)}
        data-bot-id={bot.id}
        aria-keyshortcuts={canControl ? "Alt+ArrowUp Alt+ArrowDown" : undefined}
        onClick={onClick}
        onContextMenu={menu.onContextMenu}
        onKeyDown={(event) => {
          moveKey(event, onMoveUp, onMoveDown);
        }}
        onPointerDown={drag?.onPointerDown}
        onPointerMove={drag?.onPointerMove}
        onClickCapture={drag?.onClickCapture}
        title={bot.state_reason.length > 0 ? `${bot.state}: ${bot.state_reason}` : bot.state}
      >
        <span className="bot-row-avatar">
          <BotAvatar avatar={bot.avatar} name={bot.name} id={bot.id} size="lg" />
          <span className={`dot dot-${bot.state}`} />
        </span>

        <BotRowBody bot={bot} unread={unread} failed={failed} next={next} activity={activity} />
      </button>

      {menu.overlays}
    </>
  );
}
