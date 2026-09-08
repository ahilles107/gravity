import { useEffect, useRef, useState } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { BusMessage } from "../../protocol/entities";
import { errText } from "../../util";

const DEBOUNCE_MS = 250;

export interface SearchResults {
  /** Null until a result for the current query has arrived. */
  readonly results: readonly BusMessage[] | null;
  readonly error: string | null;
  readonly searching: boolean;
}

interface Fetched {
  readonly query: string;
  readonly results: readonly BusMessage[];
  readonly error: string | null;
}

/**
 * Debounced message search. State is keyed by the query it was fetched for, so
 * a stale result is never shown next to a newer query.
 */
export function useMessageSearch(
  client: DaemonApi,
  query: string,
  enabled: boolean,
): SearchResults {
  const [fetched, setFetched] = useState<Fetched | null>(null);
  const seqRef = useRef(0);

  const active = enabled && query.length > 0;

  useEffect(() => {
    if (!active) {
      return;
    }
    const seq = seqRef.current + 1;
    seqRef.current = seq;
    const timer = setTimeout(() => {
      void (async (): Promise<void> => {
        const next = await search(client, query);
        if (seqRef.current === seq) {
          setFetched(next);
        }
      })();
    }, DEBOUNCE_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [active, client, query]);

  const current = active && fetched?.query === query ? fetched : null;
  return {
    results: current === null ? null : current.results,
    error: current === null ? null : current.error,
    searching: active && current === null,
  };
}

async function search(client: DaemonApi, query: string): Promise<Fetched> {
  try {
    const reply = await client.request({ type: "search", query }, "search_results");
    return { query, results: reply.search_results, error: null };
  } catch (error) {
    return { query, results: [], error: errText(error) };
  }
}
