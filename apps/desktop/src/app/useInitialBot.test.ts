import { describe, expect, it } from "vitest";
import * as fx from "../test/fixtures";
import { firstSidebarBot, lastUsedSidebarBot } from "./useInitialBot";

describe("firstSidebarBot", () => {
  it("picks the first bot of the first project that has one", () => {
    const projects = [fx.project({ id: "p0", name: "Empty" }), fx.project()];
    const bots = [fx.bot(), fx.bot({ id: "b2", name: "bob" })];

    expect(firstSidebarBot(projects, bots, [])?.id).toBe("b1");
  });

  it("prefers the pinned strip, which the sidebar renders above the rows", () => {
    const bots = [fx.bot(), fx.bot({ id: "b2", name: "bob" })];

    expect(firstSidebarBot([fx.project()], bots, ["b2"])?.id).toBe("b2");
  });

  it("has nothing to open with no bots", () => {
    expect(firstSidebarBot([fx.project()], [], [])).toBeUndefined();
  });
});

describe("lastUsedSidebarBot", () => {
  const projects = [fx.project()];
  const bots = [fx.bot(), fx.bot({ id: "b2", name: "bob" })];

  it("finds the stored bot", () => {
    expect(lastUsedSidebarBot(projects, bots, "b2")?.id).toBe("b2");
  });

  it("has nothing to open on a first launch", () => {
    expect(lastUsedSidebarBot(projects, bots, undefined)).toBeUndefined();
  });

  it("has nothing to open once the stored bot is gone", () => {
    expect(lastUsedSidebarBot(projects, bots, "deleted-bot")).toBeUndefined();
  });

  it("skips a bot the sidebar has no project to render it under", () => {
    const orphan = fx.bot({ id: "b3", name: "carol", project_id: "gone" });

    expect(lastUsedSidebarBot(projects, [...bots, orphan], "b3")).toBeUndefined();
  });
});
