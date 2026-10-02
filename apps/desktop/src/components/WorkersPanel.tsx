import { useCallback, useEffect, useState } from "react";
import type { ReactElement } from "react";
import { useLoadOnConnect } from "../hooks/useLoadOnConnect";
import type { DaemonApi } from "../protocol/api";
import type { WorkerView } from "../protocol/workers";
import { errText, fmtShortTime } from "../util";

interface WorkersPanelProps {
  readonly client: DaemonApi;
  readonly projectId: string;
  readonly connected: boolean;
  readonly canControl: boolean;
}

interface Listing {
  readonly workers: readonly WorkerView[];
  readonly runningHere: number;
  readonly maxHere: number;
}

/** Splits a listing into what runs, what waits, and what finished. */
export function workerSections(workers: readonly WorkerView[]): {
  readonly running: readonly WorkerView[];
  readonly queued: readonly WorkerView[];
  readonly finished: readonly WorkerView[];
} {
  return {
    running: workers.filter((w) => w.state === "running"),
    queued: workers.filter((w) => w.state === "queued"),
    finished: workers.filter((w) => w.state !== "running" && w.state !== "queued"),
  };
}

/**
 * The project's temporary workers: the ones running, the queue waiting for a
 * slot, and those that recently finished. The owner can cancel any spawn that
 * has not finished.
 */
export default function WorkersPanel({
  client,
  projectId,
  connected,
  canControl,
}: WorkersPanelProps): ReactElement {
  const [listing, setListing] = useState<Listing | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request(
        { type: "list_workers", project_id: projectId },
        "workers",
      );
      setListing({
        workers: reply.workers,
        runningHere: reply.running_here,
        maxHere: reply.max_workers_here,
      });
      setError(null);
    } catch (failure) {
      setError(errText(failure));
    }
  }, [client, projectId]);

  useLoadOnConnect(connected, load);

  useEffect(
    () =>
      client.on("workers_updated", (push) => {
        if (push.project_id === projectId) {
          void load();
        }
      }),
    [client, load, projectId],
  );

  const cancel = async (worker: WorkerView): Promise<void> => {
    try {
      await client.request({ type: "cancel_worker", worker_id: worker.id }, "worker");
      await load();
    } catch (failure) {
      setError(errText(failure));
    }
  };

  const { running, queued, finished } = workerSections(listing?.workers ?? []);
  const row = (worker: WorkerView): ReactElement => (
    <WorkerRow
      key={worker.id}
      worker={worker}
      canCancel={canControl && connected}
      onCancel={() => void cancel(worker)}
    />
  );
  return (
    <div className="panel workers-panel">
      <h3 className="panel-title">
        Workers
        {listing === null ? null : (
          <span className="tasks-count">
            {" "}
            {listing.runningHere} of {listing.maxHere} slots in use here
          </span>
        )}
      </h3>
      {error === null ? null : <div className="chat-note chat-error">{error}</div>}
      {listing !== null && listing.workers.length === 0 ? (
        <p className="field-hint">
          No workers yet. Bots spawn temporary workers for pieces of a larger job; they queue here
          when every slot is busy.
        </p>
      ) : null}
      <WorkerSection title="Running" workers={running} row={row} />
      <WorkerSection title="Queued" workers={queued} row={row} />
      <WorkerSection title="Finished" workers={finished} row={row} />
    </div>
  );
}

function WorkerSection({
  title,
  workers,
  row,
}: {
  readonly title: string;
  readonly workers: readonly WorkerView[];
  readonly row: (worker: WorkerView) => ReactElement;
}): ReactElement | null {
  if (workers.length === 0) {
    return null;
  }
  return (
    <section className="tasks-section" aria-label={title}>
      <h4 className="tasks-section-title">
        {title} <span className="tasks-count">{workers.length}</span>
      </h4>
      <ul className="tasks-list">{workers.map(row)}</ul>
    </section>
  );
}

/** Where a worker stands, in a few words. */
function standing(worker: WorkerView): string {
  if (worker.state === "queued") {
    return worker.queue_position === undefined ? "queued" : `#${worker.queue_position} in queue`;
  }
  if (worker.state === "running") {
    return worker.machine === "here" || worker.machine == null
      ? "running"
      : `running on ${worker.machine}`;
  }
  return worker.state;
}

function WorkerRow({
  worker,
  canCancel,
  onCancel,
}: {
  readonly worker: WorkerView;
  readonly canCancel: boolean;
  readonly onCancel: () => void;
}): ReactElement {
  const active = worker.state === "queued" || worker.state === "running";
  const when = worker.finished_at ?? worker.started_at ?? worker.created_at;
  return (
    <li className="task-row">
      <div className="task-row-head">
        <span className="task-who">{worker.name}</span>
        <span className="task-badge">{standing(worker)}</span>
        {worker.parent_name == null ? null : (
          <span className="worker-parent">for {worker.parent_name}</span>
        )}
        <span className="task-when">{fmtShortTime(when)}</span>
        {active ? (
          <button
            type="button"
            className="btn btn-small"
            disabled={!canCancel}
            aria-label={`Cancel ${worker.name}`}
            onClick={onCancel}
          >
            Cancel
          </button>
        ) : null}
      </div>
      <div className="worker-brief" title={worker.brief}>
        {worker.brief}
      </div>
      {worker.note === undefined || worker.note.length === 0 ? null : (
        <div className="worker-note">{worker.note}</div>
      )}
    </li>
  );
}
