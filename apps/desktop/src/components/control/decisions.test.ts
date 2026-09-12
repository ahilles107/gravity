import { describe, expect, it } from "vitest";
import * as fx from "../../test/decisionFixtures";
import {
  NO_FILTER,
  age,
  dayToIso,
  deadline,
  filedAt,
  groupByMonth,
  matchesRegistry,
  registryFacets,
  sortWaiting,
} from "./decisions";
import type { RegistryFilter } from "./decisions";

const filter = (query: string, over: Partial<RegistryFilter> = {}): RegistryFilter => ({
  ...NO_FILTER,
  query,
  ...over,
});

// Local rather than UTC: the labels talk about "today" and "tomorrow", which
// are local facts, so a fixed instant has to be pinned the same way.
const NOW = new Date(2026, 8, 11, 9, 0, 0).getTime();
const HOUR = 3_600_000;
const hours = (n: number): string => new Date(NOW + n * HOUR).toISOString();

describe("the waiting order", () => {
  it("puts urgent first, then what runs out soonest, then what has waited longest", () => {
    const urgent = fx.decision({ id: "urgent", priority: "urgent" });
    const soon = fx.decision({ id: "soon", deadline_at: hours(6) });
    const later = fx.decision({ id: "later", deadline_at: hours(72) });
    const old = fx.decision({ id: "old", created_at: "2020-01-01T00:00:00Z" });
    const fresh = fx.decision({ id: "fresh", created_at: "2026-09-10T00:00:00Z" });

    expect(sortWaiting([fresh, old, later, soon, urgent]).map((item) => item.id)).toEqual([
      "urgent",
      "soon",
      "later",
      "old",
      "fresh",
    ]);
  });

  it("does not mutate what it was given", () => {
    const input = [fx.decision({ id: "b" }), fx.decision({ id: "a", deadline_at: hours(1) })];
    sortWaiting(input);
    expect(input.map((item) => item.id)).toEqual(["b", "a"]);
  });
});

describe("age", () => {
  it("reads at the resolution the owner cares about", () => {
    expect(age(hours(0), NOW)).toBe("just now");
    expect(age(hours(-3), NOW)).toBe("3h ago");
    expect(age(hours(-25), NOW)).toBe("1 day ago");
    expect(age(hours(-49), NOW)).toBe("2 days ago");
  });
});

describe("deadlines", () => {
  it("reads a passed deadline as closed, with nothing left to say", () => {
    expect(deadline(hours(-1), NOW)).toEqual({ short: "closed", tone: "red" });
  });

  it("counts the hours down once the choice closes today", () => {
    expect(deadline(hours(6), NOW)).toEqual({
      short: "6h left",
      tone: "amber",
      long: "Choice closes in 6 hours, 15:00 today",
    });
  });

  it("says tomorrow rather than a count once it crosses midnight", () => {
    const label = deadline(hours(30), NOW);
    expect(label?.short).toBe("tomorrow");
    expect(label?.tone).toBe("amber");
    expect(label?.long).toBe("Choice closes tomorrow, 12 Sep 15:00");
  });

  // Further out is a date, not a countdown: the fact is that the option
  // expires, not that the owner is late.
  it("falls back to a date, dimmed, further out", () => {
    expect(deadline(hours(48), NOW)).toEqual({
      short: "until 13 Sep",
      tone: "dim",
      long: "Choice stops being available after 13 Sep",
    });
  });

  it("has nothing to say about a missing or unparseable deadline", () => {
    expect(deadline(undefined, NOW)).toBeUndefined();
    expect(deadline("soonish", NOW)).toBeUndefined();
  });
});

