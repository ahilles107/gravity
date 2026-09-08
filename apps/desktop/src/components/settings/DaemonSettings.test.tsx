import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DaemonConfig } from "../../protocol/entities";
import { FakeDaemon } from "../../test/fakeDaemon";
import { toastSpy } from "../../test/spies";
import DaemonSettings from "./DaemonSettings";

const invoke = vi.hoisted(() => vi.fn<(command: string, args?: unknown) => Promise<unknown>>());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

vi.mock("../../analytics", () => ({
  captureException: vi.fn<(error: unknown, context: string) => void>(),
}));

function config(over: Partial<DaemonConfig> = {}): DaemonConfig {
  return {
    bind: ["127.0.0.1"],
    port: 49777,
    configured_port: 49777,
    runtime: "pty",
    auto_compact_window: 250_000,
    ...over,
  };
}

function baseDaemon(): FakeDaemon {
  return new FakeDaemon().onRequest("get_config", () => ({
    type: "config",
    req_id: "1",
    config: config(),
  }));
}

function renderPane(daemon: FakeDaemon, canControl = true, canRestart = false) {
  const onToast = toastSpy();
  render(
    <DaemonSettings
      client={daemon}
      connected
      canControl={canControl}
      canRestart={canRestart}
      onToast={onToast}
    />,
  );
  return { onToast };
}

afterEach(() => {
  invoke.mockReset();
});

describe("DaemonSettings", () => {
  it("shows the read-only launch config and the current window", async () => {
    renderPane(baseDaemon());
    await waitFor(() => {
      expect(screen.getByText("127.0.0.1")).toBeInTheDocument();
    });
    expect(screen.getByText("49777")).toBeInTheDocument();
    expect(screen.getByText("pty")).toBeInTheDocument();
    expect(screen.getByLabelText("Auto-compact window")).toHaveValue("250000");
  });

  it("flags a negotiated port as the compromise it is", async () => {
    const daemon = new FakeDaemon().onRequest("get_config", () => ({
      type: "config" as const,
      req_id: "1",
      config: config({ port: 50_123, configured_port: 49_777 }),
    }));
    renderPane(daemon);

    expect(await screen.findByText("50123 (not 49777)")).toBeInTheDocument();
    expect(screen.getByText(/bot bus moved with it/)).toBeInTheDocument();
  });

  it("saves a new auto-compact window", async () => {
    const user = userEvent.setup();
    const daemon = baseDaemon().onRequest("set_config", (body) => {
      if (body.type !== "set_config") {
        throw new Error("unexpected request");
      }
      return {
        type: "config",
        req_id: "1",
        config: config({ auto_compact_window: body.auto_compact_window }),
      };
    });
    const { onToast } = renderPane(daemon);

    const input = await screen.findByLabelText("Auto-compact window");
    await user.clear(input);
    await user.type(input, "300000");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("info", "Daemon updated", expect.any(String));
    });
    const sent = daemon.requests.find((r) => r.body.type === "set_config");
    expect(sent?.body).toEqual({ type: "set_config", auto_compact_window: 300_000 });
  });

  it("clears the window to the model default with an empty field", async () => {
    const user = userEvent.setup();
    const daemon = baseDaemon().onRequest("set_config", () => ({
      type: "config",
      req_id: "1",
      config: config({ auto_compact_window: null }),
    }));
    renderPane(daemon);

    const input = await screen.findByLabelText("Auto-compact window");
    await user.clear(input);
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const sent = daemon.requests.find((r) => r.body.type === "set_config");
      expect(sent?.body).toEqual({ type: "set_config", auto_compact_window: null });
    });
    expect(input).toHaveValue("");
  });

  it("rejects an out-of-range window before sending anything", async () => {
    const user = userEvent.setup();
    const daemon = baseDaemon();
    renderPane(daemon);

    const input = await screen.findByLabelText("Auto-compact window");
    await user.clear(input);
    await user.type(input, "999");

    expect(screen.getByText("Enter 100000–1000000, or leave empty.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    expect(daemon.requests.some((r) => r.body.type === "set_config")).toBe(false);
  });

  it("surfaces a save failure", async () => {
    const user = userEvent.setup();
    const daemon = baseDaemon().onRequest("set_config", () => {
      throw new Error("forbidden");
    });
    const { onToast } = renderPane(daemon);

    const input = await screen.findByLabelText("Auto-compact window");
    await user.clear(input);
    await user.type(input, "300000");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("error", "Update failed", "forbidden");
    });
  });

  it("locks the field and hides save without the control grant", async () => {
    renderPane(baseDaemon(), false);
    const input = await screen.findByLabelText("Auto-compact window");
    expect(input).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
  });

  it("reports a load failure", async () => {
    const daemon = new FakeDaemon().onRequest("get_config", () => {
      throw new Error("no daemon");
    });
    renderPane(daemon);
    await waitFor(() => {
      expect(screen.getByText("Failed to load daemon config: no daemon")).toBeInTheDocument();
    });
  });

  it("hides the restart control for a daemon this app does not manage", async () => {
    renderPane(baseDaemon());
    await screen.findByLabelText("Auto-compact window");
    expect(screen.queryByRole("button", { name: "Restart" })).not.toBeInTheDocument();
  });

  it("warns about the running sessions and restarts only once confirmed", async () => {
    const user = userEvent.setup();
    invoke.mockResolvedValue(null);
    const { onToast } = renderPane(baseDaemon(), true, true);

    await user.click(await screen.findByRole("button", { name: "Restart" }));
    expect(screen.getByText(/any in-flight turn is gone/)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Restart daemon" }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("restart_local_daemon");
    });
    expect(onToast).toHaveBeenCalledWith("info", "Daemon restarting", expect.any(String));
  });

  it("leaves the daemon alone when the warning is dismissed", async () => {
    const user = userEvent.setup();
    const { onToast } = renderPane(baseDaemon(), true, true);

    await user.click(await screen.findByRole("button", { name: "Restart" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(screen.queryByText(/any in-flight turn is gone/)).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
    expect(onToast).not.toHaveBeenCalled();
  });

  it("surfaces a restart failure", async () => {
    const user = userEvent.setup();
    invoke.mockRejectedValue("no app-managed daemon is installed on this machine");
    const { onToast } = renderPane(baseDaemon(), true, true);

    await user.click(await screen.findByRole("button", { name: "Restart" }));
    await user.click(screen.getByRole("button", { name: "Restart daemon" }));

    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith(
        "error",
        "Restart failed",
        "no app-managed daemon is installed on this machine",
      );
    });
  });

  it("still offers the restart when the config never loaded", async () => {
    const daemon = new FakeDaemon().onRequest("get_config", () => {
      throw new Error("no daemon");
    });
    renderPane(daemon, true, true);
    expect(await screen.findByRole("button", { name: "Restart" })).toBeInTheDocument();
  });
});
