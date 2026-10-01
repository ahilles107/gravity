import { useCallback, useEffect, useState } from "react";
import type { ReactElement } from "react";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { DaemonApi } from "../../protocol/api";
import type { Bot, Routine } from "../../protocol/entities";
import type { BotTask } from "../../protocol/tasks";
import { errText } from "../../util";
import { TaskRow, UpcomingRow } from "./TaskRow";

interface TasksPanelProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly connected: boolean;
}

/** Waits this long after a burst of bus traffic before refetching. */
const REFRESH_DELAY_MS = 600;

interface Sections {
  readonly now: readonly BotTask[];
  readonly waiting: readonly BotTask[];
  readonly done: readonly BotTask[];
}

export function sections(tasks: readonly BotTask[]): Sections {
  return {
    now: tasks.filter((t) => t.state === "open" && t.role === "assigned"),
    waiting: tasks.filter((t) => t.state === "open" && t.role === "delegated"),
    done: tasks.filter((t) => t.state !== "open"),
  };
}

/** Enabled routines with a next run, soonest first. */
function upcoming(routines: readonly Routine[]): readonly Routine[] {
  const next = routines.filter((r) => r.enabled && r.next_run_at != null);
  // `sort` on a fresh copy, not `toSorted`: the build targets ES2021.
  // oxlint-disable-next-line unicorn/no-array-sort
  next.sort((a, b) => ((a.next_run_at ?? "") < (b.next_run_at ?? "") ? -1 : 1));
  return next;
}

/** What the bot is doing, waiting on, has scheduled, and has finished. */
export default function TasksPanel({ client, bot, connected }: TasksPanelProps): ReactElement {
  const [tasks, setTasks] = useState<readonly BotTask[]>([]);
  const [routines, setRoutines] = useState<readonly Routine[]>([]);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (): Promise<void> => {
    try {
      const [listed, scheduled] = await Promise.all([
        client.request({ type: "list_tasks", bot_id: bot.id }, "tasks"),
        client.request({ type: "list_routines", bot_id: bot.id }, "routines"),
      ]);
      setTasks(listed.tasks);
      setRoutines(scheduled.routines);
      setError(null);
    } catch (failure) {
      setError(errText(failure));
    }
  }, [client, bot.id]);

  useLoadOnConnect(connected, load);

  // Tasks open and close through bus messages and routine runs; refetch after
  // a burst rather than on every frame of it.
  useEffect(() => {
    let timer: number | undefined;
    const soon = (): void => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => void load(), REFRESH_DELAY_MS);
    };
    const off = [client.on("message_new", soon), client.on("routine_run_update", soon)];
    return () => {
      window.clearTimeout(timer);
      for (const unsubscribe of off) {
        unsubscribe();
      }
    };
  }, [client, load]);

  const { now, waiting, done } = sections(tasks);
  const next = upcoming(routines);
  const empty = tasks.length === 0 && next.length === 0;
  return (
    <div className="tasks-panel">
      <div className="files-head">
        <span className="panel-title">Tasks</span>
        <button type="button" className="btn btn-small" onClick={() => void load()}>
          Refresh
        </button>
      </div>
      {error === null ? null : <div className="chat-note chat-error">{error}</div>}
      {empty && error === null ? (
        <div className="files-empty">
          No tasks yet. Work {bot.name} is given or hands out shows up here.
        </div>
      ) : null}
      <Section title="Now" count={now.length}>
        {now.map((task) => (
          <TaskRow key={task.id} task={task} />
        ))}
      </Section>
      <Section title="Waiting on others" count={waiting.length}>
        {waiting.map((task) => (
          <TaskRow key={task.id} task={task} />
        ))}
      </Section>
      <Section title="Upcoming" count={next.length}>
        {next.map((routine) => (
          <UpcomingRow key={routine.id} routine={routine} />
        ))}
      </Section>
      <Section title="Done" count={done.length}>
        {done.map((task) => (
          <TaskRow key={task.id} task={task} />
        ))}
      </Section>
    </div>
  );
}

function Section({
  title,
  count,
  children,
}: {
  readonly title: string;
  readonly count: number;
  readonly children: ReactElement[];
}): ReactElement | null {
  if (count === 0) {
    return null;
  }
  return (
    <section className="tasks-section" aria-label={title}>
      <h3 className="tasks-section-title">
        {title} <span className="tasks-count">{count}</span>
      </h3>
      <ul className="tasks-list">{children}</ul>
    </section>
  );
}
