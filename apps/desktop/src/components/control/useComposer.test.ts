import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useComposer } from "./useComposer";

function mount(id: string | undefined) {
  return renderHook(({ selectedId }) => useComposer(selectedId), {
    initialProps: { selectedId: id },
  });
}

describe("useComposer", () => {
  it("keeps what was typed while the same decision is being read", () => {
    const { result, rerender } = mount("d1");
    act(() => {
      result.current.setText("Start today.");
      result.current.togglePick("start");
    });
    rerender({ selectedId: "d1" });
    expect(result.current.text).toBe("Start today.");
    expect(result.current.pick).toBe("start");
  });

  it("clears the pick when the same option is chosen twice", () => {
    const { result } = mount("d1");
    act(() => {
      result.current.togglePick("start");
    });
    expect(result.current.pick).toBe("start");
    act(() => {
      result.current.togglePick("start");
    });
    expect(result.current.pick).toBeUndefined();
    act(() => {
      result.current.togglePick("start");
      result.current.togglePick("hold");
    });
    expect(result.current.pick).toBe("hold");
  });

  // A half-written ruling must never be saved against the wrong question.
  it("starts blank on another decision", () => {
    const { result, rerender } = mount("d1");
    act(() => {
      result.current.setText("Start today.");
      result.current.togglePick("start");
    });
    rerender({ selectedId: "d2" });
    expect(result.current.text).toBe("");
    expect(result.current.pick).toBeUndefined();
  });

  it("puts a discarded draft back so it can be edited", () => {
    const { result } = mount("d1");
    act(() => {
      result.current.prefill("Let it fire.", "hold");
    });
    expect(result.current.text).toBe("Let it fire.");
    expect(result.current.pick).toBe("hold");
    act(() => {
      result.current.clear();
    });
    expect(result.current.text).toBe("");
    expect(result.current.pick).toBeUndefined();
  });
});
