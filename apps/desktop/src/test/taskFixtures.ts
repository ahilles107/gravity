import type { BotTask } from "../protocol/tasks";
import { FakeDaemon } from "./fakeDaemon";
import * as fx from "./fixtures";

export function task(over: Partial<BotTask> = {}): BotTask {
  return {
    id: "t1",
    state: "open",
    role: "assigned",
    other: { id: "b2", name: "lead" },
    request: "Port the updater to Windows.",
    created_at: "2025-01-15T10:00:00Z",
    deadline_at: "2025-01-16T10:00:00Z",
    ...over,
  };
}

export const sampleTasks: readonly BotTask[] = [
  task(),
  task({
    id: "t2",
    role: "delegated",
    other: { id: "b3", name: "windev", machine: "win-pc" },
    request: "Sign the MSI with the release certificate.",
  }),
  task({
    id: "t3",
    state: "done",
    request: "Review the installer changes.",
    result: "Reviewed — two nits, both fixed.",
    closed_at: "2025-01-15T09:00:00Z",
  }),
  task({ id: "t4", state: "expired", role: "delegated", request: "Benchmark the cold start." }),
];

/** A daemon answering `list_tasks` and `list_routines` for the Tasks panel. */
export function tasksDaemon(tasks: readonly BotTask[] = sampleTasks): FakeDaemon {
  return new FakeDaemon()
    .onRequest("list_tasks", () => ({ type: "tasks", req_id: "1", bot_id: "b1", tasks }))
    .onRequest("list_routines", () => ({
      type: "routines",
      req_id: "2",
      routines: [
        fx.routine({ id: "r1", name: "nightly-report", next_run_at: "2025-01-16T02:00:00Z" }),
        fx.routine({
          id: "r2",
          name: "paused",
          enabled: false,
          next_run_at: "2025-01-16T03:00:00Z",
        }),
      ],
    }));
}
