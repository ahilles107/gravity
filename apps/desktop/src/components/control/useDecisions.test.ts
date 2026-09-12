import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Decision } from "../../protocol/decisions";
import type { NotifyLevel } from "../../protocol/entities";
import * as dfx from "../../test/decisionFixtures";
import { FakeDaemon } from "../../test/fakeDaemon";
import { useDecisions } from "./useDecisions";

const OPEN = dfx.decision({ id: "d1", comments: [] });
const DRAFT = dfx.decision({
  id: "d2",
  state: "answered",
  ruling: { text: "Start today.", answered_at: "2026-09-11T00:00:00Z", answered_by: "owner" },
});
const HELD = dfx.decision({ id: "d3", state: "held" });
const SETTLED = dfx.decision({
  id: "d4",
  state: "settled",
  published_at: "2026-09-10T00:00:00Z",
});
const WITHDRAWN = dfx.decision({
  id: "d5",
  state: "withdrawn",
  withdrawn_reason: "the listing went live",
  published_at: "2026-09-09T00:00:00Z",
});

const BY_STATE: Readonly<Record<string, readonly Decision[]>> = {
  open: [OPEN, DRAFT],
  held: [HELD],
  settled: [SETTLED],
  withdrawn: [WITHDRAWN],
};

function seed(): FakeDaemon {
  return new FakeDaemon()
    .onRequest("list_decisions", (body) => ({
      type: "decisions",
      req_id: "1",
      decisions: body.type === "list_decisions" ? (BY_STATE[body.state ?? ""] ?? []) : [],
    }))
    .onRequest("list_tags", () => ({ type: "tags", req_id: "1", tags: [dfx.tag()] }));
}

function mount(daemon: FakeDaemon) {
  const onToast = vi.fn<(level: NotifyLevel, title: string, body: string) => void>();
  const hook = renderHook(() => useDecisions(daemon, true, onToast));
  return { hook, onToast };
}

async function mounted(daemon: FakeDaemon) {
  const harness = mount(daemon);
  await waitFor(() => {
    expect(harness.hook.result.current.loaded).toBe(true);
  });
  return harness;
}

describe("loading", () => {
  // `all` excludes withdrawn and sorts by deadline, so one capped list would
  // drop the newest rulings first. Four lists is the only honest shape.
  it("asks for each state separately, uncapped, plus the taxonomy", async () => {
    const daemon = seed();
    await mounted(daemon);
    expect(daemon.requests.map((item) => item.body)).toEqual([
      { type: "list_decisions", state: "open", limit: 500 },
      { type: "list_decisions", state: "held", limit: 500 },
      { type: "list_decisions", state: "settled", limit: 500 },
      { type: "list_decisions", state: "withdrawn", limit: 500 },
      { type: "list_tags" },
    ]);
  });

  it("partitions what came back by state", async () => {
    const { hook } = await mounted(seed());
    const api = hook.result.current;
    expect(api.waiting.map((item) => item.id)).toEqual(["d1", "d2"]);
    expect(api.drafts.map((item) => item.id)).toEqual(["d2"]);
    expect(api.held.map((item) => item.id)).toEqual(["d3"]);
    expect(api.registry.map((item) => item.id)).toEqual(["d4", "d5"]);
    expect(api.settled.map((item) => item.id)).toEqual(["d4"]);
    expect(api.tags).toHaveLength(1);
  });

  // Past 500 the ledger used to simply stop, and the "3 of 41" summary was
  // computed from the truncated store so it agreed with itself.
  it("follows the cursor rather than stopping at one page", async () => {
    const page = Array.from({ length: 500 }, (_, i) =>
      dfx.decision({ id: `s${i}`, state: "settled" }),
    );
    const tail = [dfx.decision({ id: "last", state: "settled" })];
    let settledCalls = 0;
    const daemon = new FakeDaemon()
      .onRequest("list_decisions", (body) => {
        if (body.type !== "list_decisions" || body.state !== "settled") {
          return { type: "decisions", req_id: "1", decisions: [] };
        }
        settledCalls += 1;
        return { type: "decisions", req_id: "1", decisions: settledCalls === 1 ? page : tail };
      })
      .onRequest("list_tags", () => ({ type: "tags", req_id: "1", tags: [] }));
    const { hook, onToast } = await mounted(daemon);

    expect(settledCalls).toBe(2);
    expect(
      daemon.requests.some(
        (item) => item.body.type === "list_decisions" && item.body.before === "s499",
      ),
    ).toBe(true);
    expect(hook.result.current.settled).toHaveLength(501);
    expect(onToast).not.toHaveBeenCalled();
  });

  it("says so when the registry is bigger than it will load", async () => {
    let call = 0;
    const daemon = new FakeDaemon()
      .onRequest("list_decisions", (body) => {
        if (body.type !== "list_decisions" || body.state !== "settled") {
          return { type: "decisions", req_id: "1", decisions: [] };
        }
        call += 1;
        // Always a full page of fresh ids: the ceiling, not the list, is what
        // ends this.
        return {
          type: "decisions",
          req_id: "1",
          decisions: Array.from({ length: 500 }, (_, i) =>
            dfx.decision({ id: `s${call}-${i}`, state: "settled" }),
          ),
        };
      })
      .onRequest("list_tags", () => ({ type: "tags", req_id: "1", tags: [] }));
    const { onToast } = await mounted(daemon);
    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith(
        "warn",
        "The registry is larger than this view loads",
        expect.stringContaining("10000 records per list"),
      );
    });
  });

  // An inbox that silently reads as empty is worse than one that says it could
  // not load: the owner would conclude nothing is waiting.
  it("says so rather than showing an empty inbox", async () => {
    const daemon = new FakeDaemon()
      .onRequest("list_decisions", () => {
        throw new Error("daemon said no");
      })
      .onRequest("list_tags", () => ({ type: "tags", req_id: "1", tags: [] }));
    const { hook, onToast } = mount(daemon);
    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith(
        "error",
        "Failed to load decisions",
        expect.stringContaining("daemon said no"),
      );
    });
    expect(hook.result.current.loaded).toBe(false);
  });
});

