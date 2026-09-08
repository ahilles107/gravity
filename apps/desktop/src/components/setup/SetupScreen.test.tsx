import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_ENDPOINT } from "../../settings";
import SetupScreen from "./SetupScreen";

const probeDaemon = vi.hoisted(() => vi.fn<(endpoint: unknown) => Promise<unknown>>());
const installLocalDaemon = vi.hoisted(() => vi.fn<() => Promise<void>>());
const localDaemonEndpoint = vi.hoisted(() => vi.fn<() => Promise<unknown>>());
const daemonLogTail = vi.hoisted(() => vi.fn<() => Promise<string>>());
vi.mock("../../setup", () => ({
  probeDaemon,
  installLocalDaemon,
  localDaemonEndpoint,
  daemonLogTail,
}));

beforeEach(() => {
  localDaemonEndpoint.mockResolvedValue(DEFAULT_ENDPOINT);
  daemonLogTail.mockResolvedValue("");
});

afterEach(() => {
  probeDaemon.mockReset();
  installLocalDaemon.mockReset();
  localDaemonEndpoint.mockReset();
  daemonLogTail.mockReset();
});

const HEALTH = { status: "ok", version: "0.1.0" };
/** Longer than the wizard's 30 one-second polls. */
const POLL_TIMEOUT_MS = 40_000;

