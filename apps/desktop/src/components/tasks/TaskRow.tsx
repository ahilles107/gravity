import { useState } from "react";
import type { ReactElement } from "react";
import type { Routine } from "../../protocol/entities";
import type { BotTask } from "../../protocol/tasks";
import { fmtTimestamp } from "../../util";

const STATE_LABEL: Readonly<Record<BotTask["state"], string>> = {
  open: "open",
  done: "done",
  cancelled: "cancelled",
  expired: "expired",
};

function counterpart(task: BotTask): string {
  const machine = task.other.machine == null ? "" : ` @ ${task.other.machine}`;
  return task.role === "assigned"
    ? `From ${task.other.name}${machine}`
    : `To ${task.other.name}${machine}`;
}

function when(task: BotTask): string {
  if (task.state !== "open") {
    return task.closed_at == null ? "" : fmtTimestamp(task.closed_at);
  }
  return task.deadline_at == null ? "" : `due ${fmtTimestamp(task.deadline_at)}`;
}

/** One task: who it is with, what was asked, and how it ended. */
export function TaskRow({ task }: { readonly task: BotTask }): ReactElement {
  const [open, setOpen] = useState(false);
  return (
    <li className={`task-row task-${task.state}`}>
      <button
        type="button"
        className="task-row-head"
        aria-expanded={open}
        onClick={() => {
          setOpen((current) => !current);
        }}
      >
        <span className="task-who">{counterpart(task)}</span>
        {task.state === "open" ? null : (
          <span className={`task-badge task-badge-${task.state}`}>{STATE_LABEL[task.state]}</span>
        )}
        <span className="task-when">{when(task)}</span>
      </button>
      <div className={open ? "task-request" : "task-request task-clamp"}>{task.request}</div>
      {open && task.result != null ? <div className="task-result">{task.result}</div> : null}
    </li>
  );
}

/** A routine's next scheduled run. */
export function UpcomingRow({ routine }: { readonly routine: Routine }): ReactElement {
  return (
    <li className="task-row task-upcoming">
      <div className="task-row-head">
        <span className="task-who">Routine {routine.name}</span>
        <span className="task-when">
          {routine.next_run_at == null ? "" : fmtTimestamp(routine.next_run_at)}
        </span>
      </div>
      <div className="task-request task-clamp">{routine.prompt}</div>
    </li>
  );
}
