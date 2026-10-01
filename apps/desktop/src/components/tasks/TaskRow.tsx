import { useLayoutEffect, useRef, useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { Routine } from "../../protocol/entities";
import type { BotTask } from "../../protocol/tasks";
import { errText, fmtTimestamp } from "../../util";
import ChatMarkdown from "../chat/ChatMarkdown";

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
 * Text clamped to two lines, with "Show more" when the clamp hides some of it
 * or `hasMore` says there is more elsewhere. Without `onMore` it expands in
 * place.
 */
function Clamped({
  text,
  hasMore = false,
  onMore,
}: {
  readonly text: string;
  readonly hasMore?: boolean;
  readonly onMore?: () => void;
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

  return (
    <>
      <div ref={body} className={open ? "task-request" : "task-request task-clamp"}>
        {text}
      </div>
      {cut || hasMore || open ? (
        <button
          type="button"
          className="task-more"
          aria-expanded={open}
          onClick={() => {
            if (onMore === undefined) {
              setOpen((current) => !current);
            } else {
              onMore();
            }
          }}
        >
          {open ? "Show less" : "Show more"}
        </button>
      ) : null}
    </>
  );
}

interface TaskRowProps {
  readonly task: BotTask;
  readonly client: DaemonApi;
  readonly botId: string;
}

/**
 * One task: who it is with, what was asked, and how it ended. The list holds
 * previews; opening a row loads the whole request and result when the preview
 * was cut, and shows them as the markdown bots write.
 */
export function TaskRow({ task, client, botId }: TaskRowProps): ReactElement {
  const [open, setOpen] = useState(false);
  const [full, setFull] = useState<BotTask | null>(null);
  const [error, setError] = useState<string | null>(null);
  const shown = full ?? task;
  const cutByDaemon = task.request_truncated === true || task.result_truncated === true;

  const toggle = async (): Promise<void> => {
    setOpen((current) => !current);
    if (open || !cutByDaemon || full !== null) {
      return;
    }
    try {
      const reply = await client.request(
        { type: "get_task", bot_id: botId, task_id: task.id },
        "task",
      );
      setFull(reply.task);
    } catch (failure) {
      setError(errText(failure));
    }
  };

  return (
    <li className={`task-row task-${task.state}`}>
      <div className="task-row-head">
        <span className="task-who">{counterpart(task)}</span>
        {task.state === "open" ? null : (
          <span className={`task-badge task-badge-${task.state}`}>{STATE_LABEL[task.state]}</span>
        )}
        <span className="task-when">{when(task)}</span>
      </div>
      {open ? (
        <>
          <div className="task-request task-full">
            <ChatMarkdown>{shown.request}</ChatMarkdown>
          </div>
          {shown.result == null ? null : (
            <div className="task-result">
              <ChatMarkdown>{shown.result}</ChatMarkdown>
            </div>
          )}
          {error === null ? null : <div className="chat-note chat-error">{error}</div>}
          <button type="button" className="task-more" aria-expanded onClick={() => void toggle()}>
            Show less
          </button>
        </>
      ) : (
        <Clamped
          text={task.request}
          hasMore={cutByDaemon || task.result != null}
          onMore={() => void toggle()}
        />
      )}
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
