import { useCallback, useEffect, useState } from "react";
import type { ReactElement } from "react";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import { useOnBotTurns } from "../../hooks/useOnBotTurns";
import type { BotCommand } from "../../protocol/agents";
import type { DaemonApi } from "../../protocol/api";
import type { Bot } from "../../protocol/entities";
import { errText, fmtTimestamp } from "../../util";

/** Waits this long after the bot's turns change before refetching. */
const REFRESH_DELAY_MS = 600;
/** How often a running background command's output is reread. */
const POLL_MS = 3000;

interface CommandsPanelProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly connected: boolean;
}

const STATUS_LABEL: Readonly<Record<BotCommand["status"], string>> = {
  running: "running",
  done: "done",
  failed: "failed",
  stopped: "stopped",
};

/** "12s", "4m 03s", "1h 05m". */
export function elapsed(from: string, to: string): string {
  const seconds = Math.max(0, Math.round((Date.parse(to) - Date.parse(from)) / 1000));
  if (seconds < 60) {
    return `${seconds}s`;
  }
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return `${minutes}m ${String(seconds % 60).padStart(2, "0")}s`;
  }
  return `${Math.floor(minutes / 60)}h ${String(minutes % 60).padStart(2, "0")}m`;
}

function when(command: BotCommand): string {
  const started = fmtTimestamp(command.started_at);
  return command.ended_at === undefined
    ? started
    : `${started} · ${elapsed(command.started_at, command.ended_at)}`;
}

/** One command: what it is for, the command line, and its output on demand. */
function CommandRow({ command }: { readonly command: BotCommand }): ReactElement {
  const [open, setOpen] = useState(false);
  const title = command.description ?? command.command.split("\n")[0] ?? "";
  return (
    <li className={`command-row command-${command.status}`}>
      <button
        type="button"
        className="command-head"
        aria-expanded={open}
        onClick={() => {
          setOpen((current) => !current);
        }}
      >
        <span className="command-dot" aria-hidden="true" />
        <span className="command-title">{title}</span>
        {command.background ? <span className="command-badge">background</span> : null}
        {command.status === "running" ? null : (
          <span className={`task-badge task-badge-${command.status}`}>
            {command.exit_code == null || command.exit_code === 0
              ? STATUS_LABEL[command.status]
              : `exit ${command.exit_code}`}
          </span>
        )}
      </button>
      <code className="command-line">{command.command}</code>
      <div className="task-when">{when(command)}</div>
      {open ? <pre className="command-output">{command.output ?? "No output yet."}</pre> : null}
    </li>
  );
}

function Section({
  title,
  commands,
}: {
  readonly title: string;
  readonly commands: readonly BotCommand[];
}): ReactElement | null {
  if (commands.length === 0) {
    return null;
  }
  return (
    <section className="tasks-section" aria-label={title}>
      <h3 className="tasks-section-title">
        {title} <span className="tasks-count">{commands.length}</span>
      </h3>
      <ul className="tasks-list">
        {commands.map((command) => (
          <CommandRow key={command.id} command={command} />
        ))}
      </ul>
    </section>
  );
}

/**
 * Everything the bot is running as commands, and what it ran: foreground and
 * background, with each command's output. Live: refetched as the bot works,
 * and polled while a background command is still producing output.
 */
export default function CommandsPanel({
  client,
  bot,
  connected,
}: CommandsPanelProps): ReactElement {
  const [commands, setCommands] = useState<readonly BotCommand[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request(
        { type: "list_bot_commands", bot_id: bot.id },
        "bot_commands",
      );
      setCommands(reply.commands);
      setError(null);
    } catch (failure) {
      setError(errText(failure));
    } finally {
      setLoaded(true);
    }
  }, [client, bot.id]);

  useLoadOnConnect(connected, load);

  useOnBotTurns(client, bot.id, load, REFRESH_DELAY_MS);

  const running = commands.filter((c) => c.status === "running");
  const polling = connected && running.some((c) => c.background);
  useEffect(() => {
    if (!polling) {
      return undefined;
    }
    const timer = window.setInterval(() => void load(), POLL_MS);
    return () => {
      window.clearInterval(timer);
    };
  }, [polling, load]);

  return (
    <div className="tasks-panel tab-pane-scroll">
      {error === null ? null : <div className="chat-note chat-error">{error}</div>}
      {loaded && commands.length === 0 ? (
        <div className="muted memory-note">{`${bot.name} has not run any commands yet.`}</div>
      ) : null}
      <Section title="Running" commands={running} />
      <Section title="Finished" commands={commands.filter((c) => c.status !== "running")} />
    </div>
  );
}
