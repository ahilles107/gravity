import { describe, expect, it, vi } from "vitest";
import type { PushOf, ServerPush } from "./messages";
import { dispatchPush, emptyHandlers } from "./push";

describe("dispatchPush", () => {
  it("routes each push to the handlers registered for its type", () => {
    const handlers = emptyHandlers();
    const onState = vi.fn<(push: PushOf<"bot_state">) => void>();
    const onDecision = vi.fn<(push: PushOf<"decision_update">) => void>();
    handlers.bot_state.add(onState);
    handlers.decision_update.add(onDecision);

    const state: ServerPush = {
      type: "bot_state",
      bot_id: "b1",
      state: "ready",
      reason: "",
      at: "2026-09-12T00:00:00Z",
    };
    dispatchPush(handlers, state);
    expect(onState).toHaveBeenCalledWith(state);
    expect(onDecision).not.toHaveBeenCalled();
  });

  it("delivers to every handler on a type", () => {
    const handlers = emptyHandlers();
    const first = vi.fn<(push: PushOf<"decision_deleted">) => void>();
    const second = vi.fn<(push: PushOf<"decision_deleted">) => void>();
    handlers.decision_deleted.add(first);
    handlers.decision_deleted.add(second);
    dispatchPush(handlers, { type: "decision_deleted", decision_id: "d1" });
    expect(first).toHaveBeenCalled();
    expect(second).toHaveBeenCalled();
  });

  // The switch is exhaustive at compile time for exactly this reason: a push
  // type declared and then not routed is silently dropped, which is how
  // `bot_updated` once went missing.
  it("has a case for every declared push type", () => {
    const handlers = emptyHandlers();
    for (const type of Object.keys(handlers)) {
      const seen = vi.fn<(push: ServerPush) => void>();
      handlers[type as keyof typeof handlers].add(seen as never);
      dispatchPush(handlers, { type } as ServerPush);
      expect(seen, `no case routes '${type}'`).toHaveBeenCalled();
    }
  });
});
