import { useLayoutEffect, useRef, useState } from "react";
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

/**
 * Text clamped to two lines, with "Show more" when the clamp hid something
 * or there is more (`extra`) to reveal.
 */
function Clamped({
  text,
  extra,
}: {
  readonly text: string;
  readonly extra?: ReactElement | null;
}): ReactElement {
  const [open, setOpen] = useState(false);
  const [cut, setCut] = useState(false);
  const body = useRef<HTMLDivElement | null>(null);

  // Whether two lines hide part of the text; measured, since it depends on width.
  useLayoutEffect(() => {
    const element = body.current;
    if (element !== null && !open) {
      setCut(element.scrollHeight > element.clientHeight + 1);
    }
    // A new text is a new length to measure; the effect reads the DOM.
    // oxlint-disable-next-line react/exhaustive-effect-dependencies
  }, [text, open]);

  const more = cut || (extra != null && !open);
  return (
    <>
      <div ref={body} className={open ? "task-request" : "task-request task-clamp"}>
        {text}
      </div>
      {open ? extra : null}
      {more || open ? (
        <button
          type="button"
          className="task-more"
          aria-expanded={open}
          onClick={() => {
            setOpen((current) => !current);
          }}
        >
          {open ? "Show less" : "Show more"}
        </button>
      ) : null}
    </>
  );
}

/** One task: who it is with, what was asked, and how it ended. */
export function TaskRow({ task }: { readonly task: BotTask }): ReactElement {
  return (
    <li className={`task-row task-${task.state}`}>
      <div className="task-row-head">
        <span className="task-who">{counterpart(task)}</span>
        {task.state === "open" ? null : (
          <span className={`task-badge task-badge-${task.state}`}>{STATE_LABEL[task.state]}</span>
        )}
        <span className="task-when">{when(task)}</span>
      </div>
      <Clamped
        text={task.request}
        extra={task.result == null ? null : <div className="task-result">{task.result}</div>}
      />
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
      <Clamped text={routine.prompt} />
    </li>
  );
}
