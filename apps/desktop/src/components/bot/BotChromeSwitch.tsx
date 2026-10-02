import { useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { Bot, NotifyLevel } from "../../protocol/entities";
import { errText } from "../../util";

interface BotChromeSwitchProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onBotUpdated: (bot: Bot) => void;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

/**
 * Whether the bot may also drive the owner's own Chrome. Off by default:
 * every bot has a browser of its own, and this is for the task that needs
 * the owner's logged-in sessions. Changing it restarts the bot's session.
 */
export default function BotChromeSwitch({
  client,
  bot,
  connected,
  canControl,
  onBotUpdated,
  onToast,
}: BotChromeSwitchProps): ReactElement | null {
  const [saving, setSaving] = useState(false);
  if (!client.capabilities.includes("bot_browser") || bot.peer != null) {
    return null;
  }
  const enabled = bot.user_chrome === true;
  const save = async (next: boolean): Promise<void> => {
    setSaving(true);
    try {
      const reply = await client.request(
        { type: "set_bot_user_chrome", bot_id: bot.id, enabled: next },
        "bot",
      );
      onBotUpdated(reply.bot);
      onToast(
        "info",
        next ? "Chrome allowed" : "Chrome off",
        `${reply.bot.name} is restarting ${next ? "with" : "without"} access to your Chrome.`,
      );
    } catch (error) {
      onToast("error", "Could not change Chrome access", errText(error));
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="settings-row bot-chrome">
      <div className="settings-row-text">
        <label className="settings-row-label" htmlFor="bot-user-chrome">
          Can use your Chrome
        </label>
        <div className="settings-row-help">
          {`${bot.name} always has a browser of its own. Allow this only for tasks that need your logged-in sessions; its tabs then open in your Chrome. Restarts the bot.`}
        </div>
      </div>
      <label className="toggle" htmlFor="bot-user-chrome" aria-label="Toggle access to your Chrome">
        <input
          id="bot-user-chrome"
          type="checkbox"
          checked={enabled}
          disabled={!connected || !canControl || saving}
          onChange={(event) => void save(event.target.checked)}
        />
        <span className="toggle-track" />
      </label>
    </div>
  );
}
