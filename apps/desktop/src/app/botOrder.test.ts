import { describe, expect, it } from "vitest";
import * as fx from "../test/fixtures";
import { sortBots, withBotOrder } from "./botOrder";

const names = (bots: readonly { readonly name: string }[]): string[] => bots.map((bot) => bot.name);

describe("sortBots", () => {
  it("orders by sort_order, then by name", () => {
    const bots = [
      fx.bot({ id: "b1", name: "zed", sort_order: 0 }),
      fx.bot({ id: "b2", name: "bob", sort_order: 1 }),
      fx.bot({ id: "b3", name: "amy", sort_order: 1 }),
    ];
    expect(names(sortBots(bots))).toEqual(["zed", "amy", "bob"]);
  });

  // A daemon that predates ordering sends no sort_order at all.
  it("falls back to name order without sort_order", () => {
    const bots = [fx.bot({ id: "b1", name: "cat" }), fx.bot({ id: "b2", name: "ann" })];
    expect(names(sortBots(bots))).toEqual(["ann", "cat"]);
  });

  it("leaves its input alone", () => {
    const bots = [fx.bot({ id: "b1", name: "cat" }), fx.bot({ id: "b2", name: "ann" })];
    sortBots(bots);
    expect(names(bots)).toEqual(["cat", "ann"]);
  });
});

describe("withBotOrder", () => {
  const ann = fx.bot({ id: "a", name: "ann", sort_order: 0 });
  const bob = fx.bot({ id: "b", name: "bob", sort_order: 1 });
  const cat = fx.bot({ id: "c", name: "cat", sort_order: 2 });
  const elsewhere = fx.bot({ id: "x", project_id: "p2", name: "xan", sort_order: 0 });

  it("puts the listed bots first and keeps the rest in their order", () => {
    const next = withBotOrder([ann, bob, cat, elsewhere], "p1", ["c"]);
    expect(names(next.filter((bot) => bot.project_id === "p1"))).toEqual(["cat", "ann", "bob"]);
    expect(next.map((bot) => [bot.id, bot.sort_order])).toContainEqual(["c", 0]);
  });

  it("leaves other projects untouched", () => {
    const next = withBotOrder([ann, bob, elsewhere], "p1", ["b", "a"]);
    expect(next.find((bot) => bot.id === "x")).toBe(elsewhere);
  });
});
