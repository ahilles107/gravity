import { useEffect, useRef } from "react";
import { captureException } from "../../analytics";
import type { DaemonApi } from "../../protocol/api";
import type { Decision } from "../../protocol/decisions";

/**
 * Fetches the full record for whatever is being read.
 *
 * List replies carry neither the thread nor who was told, so the reading pane
 * needs one more round trip. It is repeated when the record settles or is
 * published elsewhere, which is what refreshes "Told".
 */
export function useDecisionDetail(
  client: DaemonApi,
  decision: Decision | undefined,
  replace: (decision: Decision) => void,
): void {
  const generation = useRef(0);
  // The key: a record settling or being published elsewhere is a new fetch.
  const key =
    decision === undefined
      ? undefined
      : `${decision.id}\u0000${decision.state}\u0000${decision.published_at ?? ""}`;

  useEffect(() => {
    const id = key?.split("\u0000")[0];
    if (id === undefined) {
      return;
    }
    generation.current += 1;
    const mine = generation.current;
    const fetch = async (): Promise<void> => {
      try {
        const reply = await client.request({ type: "get_decision", decision_id: id }, "decision");
        if (mine === generation.current) {
          replace(reply.decision);
        }
      } catch (error) {
        captureException(error, "decision_list");
      }
    };
    void fetch();
  }, [client, key, replace]);
}
