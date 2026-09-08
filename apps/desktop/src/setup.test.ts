import { afterEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_ENDPOINT } from "./settings";
import { daemonLogTail, installLocalDaemon, localDaemonEndpoint, probeDaemon } from "./setup";

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

describe("probeDaemon", () => {
  it("returns null outside the Tauri shell", async () => {
    vi.stubGlobal("window", {});
    await expect(probeDaemon({ host: "127.0.0.1", port: 7777 })).resolves.toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("returns the daemon health through the Tauri command", async () => {
    inTauri();
    invoke.mockResolvedValue({ status: "ok", version: "0.1.0" });
    await expect(probeDaemon({ host: "mini", port: 7788 })).resolves.toEqual({
      status: "ok",
      version: "0.1.0",
    });
    expect(invoke).toHaveBeenCalledWith("daemon_health", { host: "mini", port: 7788 });
  });

  it("returns null for a malformed result", async () => {
    inTauri();
    invoke.mockResolvedValue({ status: 1 });
    await expect(probeDaemon({ host: "127.0.0.1", port: 7777 })).resolves.toBeNull();
  });

  it("returns null when the command fails", async () => {
    inTauri();
    invoke.mockRejectedValue(new Error("boom"));
    await expect(probeDaemon({ host: "127.0.0.1", port: 7777 })).resolves.toBeNull();
  });
});

describe("installLocalDaemon", () => {
  it("resolves when the install succeeds", async () => {
    inTauri();
    invoke.mockResolvedValue(null);
    await expect(installLocalDaemon()).resolves.toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("install_local_daemon");
  });

  it("passes the observed daemon version to the installer", async () => {
    inTauri();
    invoke.mockResolvedValue(null);
    await expect(installLocalDaemon("0.7.0")).resolves.toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("install_local_daemon", { currentVersion: "0.7.0" });
  });

  it("surfaces the daemon's error string", async () => {
    inTauri();
    invoke.mockRejectedValue("bundled daemon not found");
    await expect(installLocalDaemon()).rejects.toThrow("bundled daemon not found");
  });

  it("falls back to a generic message for non-string errors", async () => {
    inTauri();
    invoke.mockRejectedValue(new Error("boom"));
    await expect(installLocalDaemon()).rejects.toThrow("daemon install failed");
  });
});

describe("localDaemonEndpoint", () => {
  it("falls back to the default outside the Tauri shell", async () => {
    vi.stubGlobal("window", {});
    await expect(localDaemonEndpoint()).resolves.toEqual(DEFAULT_ENDPOINT);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("uses the port the installed daemon reports", async () => {
    inTauri();
    invoke.mockResolvedValue(7777);
    await expect(localDaemonEndpoint()).resolves.toEqual({
      host: DEFAULT_ENDPOINT.host,
      port: 7777,
    });
    expect(invoke).toHaveBeenCalledWith("local_daemon_port");
  });

  it("falls back to the default for a port that cannot be dialled", async () => {
    inTauri();
    invoke.mockResolvedValue(0);
    await expect(localDaemonEndpoint()).resolves.toEqual(DEFAULT_ENDPOINT);
  });

  it("falls back to the default when the command fails", async () => {
    inTauri();
    invoke.mockRejectedValue(new Error("boom"));
    await expect(localDaemonEndpoint()).resolves.toEqual(DEFAULT_ENDPOINT);
  });
});

describe("daemonLogTail", () => {
  it("is empty outside the Tauri shell", async () => {
    vi.stubGlobal("window", {});
    await expect(daemonLogTail()).resolves.toBe("");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("returns the daemon's own log lines", async () => {
    inTauri();
    invoke.mockResolvedValue("Address already in use");
    await expect(daemonLogTail()).resolves.toBe("Address already in use");
    expect(invoke).toHaveBeenCalledWith("daemon_log_tail");
  });

  it("is empty when there is no log to show", async () => {
    inTauri();
    invoke.mockResolvedValue(null);
    await expect(daemonLogTail()).resolves.toBe("");
  });

  it("is empty when the command fails", async () => {
    inTauri();
    invoke.mockRejectedValue(new Error("boom"));
    await expect(daemonLogTail()).resolves.toBe("");
  });
});
