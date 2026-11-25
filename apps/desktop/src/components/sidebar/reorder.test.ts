import { describe, expect, it } from "vitest";
import { moveBot, sameOrder, stepBot } from "./reorder";

describe("moveBot", () => {
  it("drops a bot before or after another", () => {
    expect(moveBot(["a", "b", "c"], "a", "c", "after")).toEqual(["b", "c", "a"]);
    expect(moveBot(["a", "b", "c"], "c", "a", "before")).toEqual(["c", "a", "b"]);
    expect(moveBot(["a", "b", "c"], "a", "c", "before")).toEqual(["b", "a", "c"]);
  });

  it("ignores a drop on itself or on an unknown bot", () => {
    const order = ["a", "b"];
    expect(moveBot(order, "a", "a", "after")).toBe(order);
    expect(moveBot(order, "a", "zz", "after")).toBe(order);
    expect(moveBot(order, "zz", "a", "after")).toBe(order);
  });
});

describe("stepBot", () => {
  it("moves one step among the visible rows", () => {
    expect(stepBot(["a", "b", "c"], ["a", "b", "c"], "b", -1)).toEqual(["b", "a", "c"]);
    expect(stepBot(["a", "b", "c"], ["a", "b", "c"], "b", 1)).toEqual(["a", "c", "b"]);
  });

  // `b` is pinned, so it is not a row: stepping `a` down passes `c`.
  it("steps past the next row rather than a pinned bot between them", () => {
    expect(stepBot(["a", "b", "c"], ["a", "c"], "a", 1)).toEqual(["b", "c", "a"]);
  });

  it("has nothing to do at either end, or for a bot that is not a row", () => {
    expect(stepBot(["a", "b"], ["a", "b"], "a", -1)).toBeNull();
    expect(stepBot(["a", "b"], ["a", "b"], "b", 1)).toBeNull();
    expect(stepBot(["a", "b"], ["b"], "a", 1)).toBeNull();
  });
});

describe("sameOrder", () => {
  it("compares element by element", () => {
    expect(sameOrder(["a", "b"], ["a", "b"])).toBe(true);
    expect(sameOrder(["a", "b"], ["b", "a"])).toBe(false);
    expect(sameOrder(["a"], ["a", "b"])).toBe(false);
  });
});
