import { describe, expect, it } from "vitest";
import { isStopped } from "./bots";
import { withoutKey } from "./records";
import * as fx from "../test/fixtures";

describe("withoutKey", () => {
  it("removes the key", () => {
    expect(withoutKey({ a: 1, b: 2 }, "a")).toEqual({ b: 2 });
  });

  it("returns the same object when the key is absent", () => {
    const source = { a: 1 };
    expect(withoutKey(source, "b")).toBe(source);
  });
});

describe("isStopped", () => {
  it("is true only for states with no live session", () => {
    expect(isStopped(fx.bot({ state: "stopped" }))).toBe(true);
    expect(isStopped(fx.bot({ state: "crashed" }))).toBe(true);
    expect(isStopped(fx.bot({ state: "auth_failed" }))).toBe(true);
    expect(isStopped(fx.bot({ state: "ready" }))).toBe(false);
    expect(isStopped(fx.bot({ state: "working" }))).toBe(false);
  });
});
