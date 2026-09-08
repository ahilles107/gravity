import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AddToast } from "../../app/useToasts";
import type { Endpoint } from "../../protocol/connection";
import AboutSettings from "./AboutSettings";

const LOCAL: Endpoint = { host: "127.0.0.1", port: 49777 };
const REMOTE: Endpoint = { host: "mini", port: 49777 };

const invoke = vi.hoisted(() => vi.fn<(command: string, args?: unknown) => Promise<unknown>>());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const getVersion = vi.hoisted(() => vi.fn<() => Promise<string>>());
vi.mock("@tauri-apps/api/app", () => ({ getVersion }));

vi.mock("../../analytics", () => ({
  captureException: vi.fn<(error: unknown, context: string) => void>(),
}));

beforeEach(() => {
  Object.assign(window, { __TAURI_INTERNALS__: {} });
  getVersion.mockResolvedValue("0.7.0");
  invoke.mockResolvedValue(null);
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  getVersion.mockReset();
  invoke.mockReset();
});

function renderAbout(
  endpoint: Endpoint,
  daemonVersion: string,
): ReturnType<typeof vi.fn<AddToast>> {
  const addToast = vi.fn<AddToast>();
  render(<AboutSettings endpoint={endpoint} daemonVersion={daemonVersion} addToast={addToast} />);
  return addToast;
}

describe("AboutSettings", () => {
  it("offers a persistent local daemon update when versions differ", async () => {
    const user = userEvent.setup();
    const addToast = renderAbout(LOCAL, "0.6.0");

    await user.click(await screen.findByRole("button", { name: "Update daemon" }));

    expect(invoke).toHaveBeenCalledWith("install_local_daemon", { currentVersion: "0.6.0" });
    await waitFor(() => {
      expect(addToast).toHaveBeenCalledWith(
        "info",
        "Daemon updated",
        "The local daemon is restarting.",
      );
    });
  });

  it("offers the update for a local daemon too old to complete a handshake", async () => {
    invoke.mockImplementation((command) =>
      command === "daemon_health"
        ? Promise.resolve({ status: "ok", version: "0.6.0" })
        : Promise.resolve(null),
    );
    const user = userEvent.setup();
    renderAbout(LOCAL, "");

    expect(await screen.findByText("0.6.0 · not connected")).toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "Update daemon" }));

    expect(invoke).toHaveBeenCalledWith("install_local_daemon", { currentVersion: "0.6.0" });
  });

  it("shows nothing to update when no local daemon answers", async () => {
    renderAbout(LOCAL, "");

    expect(await screen.findByText("not connected")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Update daemon" })).not.toBeInTheDocument();
  });

  it("never offers to downgrade a newer local daemon", async () => {
    invoke.mockImplementation((command) =>
      command === "daemon_health"
        ? Promise.resolve({ status: "ok", version: "0.9.0" })
        : Promise.resolve(null),
    );
    renderAbout(LOCAL, "");

    expect(await screen.findByText("0.9.0 · not connected")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Update daemon" })).not.toBeInTheDocument();
  });

  it("never offers to replace a remote daemon", async () => {
    renderAbout(REMOTE, "0.6.0");

    expect(await screen.findByText("0.7.0")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Update daemon" })).not.toBeInTheDocument();
  });
});
