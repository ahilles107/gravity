import { afterEach, describe, expect, it, vi } from "vitest";
import { setDockBadge } from "./badge";

const invoke = vi.hoisted(() => vi.fn<(command: string, args?: unknown) => Promise<unknown>>());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

afterEach(() => {
  vi.unstubAllGlobals();
  invoke.mockReset();
});

/** Marks the window as the Tauri shell, the way the runtime does. */
function inTauri(): void {
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
}

describe("setDockBadge", () => {
  it("does nothing outside the Tauri shell", async () => {
    vi.stubGlobal("window", {});
    await setDockBadge(3);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("sends the count to the shell", async () => {
    inTauri();
    invoke.mockResolvedValue(null);
    await setDockBadge(3);
    expect(invoke).toHaveBeenCalledWith("set_dock_badge", { count: 3 });
  });

  it("clamps a nonsensical count to something the shell accepts", async () => {
    inTauri();
    invoke.mockResolvedValue(null);
    await setDockBadge(-2.5);
    expect(invoke).toHaveBeenCalledWith("set_dock_badge", { count: 0 });
  });

  it("swallows a failure from the shell", async () => {
    inTauri();
    invoke.mockRejectedValue(new Error("no main window"));
    await expect(setDockBadge(1)).resolves.toBeUndefined();
  });
});
