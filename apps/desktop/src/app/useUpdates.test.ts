import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ConnectionStatus, Endpoint } from "../protocol/connection";
import type { AddToast } from "./useToasts";
import { useUpdates } from "./useUpdates";

const invoke = vi.hoisted(() => vi.fn<(command: string, args?: unknown) => Promise<unknown>>());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const getVersion = vi.hoisted(() => vi.fn<() => Promise<string>>());
vi.mock("@tauri-apps/api/app", () => ({ getVersion }));

const LOCAL: Endpoint = { host: "127.0.0.1", port: 49777 };
const REMOTE: Endpoint = { host: "mini", port: 49777 };

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  invoke.mockReset();
  getVersion.mockReset();
});

/** Marks the window as the Tauri shell, the way the runtime does. */
function inTauri(): void {
  Object.assign(window, { __TAURI_INTERNALS__: {} });
}

function setup(status: ConnectionStatus, endpoint: Endpoint): ReturnType<typeof vi.fn<AddToast>> {
  const addToast = vi.fn<AddToast>();
  renderHook(() => useUpdates(addToast, status, endpoint));
  return addToast;
}

describe("useUpdates", () => {
  it("does nothing outside the Tauri shell", async () => {
    const addToast = setup("connected", LOCAL);
    await Promise.resolve();
    expect(invoke).not.toHaveBeenCalled();
    expect(addToast).not.toHaveBeenCalled();
  });

  it("prompts when a newer app build is published", async () => {
    inTauri();
    getVersion.mockResolvedValue("0.4.0");
    invoke.mockImplementation((command) =>
      command === "check_for_update"
        ? Promise.resolve({ version: "0.5.0" })
        : Promise.resolve(null),
    );
    const addToast = setup("disconnected", LOCAL);
    await waitFor(() => {
      expect(addToast).toHaveBeenCalledWith(
        "info",
        "Update available",
        expect.stringContaining("0.5.0"),
        expect.objectContaining({
          sticky: true,
          action: expect.objectContaining({ label: "Restart & update" }),
        }),
      );
    });

    const action = addToast.mock.calls.find((call) => call[1] === "Update available")?.[3]?.action;
    expect(action).toBeDefined();
    action?.run();
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("install_update", { updateLocalDaemon: true });
    });
  });

  it("does not request a local daemon update for a remote connection", async () => {
    inTauri();
    invoke.mockImplementation((command) =>
      command === "check_for_update"
        ? Promise.resolve({ version: "0.5.0" })
        : Promise.resolve(null),
    );
    const addToast = setup("disconnected", REMOTE);
    await waitFor(() => {
      expect(addToast).toHaveBeenCalled();
    });

    const action = addToast.mock.calls.find((call) => call[1] === "Update available")?.[3]?.action;
    expect(action).toBeDefined();
    action?.run();
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("install_update", { updateLocalDaemon: false });
    });
  });

  it("reports a failed daemon refresh and leaves the relaunch to the user", async () => {
    inTauri();
    invoke.mockImplementation((command) => {
      if (command === "check_for_update") {
        return Promise.resolve({ version: "0.5.0" });
      }
      if (command === "install_update") {
        return Promise.resolve("daemon install failed: no such file");
      }
      return Promise.resolve(null);
    });
    const addToast = setup("disconnected", LOCAL);
    await waitFor(() => {
      expect(addToast).toHaveBeenCalled();
    });

    addToast.mock.calls.find((call) => call[1] === "Update available")?.[3]?.action?.run();
    await waitFor(() => {
      expect(addToast).toHaveBeenCalledWith(
        "error",
        "Daemon not updated",
        expect.stringContaining("no such file"),
        expect.objectContaining({
          sticky: true,
          action: expect.objectContaining({ label: "Relaunch" }),
        }),
      );
    });
    expect(invoke).not.toHaveBeenCalledWith("relaunch_app");

    const relaunch = addToast.mock.calls.find((call) => call[1] === "Daemon not updated")?.[3]
      ?.action;
    relaunch?.run();
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("relaunch_app");
    });
  });

  it("keeps the check timer through an endpoint change", async () => {
    inTauri();
    invoke.mockImplementation((command) =>
      command === "check_for_update"
        ? Promise.resolve({ version: "0.5.0" })
        : Promise.resolve(null),
    );
    const addToast = vi.fn<AddToast>();
    const { rerender } = renderHook(
      ({ endpoint }: { endpoint: Endpoint }) => useUpdates(addToast, "disconnected", endpoint),
      { initialProps: { endpoint: LOCAL } },
    );
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("check_for_update");
    });
    const checks = invoke.mock.calls.filter((call) => call[0] === "check_for_update").length;

    rerender({ endpoint: REMOTE });
    await Promise.resolve();
    expect(invoke.mock.calls.filter((call) => call[0] === "check_for_update")).toHaveLength(checks);
  });

  it("resolves the local-daemon flag from the endpoint at click time", async () => {
    inTauri();
    invoke.mockImplementation((command) =>
      command === "check_for_update"
        ? Promise.resolve({ version: "0.5.0" })
        : Promise.resolve(null),
    );
    const addToast = vi.fn<AddToast>();
    const { rerender } = renderHook(
      ({ endpoint }: { endpoint: Endpoint }) => useUpdates(addToast, "disconnected", endpoint),
      { initialProps: { endpoint: REMOTE } },
    );
    await waitFor(() => {
      expect(addToast).toHaveBeenCalled();
    });

    rerender({ endpoint: LOCAL });
    addToast.mock.calls.find((call) => call[1] === "Update available")?.[3]?.action?.run();
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("install_update", { updateLocalDaemon: true });
    });
  });

  it("stays quiet when the app build is current", async () => {
    inTauri();
    invoke.mockResolvedValue(null);
    const addToast = setup("disconnected", LOCAL);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("check_for_update");
    });
    expect(addToast).not.toHaveBeenCalled();
  });

  it("prompts when the local daemon is older than the app", async () => {
    inTauri();
    getVersion.mockResolvedValue("0.5.0");
    invoke.mockImplementation((command) => {
      if (command === "daemon_health") {
        return Promise.resolve({ status: "ok", version: "0.4.0" });
      }
      return Promise.resolve(null);
    });
    const addToast = setup("connected", LOCAL);
    await waitFor(() => {
      expect(addToast).toHaveBeenCalledWith(
        "info",
        "Daemon update available",
        expect.stringContaining("0.4.0"),
        expect.objectContaining({
          sticky: true,
          action: expect.objectContaining({ label: "Update daemon" }),
        }),
      );
    });
  });

  it("never probes a remote daemon", async () => {
    inTauri();
    getVersion.mockResolvedValue("0.5.0");
    invoke.mockResolvedValue(null);
    setup("connected", REMOTE);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("check_for_update");
    });
    expect(invoke).not.toHaveBeenCalledWith("daemon_health", expect.anything());
  });

  it("stays quiet when daemon and app versions match", async () => {
    inTauri();
    getVersion.mockResolvedValue("0.5.0");
    invoke.mockImplementation((command) => {
      if (command === "daemon_health") {
        return Promise.resolve({ status: "ok", version: "0.5.0" });
      }
      return Promise.resolve(null);
    });
    const addToast = setup("connected", LOCAL);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("daemon_health", expect.anything());
    });
    expect(addToast).not.toHaveBeenCalled();
  });

  it("offers the daemon update when the daemon is too old to complete a handshake", async () => {
    inTauri();
    getVersion.mockResolvedValue("0.8.0");
    invoke.mockImplementation((command) => {
      if (command === "daemon_health") {
        return Promise.resolve({ status: "ok", version: "0.6.0" });
      }
      return Promise.resolve(null);
    });
    const addToast = setup("version_mismatch", LOCAL);
    await waitFor(() => {
      expect(addToast).toHaveBeenCalledWith(
        "error",
        "Daemon too old to connect",
        expect.stringContaining("0.6.0"),
        expect.objectContaining({
          sticky: true,
          action: expect.objectContaining({ label: "Update daemon" }),
        }),
      );
    });

    addToast.mock.calls.find((call) => call[1] === "Daemon too old to connect")?.[3]?.action?.run();
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("install_local_daemon", { currentVersion: "0.6.0" });
    });
  });

  it("never downgrades a newer daemon after a protocol mismatch", async () => {
    inTauri();
    getVersion.mockResolvedValue("0.8.0");
    invoke.mockImplementation((command) => {
      if (command === "daemon_health") {
        return Promise.resolve({ status: "ok", version: "0.9.0" });
      }
      return Promise.resolve(null);
    });
    const addToast = setup("version_mismatch", LOCAL);
    await waitFor(() => {
      expect(addToast).toHaveBeenCalledWith(
        "error",
        "App too old to connect",
        expect.stringContaining("without downgrading"),
        { sticky: true },
      );
    });

    expect(invoke).not.toHaveBeenCalledWith("install_local_daemon", expect.anything());
  });

  it("waits for the connection to settle before probing the daemon", async () => {
    inTauri();
    getVersion.mockResolvedValue("0.8.0");
    invoke.mockResolvedValue(null);
    setup("connecting", LOCAL);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("check_for_update");
    });
    expect(invoke).not.toHaveBeenCalledWith("daemon_health", expect.anything());
  });

  it("does not probe on ordinary disconnections", async () => {
    inTauri();
    getVersion.mockResolvedValue("0.8.0");
    invoke.mockResolvedValue(null);
    setup("disconnected", LOCAL);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("check_for_update");
    });
    expect(invoke).not.toHaveBeenCalledWith("daemon_health", expect.anything());
  });
});
