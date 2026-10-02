import { useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { Bot, NotifyLevel } from "../../protocol/entities";
import { errText } from "../../util";
import ConfirmDialog from "../overlay/ConfirmDialog";

interface BotSessionActionsProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

type Action = "restart" | "clear";

interface Copy {
  readonly request: "restart_bot" | "clear_bot_session";
  readonly title: (name: string) => string;
  readonly body: (name: string) => string;
  readonly confirm: string;
  readonly done: (name: string) => string;
}

const COPY: Readonly<Record<Action, Copy>> = {
  restart: {
    request: "restart_bot",
    title: (name) => `Restart ${name}?`,
    body: (name) =>
      `${name}'s session restarts and picks its conversation back up. If it was in the middle of something, it is told what it was doing and which tasks are open, so it carries on. Its files, memory and tasks are kept.`,
    confirm: "Restart",
    done: (name) => `${name} is restarting and will pick up where it left off.`,
  },
  clear: {
    request: "clear_bot_session",
    title: (name) => `Clear ${name}'s conversation?`,
    body: (name) =>
      `${name} restarts with a fresh conversation, without this one's history, which leaves the chat. Its files, its memory (FACTS.md) and its tasks are kept, and it is told what it was working on so it can carry on.`,
    confirm: "Clear conversation",
    done: (name) => `${name} is starting a fresh conversation and will carry on its work.`,
  },
};

/**
 * Restart the bot's session, or clear its conversation. Either way it is told
 * what it left unfinished, so no work is lost. Also available to the phone,
 * through the same requests.
 */
export default function BotSessionActions({
  client,
  bot,
  connected,
  canControl,
  onToast,
}: BotSessionActionsProps): ReactElement | null {
  const [confirming, setConfirming] = useState<Action | null>(null);
  const [busy, setBusy] = useState(false);
  if (!canControl || !client.capabilities.includes("restart_bot")) {
    return null;
  }
  const run = async (action: Action): Promise<void> => {
    const copy = COPY[action];
    setBusy(true);
    try {
      await client.request({ type: copy.request, bot_id: bot.id }, "ok");
      onToast(
        "info",
        action === "restart" ? "Restarting" : "Conversation cleared",
        copy.done(bot.name),
      );
    } catch (error) {
      onToast("error", action === "restart" ? "Restart failed" : "Clear failed", errText(error));
    } finally {
      setBusy(false);
    }
  };
  const open = confirming === null ? null : COPY[confirming];
  return (
    <>
      <button
        type="button"
        className="btn btn-small"
        disabled={!connected || busy}
        title="Restart the session; it picks up where it left off"
        onClick={() => {
          setConfirming("restart");
        }}
      >
        Restart
      </button>
      <button
        type="button"
        className="btn btn-small"
        disabled={!connected || busy}
        title="Start a fresh conversation; work, memory and tasks are kept"
        onClick={() => {
          setConfirming("clear");
        }}
      >
        Clear chat
      </button>
      {open === null || confirming === null ? null : (
        <ConfirmDialog
          title={open.title(bot.name)}
          body={open.body(bot.name)}
          confirmLabel={open.confirm}
          onConfirm={() => {
            const action = confirming;
            setConfirming(null);
            void run(action);
          }}
          onCancel={() => {
            setConfirming(null);
          }}
        />
      )}
    </>
  );
}
