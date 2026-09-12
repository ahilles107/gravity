// Wire parsing for server frames: type guards only, no casts.

import type {
  ReplyOf,
  ServerMessage,
  ServerPushType,
  ServerReply,
  ServerReplyType,
} from "./messages";

/**
 * Every push type the parser accepts. Keyed by `ServerPushType` rather than a
 * loose string set: `bot_updated` was declared in the protocol types and
 * handled downstream, but missing here, so the frame was dropped as unknown
 * and bots created by bots never reached the sidebar. A missing key is now a
 * type error instead of a silently discarded frame.
 */
const PUSH_TYPES: Readonly<Record<ServerPushType, true>> = {
  term: true,
  bot_state: true,
  message_new: true,
  bot_updated: true,
  project_updated: true,
  activity_update: true,
  delivery_update: true,
  routine_run_update: true,
  approval_pending: true,
  notify: true,
  decision_update: true,
  decision_deleted: true,
  decision_comment_new: true,
};

const REPLY_TYPES: ReadonlySet<string> = new Set(
  Object.keys({
    hello_ok: true,
    error: true,
    ok: true,
    project: true,
    projects: true,
    bot: true,
    bots: true,
    bot_activity: true,
    bot_revisions: true,
    message: true,
    messages: true,
    conversations: true,
    routine: true,
    routines: true,
    routine_run: true,
    routine_runs: true,
    signal: true,
    deliveries: true,
    search_results: true,
    diagnostics: true,
    config: true,
    device: true,
    devices: true,
    attached: true,
    decision: true,
    decisions: true,
    decision_comment: true,
    pending_decisions: true,
    publish_result: true,
    tag: true,
    tags: true,
  } satisfies Record<ServerReplyType, true>),
);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isServerMessage(value: unknown): value is ServerMessage {
  if (!isRecord(value) || typeof value["type"] !== "string") {
    return false;
  }
  const type = value["type"];
  if (type in PUSH_TYPES) {
    return true;
  }
  if (REPLY_TYPES.has(type)) {
    return typeof value["req_id"] === "string";
  }
  return false;
}

/** Parses one wire frame; returns null for malformed or unknown frames. */
export function parseServerMessage(raw: string): ServerMessage | null {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return null;
  }
  return isServerMessage(value) ? value : null;
}

export function isReply(message: ServerMessage): message is ServerReply {
  return "req_id" in message;
}

export function replyIs<K extends ServerReplyType>(
  reply: ServerReply,
  type: K,
): reply is ReplyOf<K> {
  return reply.type === type;
}
