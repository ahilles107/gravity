import { useCallback, useEffect, useState } from "react";
import type { ReactElement } from "react";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { DaemonApi } from "../../protocol/api";
import type { RoutineRun } from "../../protocol/entities";
import { errText, fmtTimestamp } from "../../util";
import TableHead from "../TableHead";

interface RunHistoryProps {
  readonly client: DaemonApi;
  readonly routineId: string;
  readonly connected: boolean;
  readonly canControl: boolean;
}

const RUN_LIMIT = 20;

function isUnfinished(run: RoutineRun): boolean {
  return run.state === "scheduled" || run.state === "running";
}

/** Why the occurrence exists, and which attempt this is. */
function sourceLabel(run: RoutineRun): string {
  return run.attempt > 1 ? `${run.source} · try ${run.attempt}` : run.source;
}

/** Recent occurrences of one routine, kept live via `routine_run_update`. */
export default function RunHistory({
  client,
  routineId,
  connected,
  canControl,
}: RunHistoryProps): ReactElement {
  const [runs, setRuns] = useState<readonly RoutineRun[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request(
        { type: "list_routine_runs", routine_id: routineId, limit: RUN_LIMIT },
        "routine_runs",
      );
      setRuns(reply.routine_runs);
      setError(null);
    } catch (err) {
      setError(errText(err));
    }
  }, [client, routineId]);

  useLoadOnConnect(connected, load);

  useEffect(() => {
    return client.on("routine_run_update", (push) => {
      if (push.routine_run.routine_id !== routineId) {
        return;
      }
      setRuns((prev) => {
        const list = prev ?? [];
        const others = list.filter((run) => run.id !== push.routine_run.id);
        return [push.routine_run, ...others].slice(0, RUN_LIMIT);
      });
    });
  }, [client, routineId]);

  const cancel = useCallback(
    async (run: RoutineRun): Promise<void> => {
      try {
        await client.request({ type: "cancel_routine_run", routine_run_id: run.id }, "routine_run");
      } catch (err) {
        setError(errText(err));
      }
    },
    [client],
  );

  if (error !== null) {
    return <div className="muted">Failed to load runs: {error}</div>;
  }
  if (runs === null) {
    return <div className="muted">Loading runs…</div>;
  }
  if (runs.length === 0) {
    return <div className="muted">No runs yet.</div>;
  }
  return (
    <table className="runs-table">
      <TableHead
        columns={["Scheduled for", "Source", "State", "Started", "Finished", "Error", ""]}
      />
      <tbody>
        {runs.map((run) => (
          <tr key={run.id}>
            <td>{fmtTimestamp(run.scheduled_for)}</td>
            <td className="run-source">{sourceLabel(run)}</td>
            <td>
              <span className={`run-state run-${run.state}`}>{run.state}</span>
            </td>
            <td>{typeof run.started_at === "string" ? fmtTimestamp(run.started_at) : "–"}</td>
            <td>{typeof run.finished_at === "string" ? fmtTimestamp(run.finished_at) : "–"}</td>
            <td className="run-error">{typeof run.error === "string" ? run.error : ""}</td>
            <td>
              {canControl && isUnfinished(run) ? (
                <button
                  type="button"
                  className="btn btn-small"
                  disabled={!connected}
                  onClick={() => {
                    void cancel(run);
                  }}
                >
                  Cancel
                </button>
              ) : null}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
