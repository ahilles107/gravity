import type { Bot } from "../../protocol/entities";
import type { RowMenuApi } from "./useRowMenu";
import { useRowMenu } from "./useRowMenu";

interface BotMenuOptions {
  readonly bot: Bot;
  readonly pinned: boolean;
  /** Deleting needs control; pinning is a local view preference and never does. */
  readonly canControl: boolean;
  readonly onTogglePin: () => void;
  readonly onDelete: () => void;
  /** Null when the bot is already at that end, or the order cannot change. */
  readonly onMoveUp?: (() => void) | null;
  readonly onMoveDown?: (() => void) | null;
}

/** The right-click menu shared by a bot's sidebar row and its pinned tile. */
export function useBotMenu({
  bot,
  pinned,
  canControl,
  onTogglePin,
  onDelete,
  onMoveUp = null,
  onMoveDown = null,
}: BotMenuOptions): RowMenuApi {
  return useRowMenu({
    items: [
      { label: pinned ? "Unpin" : "Pin to top", onSelect: onTogglePin },
      ...(onMoveUp === null ? [] : [{ label: "Move up", onSelect: onMoveUp }]),
      ...(onMoveDown === null ? [] : [{ label: "Move down", onSelect: onMoveDown }]),
    ],
    deletion: canControl
      ? {
          label: "Delete",
          title: "Delete bot",
          body: `Delete “${bot.name}”? Its session is stopped and the bot is archived along with its conversations.`,
          confirmLabel: "Delete bot",
          onConfirm: onDelete,
        }
      : null,
  });
}
