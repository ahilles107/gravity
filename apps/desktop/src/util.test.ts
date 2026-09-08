import { describe, expect, it } from "vitest";
import { errText, fmtShortTime, fmtTimestamp, fuzzyScore } from "./util";

describe("fuzzyScore", () => {
  it("scores an empty query as neutral", () => {
    expect(fuzzyScore("   ", "anything")).toBe(0);
  });

  it("returns null when the query is not a subsequence", () => {
    expect(fuzzyScore("xyz", "alpha")).toBeNull();
  });

  it("matches case-insensitively", () => {
    expect(fuzzyScore("AL", "alpha")).not.toBeNull();
  });

  it("rewards consecutive and prefix hits", () => {
    const prefix = fuzzyScore("al", "alpha");
    const scattered = fuzzyScore("ah", "alpha");
    expect(prefix).not.toBeNull();
    expect(scattered).not.toBeNull();
    expect(prefix ?? 0).toBeGreaterThan(scattered ?? 0);
  });
});

describe("errText", () => {
  it("uses the message of an Error", () => {
    expect(errText(new Error("boom"))).toBe("boom");
  });

  it("stringifies anything else", () => {
    expect(errText("plain")).toBe("plain");
    expect(errText(42)).toBe("42");
  });
});

describe("timestamp formatting", () => {
  it("passes through unparseable input", () => {
    expect(fmtTimestamp("not-a-date")).toBe("not-a-date");
  });

  it("returns an empty short time for unparseable input", () => {
    expect(fmtShortTime("not-a-date")).toBe("");
  });

  it("omits the date for today", () => {
    const now = new Date();
    const today = fmtTimestamp(now.toISOString());
    const other = fmtTimestamp("2001-02-03T04:05:06.000Z");
    expect(today.length).toBeLessThan(other.length);
  });
});
