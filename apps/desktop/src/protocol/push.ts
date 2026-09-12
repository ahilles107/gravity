import type { PushOf, ServerPush, ServerPushType } from "./messages";

/** One handler set per push type. */
export type PushHandlerSets = {
  readonly [K in ServerPushType]: Set<(push: PushOf<K>) => void>;
};

export function emptyHandlers(): PushHandlerSets {
  return {
    term: new Set(),
    bot_state: new Set(),
    message_new: new Set(),
    bot_updated: new Set(),
    project_updated: new Set(),
    activity_update: new Set(),
    delivery_update: new Set(),
    routine_run_update: new Set(),
    approval_pending: new Set(),
    notify: new Set(),
    decision_update: new Set(),
    decision_deleted: new Set(),
    decision_comment_new: new Set(),
  };
}

function emitPush<K extends ServerPushType>(
  handlers: PushHandlerSets,
  type: K,
  push: PushOf<K>,
): void {
  for (const handler of handlers[type]) {
    handler(push);
  }
}

/**
 * Route a parsed push to its handler set.
 *
 * The switch is exhaustive on purpose: a new push type without a case here
 * would be parsed and then dropped, which is exactly how `bot_updated` went
 * missing once.
 */
export function dispatchPush(handlers: PushHandlerSets, push: ServerPush): void {
  switch (push.type) {
    case "term":
      emitPush(handlers, "term", push);
      break;
    case "bot_state":
      emitPush(handlers, "bot_state", push);
      break;
    case "message_new":
      emitPush(handlers, "message_new", push);
      break;
    case "bot_updated":
      emitPush(handlers, "bot_updated", push);
      break;
    case "activity_update":
      emitPush(handlers, "activity_update", push);
      break;
    case "delivery_update":
      emitPush(handlers, "delivery_update", push);
      break;
    case "routine_run_update":
      emitPush(handlers, "routine_run_update", push);
      break;
    case "approval_pending":
      emitPush(handlers, "approval_pending", push);
      break;
    case "project_updated":
      emitPush(handlers, "project_updated", push);
      break;
    case "notify":
      emitPush(handlers, "notify", push);
      break;
    case "decision_update":
      emitPush(handlers, "decision_update", push);
      break;
    case "decision_deleted":
      emitPush(handlers, "decision_deleted", push);
      break;
    case "decision_comment_new":
      emitPush(handlers, "decision_comment_new", push);
      break;
    default:
      push satisfies never;
  }
}