describe("SetupScreen", () => {
  it("connects straight away when a local daemon answers", async () => {
    probeDaemon.mockResolvedValue(HEALTH);
    const onConnect = vi.fn<() => void>();
    render(<SetupScreen onConnect={onConnect} />);
    await waitFor(() => {
      expect(screen.getByText(/Connecting to the daemon/)).toBeInTheDocument();
    });
    expect(onConnect).toHaveBeenCalledWith({ method: "local", endpoint: DEFAULT_ENDPOINT });
  });

  it("uses the port the installed daemon reports", async () => {
    const legacy = { host: "127.0.0.1", port: 7777 };
    localDaemonEndpoint.mockResolvedValue(legacy);
    probeDaemon.mockResolvedValue(HEALTH);
    const onConnect = vi.fn<(connection: unknown) => void>();
    render(<SetupScreen onConnect={onConnect} />);
    await waitFor(() => {
      expect(onConnect).toHaveBeenCalledWith({ method: "local", endpoint: legacy });
    });
    expect(probeDaemon).toHaveBeenCalledWith(legacy);
  });

  it("installs the local daemon automatically and connects once it is reachable", async () => {
    probeDaemon.mockResolvedValueOnce(null);
    installLocalDaemon.mockResolvedValue(undefined);
    probeDaemon.mockResolvedValue(HEALTH);
    const onConnect = vi.fn<(connection: unknown) => void>();
    render(<SetupScreen onConnect={onConnect} />);
    await waitFor(() => {
      expect(onConnect).toHaveBeenCalledWith({ method: "local", endpoint: DEFAULT_ENDPOINT });
    });
    expect(installLocalDaemon).toHaveBeenCalledTimes(1);
  });

  it("says what the install puts on the machine", async () => {
    probeDaemon.mockResolvedValue(null);
    installLocalDaemon.mockReturnValue(new Promise(() => undefined));
    render(<SetupScreen onConnect={vi.fn<() => void>()} />);
    expect(
      await screen.findByText(/launchd agent in ~\/Library\/LaunchAgents/),
    ).toBeInTheDocument();
  });

  it("reports how long it has waited for the daemon to answer", async () => {
    probeDaemon.mockResolvedValue(null);
    installLocalDaemon.mockResolvedValue(undefined);
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      render(<SetupScreen onConnect={vi.fn<() => void>()} />);
      await vi.advanceTimersByTimeAsync(3000);
      expect(await screen.findByText(/Waiting for it to answer — \ds of 30s/)).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("connects on the port negotiated during installation", async () => {
    const negotiated = { host: DEFAULT_ENDPOINT.host, port: 50_123 };
    localDaemonEndpoint.mockResolvedValueOnce(DEFAULT_ENDPOINT).mockResolvedValue(negotiated);
    probeDaemon.mockResolvedValueOnce(null).mockResolvedValue(HEALTH);
    installLocalDaemon.mockResolvedValue(undefined);
    const onConnect = vi.fn<(connection: unknown) => void>();
    render(<SetupScreen onConnect={onConnect} />);

    await waitFor(() => {
      expect(onConnect).toHaveBeenCalledWith({ method: "local", endpoint: negotiated });
    });
  });

  it("shows the daemon log when the install never comes up", async () => {
    probeDaemon.mockResolvedValue(null);
    installLocalDaemon.mockResolvedValue(undefined);
    daemonLogTail.mockResolvedValue("Error: Address already in use (os error 48)");
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      render(<SetupScreen onConnect={vi.fn<() => void>()} />);
      await vi.advanceTimersByTimeAsync(POLL_TIMEOUT_MS);
      expect(await screen.findByText(/Address already in use/)).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("re-probes rather than reinstalling when a daemon turned up meanwhile", async () => {
    probeDaemon.mockResolvedValueOnce(null);
    installLocalDaemon.mockRejectedValueOnce(new Error("bundled daemon not found"));
    const onConnect = vi.fn<(connection: unknown) => void>();
    render(<SetupScreen onConnect={onConnect} />);
    expect(await screen.findByText("bundled daemon not found")).toBeInTheDocument();

    probeDaemon.mockResolvedValue(HEALTH);
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(onConnect).toHaveBeenCalledWith({ method: "local", endpoint: DEFAULT_ENDPOINT });
    });
    expect(installLocalDaemon).toHaveBeenCalledTimes(1);
  });

  it("retries the install when nothing is answering yet", async () => {
    probeDaemon.mockResolvedValue(null);
    installLocalDaemon.mockRejectedValueOnce(new Error("bundled daemon not found"));
    const onConnect = vi.fn<(connection: unknown) => void>();
    render(<SetupScreen onConnect={onConnect} />);
    expect(await screen.findByText("bundled daemon not found")).toBeInTheDocument();

    installLocalDaemon.mockResolvedValue(undefined);
    probeDaemon.mockResolvedValueOnce(null).mockResolvedValue(HEALTH);
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(onConnect).toHaveBeenCalledWith({ method: "local", endpoint: DEFAULT_ENDPOINT });
    });
    expect(installLocalDaemon).toHaveBeenCalledTimes(2);
  });

  it("falls back to a typed-in daemon when the install cannot be fixed", async () => {
    probeDaemon.mockResolvedValue(null);
    installLocalDaemon.mockRejectedValue(new Error("bundled daemon not found"));
    const onConnect = vi.fn<(connection: unknown) => void>();
    render(<SetupScreen onConnect={onConnect} />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Connect to a different daemon" }),
    );
    await userEvent.clear(screen.getByLabelText("Daemon host"));
    await userEvent.type(screen.getByLabelText("Daemon host"), "mini.ts.net");
    await userEvent.type(screen.getByLabelText("Device token"), "  tok-123 ");
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(onConnect).toHaveBeenCalledWith({
      method: "remote",
      endpoint: { host: "mini.ts.net", port: DEFAULT_ENDPOINT.port },
      token: "tok-123",
    });
    expect(screen.getByText(/Connecting to the daemon/)).toBeInTheDocument();
  });

  it("omits the token when the fallback form leaves it blank", async () => {
    probeDaemon.mockResolvedValue(null);
    installLocalDaemon.mockRejectedValue(new Error("bundled daemon not found"));
    const onConnect = vi.fn<(connection: unknown) => void>();
    render(<SetupScreen onConnect={onConnect} />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Connect to a different daemon" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(onConnect).toHaveBeenCalledWith({
      method: "remote",
      endpoint: DEFAULT_ENDPOINT,
    });
  });

  it("keeps Connect disabled until the fallback form has an address", async () => {
    probeDaemon.mockResolvedValue(null);
    installLocalDaemon.mockRejectedValue(new Error("bundled daemon not found"));
    render(<SetupScreen onConnect={vi.fn<() => void>()} />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Connect to a different daemon" }),
    );
    await userEvent.clear(screen.getByLabelText("Daemon host"));
    expect(screen.getByRole("button", { name: "Connect" })).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Daemon host"), "mini");
    expect(screen.getByRole("button", { name: "Connect" })).toBeEnabled();
  });

  it("returns to the install error from the fallback form", async () => {
    probeDaemon.mockResolvedValue(null);
    installLocalDaemon.mockRejectedValue(new Error("bundled daemon not found"));
    render(<SetupScreen onConnect={vi.fn<() => void>()} />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Connect to a different daemon" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByText("bundled daemon not found")).toBeInTheDocument();
  });
});
