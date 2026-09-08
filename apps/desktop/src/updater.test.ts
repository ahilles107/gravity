import { afterEach, describe, expect, it, vi } from "vitest";
import { checkForAppUpdate, installAppUpdate, relaunchApp } from "./updater";

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

describe("checkForAppUpdate", () => {
  it("returns null outside the Tauri shell", async () => {
    vi.stubGlobal("window", {});
    await expect(checkForAppUpdate()).resolves.toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("returns the new version when an update exists", async () => {
    inTauri();
    invoke.mockResolvedValue({ version: "0.5.0" });
    await expect(checkForAppUpdate()).resolves.toBe("0.5.0");
    expect(invoke).toHaveBeenCalledWith("check_for_update");
  });

  it("returns null when the build is current", async () => {
    inTauri();
    invoke.mockResolvedValue(null);
    await expect(checkForAppUpdate()).resolves.toBeNull();
  });

  it("returns null when the check fails", async () => {
    inTauri();
    invoke.mockRejectedValue("endpoint unreachable");
    await expect(checkForAppUpdate()).resolves.toBeNull();
  });

  it("returns null for a malformed result", async () => {
    inTauri();
    invoke.mockResolvedValue({ version: 5 });
    await expect(checkForAppUpdate()).resolves.toBeNull();
  });
});

describe("installAppUpdate", () => {
  it("invokes the install command", async () => {
    invoke.mockResolvedValue(null);
    await expect(installAppUpdate(true)).resolves.toBeNull();
    expect(invoke).toHaveBeenCalledWith("install_update", { updateLocalDaemon: true });
  });

  it("returns the daemon failure the command reports", async () => {
    invoke.mockResolvedValue("daemon install failed: no such file");
    await expect(installAppUpdate(true)).resolves.toBe("daemon install failed: no such file");
  });

  it("wraps a string error in an Error with that message", async () => {
    invoke.mockRejectedValue("signature mismatch");
    await expect(installAppUpdate(false)).rejects.toThrow("signature mismatch");
  });

  it("falls back to a generic message for non-string errors", async () => {
    invoke.mockRejectedValue({ odd: true });
    await expect(installAppUpdate(false)).rejects.toThrow("update install failed");
  });

  it("relaunches through its own command", async () => {
    invoke.mockResolvedValue(null);
    await expect(relaunchApp()).resolves.toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("relaunch_app");
  });
});
