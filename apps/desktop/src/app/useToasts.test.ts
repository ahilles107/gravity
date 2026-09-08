import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useToasts } from "./useToasts";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useToasts", () => {
  it("drops the oldest transient toast once the cap is reached", () => {
    const { result } = renderHook(() => useToasts());
    act(() => {
      for (let i = 0; i < 7; i += 1) {
        result.current.addToast("info", `t${i}`, "");
      }
    });
    expect(result.current.toasts.map((toast) => toast.title)).toEqual([
      "t2",
      "t3",
      "t4",
      "t5",
      "t6",
    ]);
  });

  it("keeps a sticky toast through a burst that would otherwise evict it", () => {
    const { result } = renderHook(() => useToasts());
    act(() => {
      result.current.addToast("info", "Update available", "", { sticky: true });
      for (let i = 0; i < 8; i += 1) {
        result.current.addToast("warn", `t${i}`, "");
      }
    });
    const titles = result.current.toasts.map((toast) => toast.title);
    expect(titles).toContain("Update available");
    expect(titles).toHaveLength(5);
  });

  it("expires transient toasts but never a sticky one", () => {
    const { result } = renderHook(() => useToasts());
    act(() => {
      result.current.addToast("info", "Update available", "", { sticky: true });
      result.current.addToast("info", "Bot started", "");
    });
    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(result.current.toasts.map((toast) => toast.title)).toEqual(["Update available"]);
  });

  it("dismisses a sticky toast by hand", () => {
    const { result } = renderHook(() => useToasts());
    act(() => {
      result.current.addToast("info", "Update available", "", { sticky: true });
    });
    const id = result.current.toasts[0]?.id ?? -1;
    act(() => {
      result.current.dismissToast(id);
    });
    expect(result.current.toasts).toHaveLength(0);
  });
});
