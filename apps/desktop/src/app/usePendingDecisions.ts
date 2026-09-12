import { useCallback, useEffect, useState } from "react";
import { useLoadOnConnect } from "../hooks/useLoadOnConnect";
import type { DaemonApi } from "../protocol/api";
import type { PendingCounts } from "../protocol/decisions";

const EMPTY: PendingCounts = { by_project: {}, total: 0, urgent: 0, due_soon: 0 };

/**
 * How many decisions are waiting on the owner, for the sidebar row and the
 * dock badge.
 *
 * Counted by the daemon rather than derived from a loaded list, so the badge is
 * right before the Control center has ever been opened — which is the whole
 * point of it: a question a bot asked while nobody was looking should be
 * visible without going to find it.
 *
 * A daemon without the `decisions` capability answers with an error; that
 * degrades to zero rather than failing the connection.
 */
export function usePendingDecisions(client: DaemonApi, connected: boolean): PendingCounts {
  const [counts, setCounts] = useState<PendingCounts>(EMPTY);

  const load = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request({ type: "count_pending_decisions" }, "pending_decisions");
      setCounts(reply.counts);
    } catch {
      setCounts(EMPTY);
    }
  }, [client]);

  useLoadOnConnect(connected, load);

  // Any change to any decision can move the count, and the pushes are cheap;
  // recounting is one query and keeps the badge honest without a poll.
  useEffect(() => {
    const off = [
      client.on("decision_update", () => void load()),
      client.on("decision_deleted", () => void load()),
    ];
    return () => {
      for (const unsubscribe of off) {
        unsubscribe();
      }
    };
  }, [client, load]);

  return counts;
}