describe("the registry filter", () => {
  const settled = fx.decision({
    ruling: {
      text: "Start today.",
      answered_at: "2026-09-10T00:00:00Z",
      answered_by: "owner",
    },
  });

  it("matches who asked as well as the words", () => {
    expect(matchesRegistry(settled, filter("auction"))).toBe(true);
    expect(matchesRegistry(settled, filter("start today"))).toBe(true);
    expect(matchesRegistry(settled, filter("backblaze"))).toBe(false);
  });

  // The body is what the owner actually remembers, and the daemon indexes it
  // too — leaving it out meant a phrase from a decision you had read found
  // nothing.
  it("matches the question itself, not only its title", () => {
    expect(matchesRegistry(settled, filter("17+ listing"))).toBe(true);
    expect(matchesRegistry(settled, filter("$40/day"))).toBe(true);
  });

  it("matches why a decision was called off", () => {
    const withdrawn = fx.decision({
      state: "withdrawn",
      withdrawn_reason: "the listing went live on its own",
    });
    expect(matchesRegistry(withdrawn, filter("went live"))).toBe(true);
  });

  it("matches a tag by name", () => {
    expect(matchesRegistry(settled, filter("apple-ads"))).toBe(true);
  });

  // The chip is a filter, not another search term: a row that misses the tag
  // stays out however well the query matched.
  it("respects the tag filter whatever the query says", () => {
    expect(matchesRegistry(settled, filter("", { tagFilter: "spend" }))).toBe(true);
    expect(matchesRegistry(settled, filter("", { tagFilter: "backups" }))).toBe(false);
    expect(matchesRegistry(settled, filter("auction", { tagFilter: "backups" }))).toBe(false);
  });

  it("respects the project and bot chips", () => {
    expect(matchesRegistry(settled, filter("", { projectFilter: "p1" }))).toBe(true);
    expect(matchesRegistry(settled, filter("", { projectFilter: "p2" }))).toBe(false);
    expect(matchesRegistry(settled, filter("", { botFilter: "b1" }))).toBe(true);
    expect(matchesRegistry(settled, filter("auction", { botFilter: "b2" }))).toBe(false);
  });
});

const projectName = (projectId: string): string => (projectId === "p1" ? "Acme" : "Zephyr");

describe("the registry facets", () => {
  const rows = [
    fx.decision({ id: "a", project_id: "p1" }),
    fx.decision({
      id: "b",
      project_id: "p1",
      raised_by: { bot_id: "b2", name: "chief", avatar: "" },
    }),
    fx.decision({
      id: "c",
      project_id: "p2",
      raised_by: { bot_id: "b3", name: "shepherd", avatar: "" },
    }),
  ];

  it("counts the projects and the bots that raised something", () => {
    const facets = registryFacets(rows, NO_FILTER, projectName);
    expect(facets.projects).toEqual([
      { id: "p1", label: "Acme", count: 2 },
      { id: "p2", label: "Zephyr", count: 1 },
    ]);
    expect(facets.bots.map((bot) => bot.label)).toEqual(["auction", "chief", "shepherd"]);
  });

  // Chips for bots with nothing left to show are worse than no chips at all.
  it("narrows the bots to the chosen project, and keeps every project offered", () => {
    const facets = registryFacets(rows, { ...NO_FILTER, projectFilter: "p2" }, projectName);
    expect(facets.bots).toEqual([{ id: "b3", label: "shepherd", count: 1 }]);
    expect(facets.projects).toHaveLength(2);
  });

  it("counts only what the search left", () => {
    const facets = registryFacets(rows, { ...NO_FILTER, query: "chief" }, projectName);
    expect(facets.projects).toEqual([{ id: "p1", label: "Acme", count: 1 }]);
  });
});

describe("month grouping", () => {
  it("groups consecutive rows and names the month in full", () => {
    const rows = [
      fx.decision({ id: "a", published_at: "2026-09-20T10:00:00Z" }),
      fx.decision({ id: "b", published_at: "2026-09-02T10:00:00Z" }),
      fx.decision({ id: "c", published_at: "2026-08-28T10:00:00Z" }),
    ];
    expect(
      groupByMonth(rows).map((group) => [group.label, group.decisions.map((item) => item.id)]),
    ).toEqual([
      ["September 2026", ["a", "b"]],
      ["August 2026", ["c"]],
    ]);
  });
});

describe("filing", () => {
  it("files a row under when it was ruled, not when it was raised", () => {
    const decision = fx.decision({
      created_at: "2026-08-01T00:00:00Z",
      edited_at: "2026-08-15T00:00:00Z",
      published_at: "2026-09-01T00:00:00Z",
    });
    expect(filedAt(decision)).toBe("2026-09-01T00:00:00Z");
    expect(filedAt(fx.decision({ created_at: "2026-08-01T00:00:00Z" }))).toBe(
      "2026-08-01T00:00:00Z",
    );
  });
});

describe("date inputs", () => {
  it("reads a date field as the instant that day begins locally", () => {
    const iso = dayToIso("2026-09-20");
    expect(iso).toBeDefined();
    expect(new Date(String(iso)).getTime()).toBe(new Date(2026, 8, 20).getTime());
  });

  it("rejects anything that is not a date field's value", () => {
    expect(dayToIso("")).toBeUndefined();
    expect(dayToIso("tomorrow")).toBeUndefined();
    expect(dayToIso("2026-9-20")).toBeUndefined();
  });
});
