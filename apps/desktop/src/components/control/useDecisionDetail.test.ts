import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Decision } from "../../protocol/decisions";
import type { ReplyOf, ServerReplyType } from "../../protocol/messages";
import type { RequestBody } from "../../protocol/requests";
import * as dfx from "../../test/decisionFixtures";
import { FakeDaemon } from "../../test/fakeDaemon";
import { useDecisionDetail } from "./useDecisionDetail";

/** A daemon whose replies are released by hand, so two can land out of order. */
class SlowDaemon extends FakeDaemon {
  readonly held: (() => void)[] = [];

  override request<K extends ServerReplyType>(body: RequestBody, reply: K): Promise<ReplyOf<K>> {
    const inner = super.request(body, reply);
    return new Promise<ReplyOf<K>>((resolve, reject) => {
      this.held.push(() => {
        inner.then(resolve, reject);
      });
    });
  }
}

function daemonWith(reply: (body: RequestBody) => Decision): FakeDaemon {
  return new FakeDaemon().onRequest("get_decision", (body) => ({
    type: "decision",
    req_id: "1",
    decision: reply(body),
  }));
}

function mount(client: FakeDaemon, decision: Decision | undefined) {
  const replace = vi.fn<(decision: Decision) => void>();
  const hook = renderHook(({ current }) => useDecisionDetail(client, current, replace), {
    initialProps: { current: decision },
  });
  return { hook, replace };
}

describe("useDecisionDetail", () => {
  // List replies carry neither the thread nor who was told, so the reading
  // pane always needs one more round trip.
  it("fetches the full record and hands it over", async () => {
    const full = dfx.decision({ id: "d1", comments: [dfx.decisionComment()] });
    const client = daemonWith(() => full);
    const { replace } = mount(client, dfx.decision({ id: "d1" }));
    await waitFor(() => {
      expect(replace).toHaveBeenCalledWith(full);
    });
    expect(client.requests.map((item) => item.body)).toEqual([
      { type: "get_decision", decision_id: "d1" },
    ]);
  });

  it("does nothing when nothing is being read", () => {
    const client = daemonWith(() => dfx.decision());
    mount(client, undefined);
    expect(client.requests).toHaveLength(0);
  });

  // Settling or publishing elsewhere is what refreshes "Told".
  it("fetches again once the record changes state", async () => {
    const client = daemonWith(() => dfx.decision({ id: "d1" }));
    const { hook } = mount(client, dfx.decision({ id: "d1" }));
    await waitFor(() => {
      expect(client.requests).toHaveLength(1);
    });
    hook.rerender({ current: dfx.decision({ id: "d1", state: "settled" }) });
    await waitFor(() => {
      expect(client.requests).toHaveLength(2);
    });
  });

  it("does not fetch again when nothing about the record changed", async () => {
    const client = daemonWith(() => dfx.decision({ id: "d1" }));
    const { hook } = mount(client, dfx.decision({ id: "d1" }));
    await waitFor(() => {
      expect(client.requests).toHaveLength(1);
    });
    hook.rerender({ current: dfx.decision({ id: "d1", title: "Waive rule 3?" }) });
    hook.rerender({ current: dfx.decision({ id: "d1" }) });
    expect(client.requests).toHaveLength(1);
  });

  // A slow reply for the record the owner has already left would otherwise
  // overwrite the one they are reading now.
  it("ignores a reply that a newer fetch has already superseded", async () => {
    const stale = dfx.decision({ id: "d1", title: "stale" });
    const fresh = dfx.decision({ id: "d1", state: "settled", title: "fresh" });
    const client = new SlowDaemon();
    let next = stale;
    client.onRequest("get_decision", () => ({ type: "decision", req_id: "1", decision: next }));

    const { hook, replace } = mount(client, dfx.decision({ id: "d1" }));
    await waitFor(() => {
      expect(client.held).toHaveLength(1);
    });
    next = fresh;
    hook.rerender({ current: dfx.decision({ id: "d1", state: "settled" }) });
    await waitFor(() => {
      expect(client.held).toHaveLength(2);
    });

    await act(async () => {
      client.held[1]?.();
      client.held[0]?.();
      await Promise.resolve();
    });

    expect(replace).toHaveBeenCalledTimes(1);
    expect(replace).toHaveBeenCalledWith(fresh);
  });
});
