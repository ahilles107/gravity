import { afterEach, describe, expect, it, vi } from "vitest";
import { readClientToken } from "./token";

const invoke = vi.hoisted(() => vi.fn<(command: string) => Promise<unknown>>());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

afterEach(() => {
  vi.unstubAllGlobals();
  invoke.mockReset();
});

/** Marks the window as the Tauri shell, the way the runtime does. */
function inTauri(): void {
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
}

describe("readClientToken", () => {
  it("prefers a stored device token over the local token file", async () => {
    inTauri();
    vi.stubGlobal("localStorage", {
      getItem: (key: string): string | null =>
        key === "gravity.device-token" ? "remote-device-token" : null,
    });
    await expect(readClientToken()).resolves.toBe("remote-device-token");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("returns an empty token outside the Tauri shell", async () => {
    vi.stubGlobal("window", {});
    await expect(readClientToken()).resolves.toBe("");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("reads the token through the Tauri command", async () => {
    inTauri();
    invoke.mockResolvedValue("secret");
    await expect(readClientToken()).resolves.toBe("secret");
    expect(invoke).toHaveBeenCalledWith("read_client_token");
  });

  it("falls back to an empty token for a non-string result", async () => {
    inTauri();
    invoke.mockResolvedValue(42);
    await expect(readClientToken()).resolves.toBe("");
  });

  it("falls back to an empty token when the command fails", async () => {
    inTauri();
    invoke.mockRejectedValue(new Error("no such file"));
    await expect(readClientToken()).resolves.toBe("");
  });
});
