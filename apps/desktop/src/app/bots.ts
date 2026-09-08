import type { Bot } from "../protocol/entities";

/** True when the bot has no live runtime session and can be started. */
export function isStopped(bot: Bot): boolean {
  return bot.state === "stopped" || bot.state === "crashed" || bot.state === "auth_failed";
}
