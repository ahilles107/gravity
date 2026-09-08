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
}

/** The right-click menu shared by a bot's sidebar row and its pinned tile. */
export function useBotMenu({
  bot,
  pinned,
  canControl,
  onTogglePin,
  onDelete,
}: BotMenuOptions): RowMenuApi {
  return useRowMenu({
    items: [{ label: pinned ? "Unpin" : "Pin to top", onSelect: onTogglePin }],
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
