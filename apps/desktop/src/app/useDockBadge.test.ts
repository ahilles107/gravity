import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { totalUnread, useDockBadge } from "./useDockBadge";

const setDockBadge = vi.hoisted(() => vi.fn<(count: number) => Promise<void>>());
vi.mock("../badge", () => ({ setDockBadge }));

type Unread = Readonly<Record<string, number>>;

/** Renders the hook so the caller can hand it a fresh set of counters. */
function render(initialProps: Unread, enabled = true): { rerender: (next: Unread) => void } {
  return renderHook<void, Unread>(
    (unread) => {
      useDockBadge(unread, enabled);
    },
    { initialProps },
  );
}

describe("totalUnread", () => {
  it("adds up every bot's counter", () => {
    expect(totalUnread({ b1: 2, b2: 3 })).toBe(5);
  });

  it("is zero with nothing unread", () => {
    expect(totalUnread({})).toBe(0);
  });
});

describe("useDockBadge", () => {
  it("pushes the total to the dock", () => {
    render({ b1: 2, b2: 3 });

    expect(setDockBadge).toHaveBeenCalledWith(5);
  });

  it("leaves the dock alone while the total holds, even as bots shuffle", () => {
    const { rerender } = render({ b1: 2 });
    setDockBadge.mockClear();

    rerender({ b2: 2 });

    expect(setDockBadge).not.toHaveBeenCalled();
  });

  it("clears the dock once the last badge goes", () => {
    const { rerender } = render({ b1: 2 });
    setDockBadge.mockClear();

    rerender({});

    expect(setDockBadge).toHaveBeenCalledWith(0);
  });

  it("keeps the dock clear when the badge pref is off", () => {
    render({ b1: 2 }, false);

    expect(setDockBadge).toHaveBeenCalledWith(0);
    expect(setDockBadge).not.toHaveBeenCalledWith(2);
  });
});
