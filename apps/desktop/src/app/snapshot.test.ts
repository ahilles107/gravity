import { describe, expect, it } from "vitest";
import { FakeDaemon } from "../test/fakeDaemon";
import * as fx from "../test/fixtures";
import { earliestRun, fetchSnapshot } from "./snapshot";

function daemonWithBots(bots: readonly ReturnType<typeof fx.bot>[]): FakeDaemon {
  return new FakeDaemon()
    .onRequest("list_projects", () => ({ type: "projects", req_id: "1", projects: [fx.project()] }))
    .onRequest("list_bots", () => ({ type: "bots", req_id: "1", bots }))
    .onRequest("list_conversations", () => ({
      type: "conversations",
      req_id: "1",
      conversations: [fx.conversation()],
    }))
    .onRequest("list_deliveries", () => ({ type: "deliveries", req_id: "1", deliveries: [] }));
}

describe("fetchSnapshot", () => {
  it("collects every entity list plus routines per bot", async () => {
    const bots = [fx.bot({ id: "b1" }), fx.bot({ id: "b2", name: "bob" })];
    const daemon = daemonWithBots(bots).onRequest("list_routines", (body) => ({
      type: "routines",
      req_id: "1",
      routines: body.type === "list_routines" && body.bot_id === "b1" ? [fx.routine()] : [],
    }));

    const snapshot = await fetchSnapshot(daemon);

    expect(snapshot.projects).toHaveLength(1);
    expect(snapshot.bots).toHaveLength(2);
    expect(snapshot.conversations).toHaveLength(1);
    expect(snapshot.failedDeliveries).toEqual([]);
    expect(snapshot.routinesByBot).toEqual([
      ["b1", [fx.routine()]],
      ["b2", []],
    ]);
    expect(snapshot.activity).toEqual([]);
  });

  it("collects the preview line for every bot", async () => {
    const activity = fx.botActivity();
    const daemon = daemonWithBots([fx.bot()])
      .onRequest("list_routines", () => ({ type: "routines", req_id: "1", routines: [] }))
      .onRequest("list_bot_activity", () => ({
        type: "bot_activity",
        req_id: "1",
        activity: [activity],
      }));

    const snapshot = await fetchSnapshot(daemon);

    expect(snapshot.activity).toEqual([activity]);
  });

  it("degrades to no previews when the activity lookup fails", async () => {
    const daemon = daemonWithBots([fx.bot()])
      .onRequest("list_routines", () => ({ type: "routines", req_id: "1", routines: [] }))
      .onRequest("list_bot_activity", () => {
        throw new Error("older daemon");
      });

    const snapshot = await fetchSnapshot(daemon);

    expect(snapshot.activity).toEqual([]);
  });

  it("treats a failing routine lookup as an empty list", async () => {
    const daemon = daemonWithBots([fx.bot()]).onRequest("list_routines", () => {
      throw new Error("bot is offline");
    });

    const snapshot = await fetchSnapshot(daemon);

    expect(snapshot.routinesByBot).toEqual([["b1", []]]);
  });

  it("propagates a failure of the top-level lists", async () => {
    const daemon = daemonWithBots([]).onRequest("list_bots", () => {
      throw new Error("nope");
    });

    await expect(fetchSnapshot(daemon)).rejects.toThrow("nope");
  });
});

describe("earliestRun", () => {
  it("returns null when nothing is scheduled", () => {
    expect(earliestRun([])).toBeNull();
    expect(earliestRun([fx.routine({ enabled: false })])).toBeNull();
    expect(earliestRun([fx.routine({ next_run_at: null })])).toBeNull();
    expect(earliestRun([fx.routine({ next_run_at: "" })])).toBeNull();
  });

  it("picks the earliest enabled run", () => {
    const runs = [
      fx.routine({ id: "a", next_run_at: "2024-05-02T00:00:00.000Z" }),
      fx.routine({ id: "b", next_run_at: "2024-05-01T00:00:00.000Z" }),
      fx.routine({ id: "c", enabled: false, next_run_at: "2020-01-01T00:00:00.000Z" }),
    ];
    expect(earliestRun(runs)).toBe("2024-05-01T00:00:00.000Z");
  });
});
