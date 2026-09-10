import type { FitAddon } from "@xterm/addon-fit";
import type { Terminal } from "@xterm/xterm";
import type { DaemonApi } from "../protocol/api";

/**
 * Keeps the terminals of recently viewed bots alive across switches.
 *
 * A terminal that survives its pane keeps its screen and scrollback, so
 * returning to a bot needs only a resumed replay of what arrived while it was
 * hidden — no reset, no full-ring replay, no forced repaint. Each entry holds
 * a live xterm instance and the DOM element it rendered into, both expensive,
 * so the cache stays small and evicts least-recently-viewed first.
 */
export interface CachedTerminal {
  readonly term: Terminal;
  readonly fit: FitAddon;
  readonly element: HTMLElement;
  /** Scroll offset the viewport had when parked; the DOM drops it on detach. */
  viewportTop: number;
}

const MAX_CACHED = 3;

interface StoredTerminal {
  readonly client: DaemonApi;
  readonly connectionGeneration: number;
  readonly botId: string;
  readonly terminal: CachedTerminal;
}

/** Array order is recency: `take` removes, `store` re-inserts at the end. */
const cache: StoredTerminal[] = [];

/**
 * xterm's teardown can throw on a renderer whose WebGL context is already
 * gone (detached canvas, reclaimed context). A parked terminal dies out of
 * sight, so its death must never take the app down with it — dropping the
 * references is enough for the rest to be collected.
 */
function dispose(entry: CachedTerminal): void {
  try {
    entry.term.dispose();
  } catch {
    // Already broken beyond disposing.
  }
  entry.element.remove();
}

/** Drops terminals belonging to an earlier endpoint of this client. */
function pruneConnection(client: DaemonApi, connectionGeneration: number): void {
  for (let index = cache.length - 1; index >= 0; index -= 1) {
    const stored = cache[index];
    if (stored.client === client && stored.connectionGeneration !== connectionGeneration) {
      cache.splice(index, 1);
      dispose(stored.terminal);
    }
  }
}

/** Removes and returns the cached terminal for a bot, if one is still warm. */
export function takeCachedTerminal(
  client: DaemonApi,
  connectionGeneration: number,
  botId: string,
): CachedTerminal | undefined {
  pruneConnection(client, connectionGeneration);
  const index = cache.findIndex(
    (stored) =>
      stored.client === client &&
      stored.connectionGeneration === connectionGeneration &&
      stored.botId === botId,
  );
  if (index < 0) {
    return undefined;
  }
  const [stored] = cache.splice(index, 1);
  return stored?.terminal;
}

/** Parks a pane's terminal for reuse, evicting the coldest beyond capacity. */
export function storeCachedTerminal(
  client: DaemonApi,
  connectionGeneration: number,
  botId: string,
  terminal: CachedTerminal,
): void {
  pruneConnection(client, connectionGeneration);
  const existing = cache.findIndex(
    (stored) =>
      stored.client === client &&
      stored.connectionGeneration === connectionGeneration &&
      stored.botId === botId,
  );
  if (existing >= 0) {
    const [replaced] = cache.splice(existing, 1);
    if (replaced !== undefined) {
      dispose(replaced.terminal);
    }
  }
  cache.push({ client, connectionGeneration, botId, terminal });
  while (cache.length > MAX_CACHED) {
    const evicted = cache.shift();
    if (evicted !== undefined) {
      dispose(evicted.terminal);
    }
  }
}

/** Disposes every cached terminal; keeps tests independent of each other. */
export function clearTerminalCache(): void {
  for (const stored of cache) {
    dispose(stored.terminal);
  }
  cache.length = 0;
}
