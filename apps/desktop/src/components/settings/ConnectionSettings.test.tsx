import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DaemonConfig, NotifyLevel } from "../../protocol/entities";
import type { Endpoint } from "../../protocol/connection";
import { FakeDaemon } from "../../test/fakeDaemon";
import { toastSpy, voidSpy } from "../../test/spies";
import ConnectionSettings from "./ConnectionSettings";

const LOCAL: Endpoint = { host: "127.0.0.1", port: 49777 };
const REMOTE: Endpoint = { host: "mini", port: 49777 };
/** The workspace-private daemon `scripts/dev.sh` starts, on its own port. */
const DEV: Endpoint = { host: "127.0.0.1", port: 55041 };

const invoke = vi.hoisted(() => vi.fn<(command: string, args?: unknown) => Promise<unknown>>());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const CONFIG: DaemonConfig = {
  bind: ["127.0.0.1"],
  port: 49777,
  configured_port: 49777,
  runtime: "pty",
  auto_compact_window: null,
};

beforeEach(() => {
  Object.assign(window, { __TAURI_INTERNALS__: {} });
  // Only the managed daemon on its own port is restartable.
  invoke.mockImplementation((command, args) => {
    if (command === "local_daemon_is_managed") {
      const port = (args as { readonly port: number }).port;
      return Promise.resolve(port === LOCAL.port);
    }
    return Promise.resolve(null);
  });
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  invoke.mockReset();
});

function renderPane(endpoint: Endpoint): void {
  const daemon = new FakeDaemon().onRequest("get_config", () => ({
    type: "config",
    req_id: "1",
    config: CONFIG,
  }));
  const onToast: (level: NotifyLevel, title: string, body: string) => void = toastSpy();
  render(
    <ConnectionSettings
      client={daemon}
      status="connected"
      endpoint={endpoint}
      connected
      canControl
      onChangeEndpoint={voidSpy()}
      onToast={onToast}
    />,
  );
}

describe("ConnectionSettings", () => {
  it("offers the restart for the app-managed daemon on this machine", async () => {
    renderPane(LOCAL);
    expect(await screen.findByRole("button", { name: "Restart" })).toBeInTheDocument();
  });

  it("hides the restart for a remote daemon", async () => {
    renderPane(REMOTE);
    await screen.findByLabelText("Auto-compact window");
    expect(screen.queryByRole("button", { name: "Restart" })).not.toBeInTheDocument();
    // A remote daemon is settled without asking this machine anything.
    expect(invoke).not.toHaveBeenCalled();
  });

  it("hides the restart for a dev daemon launchd does not manage", async () => {
    renderPane(DEV);
    await screen.findByLabelText("Auto-compact window");
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("local_daemon_is_managed", { port: DEV.port });
    });
    expect(screen.queryByRole("button", { name: "Restart" })).not.toBeInTheDocument();
  });
});
