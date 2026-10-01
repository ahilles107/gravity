import { afterEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn<(command: string) => Promise<unknown>>();
let deliver: ((event: { payload: unknown }) => void) | null = null;
const unlisten = vi.fn<() => void>();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (command: string) => invoke(command) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_name: string, handler: (event: { payload: unknown }) => void) => {
    deliver = handler;
    return unlisten;
  },
}));

const { nativeDictation } = await import("./dictation");

describe("nativeDictation", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    invoke.mockReset();
    deliver = null;
  });

  it("is unavailable outside the app shell", async () => {
    expect(await nativeDictation.available()).toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("asks the shell, and passes only well-formed events on", async () => {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    invoke.mockResolvedValue(true);
    expect(await nativeDictation.available()).toBe(true);

    const seen: unknown[] = [];
    const stop = await nativeDictation.start((event) => seen.push(event));
    expect(invoke).toHaveBeenCalledWith("start_dictation");
    deliver?.({ payload: { kind: "partial", text: "hi" } });
    deliver?.({ payload: { kind: "bogus" } });
    deliver?.({ payload: "nonsense" });
    expect(seen).toEqual([{ kind: "partial", text: "hi" }]);
    stop();
    expect(invoke).toHaveBeenCalledWith("stop_dictation");
  });

  it("stops listening when the shell refuses to start", async () => {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    invoke.mockRejectedValue("Speech recognition is off");
    await expect(nativeDictation.start(() => {})).rejects.toThrow("Speech recognition is off");
    expect(unlisten).toHaveBeenCalled();
  });
});
