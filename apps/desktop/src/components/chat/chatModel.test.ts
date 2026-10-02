import { describe, expect, it } from "vitest";
import { stats, step, text, turn } from "../../test/chatFixtures";
import {
  durationLabel,
  groupItems,
  groupStartsOpen,
  groupSummary,
  mergeTurns,
  statsLine,
  triggerView,
} from "./chatModel";

describe("mergeTurns", () => {
  it("replaces turns by id and keeps start order", () => {
    const first = turn({ id: "a", started_at: "2026-10-01T10:00:00Z" });
    const second = turn({ id: "b", started_at: "2026-10-01T11:00:00Z" });
    const updated = turn({ id: "a", started_at: "2026-10-01T10:00:00Z", open: true });
    const merged = mergeTurns([second], [updated, first]);
    expect(merged.map((t) => t.id)).toEqual(["a", "b"]);
    expect(merged[0]?.open).toBe(false);
    expect(mergeTurns(merged, [updated])[0]?.open).toBe(true);
  });

  it("returns the same list when nothing arrives", () => {
    const list = [turn()];
    expect(mergeTurns(list, [])).toBe(list);
  });
});

describe("groupItems", () => {
  it("folds runs of steps between other items", () => {
    const blocks = groupItems([
      text("before", "t0"),
      step({ id: "s1" }),
      step({ id: "s2" }),
      text("after", "t1"),
      step({ id: "s3" }),
    ]);
    expect(blocks.map((b) => (b.kind === "steps" ? b.steps.length : b.item.type))).toEqual([
      "text",
      2,
      "text",
      1,
    ]);
  });

  it("opens short groups and running turns", () => {
    const many = [1, 2, 3, 4, 5].map((n) => step({ id: `s${n}` }));
    expect(groupStartsOpen(many.slice(0, 4), false)).toBe(true);
    expect(groupStartsOpen(many, false)).toBe(false);
    expect(groupStartsOpen(many, true)).toBe(true);
  });
});

describe("labels", () => {
  it("names who woke the bot", () => {
    expect(triggerView({ kind: "owner", text: "hi", via: "chat" })).toEqual({
      label: "You",
      text: "hi",
      own: true,
    });
    expect(triggerView({ kind: "owner", text: "/compact", via: "terminal" }).label).toBe(
      "You, in the terminal",
    );
    expect(
      triggerView({ kind: "bus", from: "lead", msg_kind: "task", num: 3, text: "port it" }).label,
    ).toBe("Task from lead");
    expect(triggerView({ kind: "routine", name: "nightly", text: "/report" }).label).toBe(
      "Routine nightly",
    );
    expect(triggerView({ kind: "ruling", decision_id: "d", text: "yes" }).label).toBe(
      "Your ruling",
    );
    expect(triggerView({ kind: "resumed" }).text).toBe("");
    expect(triggerView({ kind: "background", text: "done" }).own).toBe(false);
  });

  it("summarises a turn and a step group", () => {
    expect(statsLine(stats())).toBe("");
    expect(
      statsLine(
        stats({
          commands: 3,
          edits: 2,
          added: 120,
          removed: 4,
          sent: 1,
          reads: 1,
          images: 2,
          errors: 1,
        }),
      ),
    ).toBe("3 commands · 2 edits +120 −4 · 1 read · 1 message sent · 2 images · 1 error");
    expect(
      groupSummary([step({ id: "a", added: 10, removed: 2 }), step({ id: "b", status: "error" })]),
    ).toBe("2 steps · 1 edit +10 −2 · 1 error");
  });

  it("formats durations", () => {
    expect(durationLabel(undefined)).toBe("");
    expect(durationLabel(12_400)).toBe("took 12s");
    expect(durationLabel(180_000)).toBe("took 3m");
    expect(durationLabel(184_000)).toBe("took 3m 4s");
  });
});
