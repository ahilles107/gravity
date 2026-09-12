import { describe, expect, it } from "vitest";
import * as dfx from "../../test/decisionFixtures";
import { bot } from "../../test/fixtures";
import { buildPublishItems, defaultNotify, notifyCandidates } from "./publishPlan";

const BOTS = [
  bot({ id: "b1", name: "auction" }),
  bot({ id: "b2", name: "chief" }),
  bot({ id: "b3", name: "storefront" }),
  bot({ id: "b4", name: "shepherd" }),
  bot({ id: "b5", name: "ledger", project_id: "p2" }),
  bot({ id: "b6", name: "ghost", deleted_at: "2026-09-01T00:00:00Z" }),
] as const;

const ruled = (id: string, text: string) =>
  dfx.decision({
    id,
    state: "answered",
    ruling: { text, answered_at: "2026-09-11T00:00:00Z", answered_by: "owner" },
  });

describe("who gets told", () => {
  it("locks the asker on, checks whoever is blocked, and offers the rest of the project", () => {
    const candidates = notifyCandidates(dfx.decision({ on_behalf_of_bot_id: "b3" }), BOTS, "b2");
    expect(candidates).toEqual([
      { botId: "b1", name: "auction", checked: true, locked: true, title: expect.any(String) },
      {
        botId: "b3",
        name: "storefront",
        checked: true,
        locked: false,
        title: "Waiting on this answer",
      },
      { botId: "b2", name: "chief", checked: true, locked: false, title: "Project lead" },
      { botId: "b4", name: "shepherd", checked: false, locked: false, title: "" },
    ]);
  });

  it("leaves out a bot from another project", () => {
    const candidates = notifyCandidates(dfx.decision(), BOTS, undefined);
    expect(candidates.map((item) => item.botId)).not.toContain("b5");
  });

  // Telling an archived bot means queueing a message nothing will ever read.
  it("leaves out an archived bot", () => {
    const candidates = notifyCandidates(dfx.decision(), BOTS, undefined);
    expect(candidates.map((item) => item.botId)).not.toContain("b6");
  });

  it("offers the asker once when it is also the lead", () => {
    const candidates = notifyCandidates(dfx.decision(), BOTS, "b1");
    expect(candidates.filter((item) => item.botId === "b1")).toHaveLength(1);
    expect(candidates[0]?.locked).toBe(true);
  });

  it("starts the set from whatever the candidates checked", () => {
    const candidates = notifyCandidates(dfx.decision({ on_behalf_of_bot_id: "b3" }), BOTS, "b2");
    expect([...defaultNotify(candidates)]).toEqual(["b1", "b3", "b2"]);
  });
});

describe("the batch payload", () => {
  it("sends each draft with the set it was given", () => {
    const notify = new Map<string, ReadonlySet<string>>([
      ["d2", new Set(["b1", "b2"])],
      ["d3", new Set(["b1"])],
    ]);
    const items = buildPublishItems(
      [ruled("d2", "Start today."), ruled("d3", "Let it fire.")],
      (id) => notify.get(id) ?? new Set<string>(),
    );
    expect(items).toEqual([
      { decision_id: "d2", notify_bot_ids: ["b1", "b2"] },
      { decision_id: "d3", notify_bot_ids: ["b1"] },
    ]);
  });

  // Publishing a blank ruling would tell every bot the owner decided nothing.
  it("leaves behind a draft with no words in it", () => {
    const items = buildPublishItems(
      [ruled("d2", "   "), dfx.decision({ id: "d4" }), ruled("d3", "Let it fire.")],
      () => new Set(["b1"]),
    );
    expect(items.map((item) => item.decision_id)).toEqual(["d3"]);
  });
});
