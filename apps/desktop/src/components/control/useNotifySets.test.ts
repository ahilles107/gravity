import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { NotifyCandidate } from "./publishPlan";
import { useNotifySets } from "./useNotifySets";

const CANDIDATES: readonly NotifyCandidate[] = [
  { botId: "b1", name: "auction", checked: true, locked: true, title: "The asking bot" },
  { botId: "b2", name: "chief", checked: true, locked: false, title: "Project lead" },
  { botId: "b4", name: "shepherd", checked: false, locked: false, title: "" },
];

const candidatesFor = (): readonly NotifyCandidate[] => CANDIDATES;

describe("useNotifySets", () => {
  it("starts a draft from whatever the candidates checked", () => {
    const { result } = renderHook(() => useNotifySets(candidatesFor));
    expect([...result.current.get("d1")]).toEqual(["b1", "b2"]);
  });

  it("adds and removes on toggle", () => {
    const { result } = renderHook(() => useNotifySets(candidatesFor));
    act(() => {
      result.current.toggle("d1", "b4");
    });
    expect(result.current.get("d1").has("b4")).toBe(true);
    act(() => {
      result.current.toggle("d1", "b4");
    });
    expect(result.current.get("d1").has("b4")).toBe(false);
    act(() => {
      result.current.toggle("d1", "b2");
    });
    expect(result.current.get("d1").has("b2")).toBe(false);
  });

  // The daemon sends to exactly the ids it is given, so the asker's chip is
  // the only guarantee that whoever asked hears the answer.
  it("ignores a toggle on the locked asker", () => {
    const { result } = renderHook(() => useNotifySets(candidatesFor));
    act(() => {
      result.current.toggle("d1", "b1");
    });
    expect(result.current.get("d1").has("b1")).toBe(true);
  });

  it("keeps each draft's set as the owner moves through the list", () => {
    const { result } = renderHook(() => useNotifySets(candidatesFor));
    act(() => {
      result.current.toggle("d1", "b4");
    });
    act(() => {
      result.current.toggle("d2", "b2");
    });
    expect([...result.current.get("d1")]).toEqual(["b1", "b2", "b4"]);
    expect([...result.current.get("d2")]).toEqual(["b1"]);
  });
});
