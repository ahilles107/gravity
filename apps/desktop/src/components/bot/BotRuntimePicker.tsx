import { useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { Bot, BotRuntime, NotifyLevel } from "../../protocol/entities";
import { errText } from "../../util";

interface Props {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onBotUpdated: (bot: Bot) => void;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

export default function BotRuntimePicker({
  client,
  bot,
  connected,
  canControl,
  onBotUpdated,
  onToast,
}: Props): ReactElement | null {
  const current = bot.runtime ?? "claude_code";
  const [runtime, setRuntime] = useState<BotRuntime>(current);
  const [saving, setSaving] = useState(false);
  if (!client.capabilities.includes("bot_runtime")) {
    return null;
  }
  const save = async (): Promise<void> => {
    setSaving(true);
    try {
      const reply = await client.request(
        { type: "set_bot_runtime", bot_id: bot.id, runtime },
        "bot",
      );
      onBotUpdated(reply.bot);
      onToast(
        "info",
        "Runtime saved",
        `${reply.bot.name} is restarting with ${runtime === "codex_cli" ? "Codex CLI" : "Claude Code"}. Startup progress appears in the terminal.`,
      );
    } catch (error) {
      onToast("error", "Runtime update failed", errText(error));
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="field bot-runtime">
      <label className="field-label" htmlFor="bot-runtime">
        Bot runtime
      </label>
      <select
        id="bot-runtime"
        value={runtime}
        disabled={!connected || !canControl || saving}
        onChange={(event) => {
          const value = event.target.value;
          if (value === "claude_code" || value === "codex_cli") {
            setRuntime(value);
          }
        }}
      >
        <option value="claude_code">Claude Code</option>
        <option value="codex_cli">Codex CLI</option>
      </select>
      <span className="field-hint">
        Changing runtime interrupts the current turn. Both use the same workspace and saved facts;
        each keeps its own conversation history. Install and sign in to the selected CLI first.
      </span>
      <div className="panel-actions">
        <button
          type="button"
          className="btn"
          disabled={!connected || !canControl || saving || runtime === current}
          onClick={() => {
            void save();
          }}
        >
          {saving ? "Switching…" : "Change runtime"}
        </button>
      </div>
    </div>
  );
}