describe("what the daemon pushes", () => {
  it("moves a record off the waiting list once it settles elsewhere", async () => {
    const daemon = seed();
    const { hook } = await mounted(daemon);
    act(() => {
      daemon.emit("decision_update", {
        type: "decision_update",
        decision: dfx.decision({
          id: "d1",
          state: "settled",
          published_at: "2026-09-12T00:00:00Z",
        }),
      });
    });
    expect(hook.result.current.waiting.map((item) => item.id)).toEqual(["d2"]);
    expect(hook.result.current.registry.map((item) => item.id)).toEqual(["d1", "d4", "d5"]);
  });

  it("forgets a record another client deleted", async () => {
    const daemon = seed();
    const { hook } = await mounted(daemon);
    act(() => {
      daemon.emit("decision_deleted", { type: "decision_deleted", decision_id: "d1" });
    });
    expect(hook.result.current.byId.has("d1")).toBe(false);
  });

  // The same comment can arrive twice on a reconnect; the thread must not
  // double it, and the count must not drift.
  it("appends a comment once however often it arrives", async () => {
    const daemon = seed();
    const { hook } = await mounted(daemon);
    const comment = dfx.decisionComment({ id: "c1", decision_id: "d1" });
    act(() => {
      daemon.emit("decision_comment_new", { type: "decision_comment_new", comment });
      daemon.emit("decision_comment_new", { type: "decision_comment_new", comment });
    });
    const record = hook.result.current.byId.get("d1");
    expect(record?.comments).toHaveLength(1);
    expect(record?.comment_count).toBe(1);
  });

  // An empty thread is omitted from the wire, so the first comment on a record
  // that has none arrives with nothing to append to. It must still show,
  // rather than waiting for the record to be fetched again.
  it("starts a thread on a record whose empty one was never sent", async () => {
    const daemon = seed();
    const { hook } = await mounted(daemon);
    const comment = dfx.decisionComment({ id: "c1", decision_id: "d3" });
    act(() => {
      daemon.emit("decision_comment_new", { type: "decision_comment_new", comment });
    });
    const record = hook.result.current.byId.get("d3");
    expect(record?.comments).toHaveLength(1);
    expect(record?.comment_count).toBe(1);
  });

  // A record with a thread that was never fetched must not pretend the one
  // comment it just saw is the whole of it.
  it("leaves an unfetched thread alone and only moves the count", async () => {
    const daemon = seed();
    const { hook } = await mounted(daemon);
    act(() => {
      hook.result.current.replace(dfx.decision({ id: "d3", state: "held", comment_count: 2 }));
    });
    act(() => {
      daemon.emit("decision_comment_new", {
        type: "decision_comment_new",
        comment: dfx.decisionComment({ id: "c3", decision_id: "d3" }),
      });
    });
    const record = hook.result.current.byId.get("d3");
    expect(record?.comments).toBeUndefined();
    expect(record?.comment_count).toBe(3);
  });

  // List replies and most pushes carry no thread; a summary arriving late must
  // not flatten a record the reading pane already fetched in full.
  it("keeps a thread a summary push does not carry", async () => {
    const daemon = seed();
    const { hook } = await mounted(daemon);
    act(() => {
      hook.result.current.replace(
        dfx.decision({ id: "d1", comments: [dfx.decisionComment({ id: "c1" })] }),
      );
    });
    act(() => {
      daemon.emit("decision_update", {
        type: "decision_update",
        decision: dfx.decision({ id: "d1", title: "Waive rule 3?" }),
      });
    });
    const record = hook.result.current.byId.get("d1");
    expect(record?.title).toBe("Waive rule 3?");
    expect(record?.comments).toHaveLength(1);
  });
});

describe("mutations", () => {
  it("takes the daemon's record rather than assuming its own write landed", async () => {
    const daemon = seed().onRequest("answer_decision", () => ({
      type: "decision",
      req_id: "1",
      decision: dfx.decision({ id: "d1", state: "answered", title: "Waive rule 3?" }),
    }));
    const { hook } = await mounted(daemon);
    await act(async () => {
      await hook.result.current.answer("d1", "Start today.");
    });
    expect(hook.result.current.byId.get("d1")).toMatchObject({
      state: "answered",
      title: "Waive rule 3?",
    });
  });

  // Fewer bots told than the owner ticked has to be visible, not silent.
  it("says which bots a publish could not reach", async () => {
    const daemon = seed().onRequest("publish_decisions", () => ({
      type: "publish_result",
      req_id: "1",
      results: [
        {
          decision_id: "d2",
          notified: ["auction"],
          skipped: [{ bot: "storefront", reason: "the bot was deleted" }],
        },
      ],
    }));
    const { hook, onToast } = await mounted(daemon);
    await act(async () => {
      await hook.result.current.publish([{ decision_id: "d2" }]);
    });
    expect(onToast).toHaveBeenCalledWith(
      "warn",
      "Some bots were not told",
      "storefront: the bot was deleted",
    );
  });
});
