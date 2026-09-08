/**
 * Scrubbing for text that leaves the machine as error telemetry.
 *
 * Error messages and daemon logs are worth reporting verbatim — they are the
 * only thing that explains a failure we cannot reproduce. Two things in them
 * identify a person: the home directory baked into absolute paths, and the
 * daemon's tokens. Both have a fixed shape, so they can be removed without
 * destroying the rest of the text.
 */

/** `/Users/alice/.gravity` -> `/Users/~/.gravity`, keeping the path readable. */
const HOME_DIR = /\/Users\/[^/\s:;,)"'\]]+/g;

/** `random_token` in the daemon is 32 random bytes, hex-encoded. */
const HEX_SECRET = /\b[0-9a-fA-F]{32,}\b/g;

/** Long enough for a daemon error, short enough to stay a sane event property. */
export const MESSAGE_LIMIT = 500;

/** A log tail only explains anything with several lines intact. */
export const LOG_LIMIT = 2000;

function scrub(text: string): string {
  return text.replace(HOME_DIR, "/Users/~").replace(HEX_SECRET, "[redacted]");
}

/** Scrubs and clamps, keeping the start — for messages and stacks. */
export function redact(text: string, limit: number): string {
  const scrubbed = scrub(text);
  return scrubbed.length > limit ? `${scrubbed.slice(0, limit)}…` : scrubbed;
}

/** Scrubs and clamps, keeping the end — for log tails, where the last lines matter. */
export function redactTail(text: string, limit: number): string {
  const scrubbed = scrub(text);
  return scrubbed.length > limit ? `…${scrubbed.slice(scrubbed.length - limit)}` : scrubbed;
}
