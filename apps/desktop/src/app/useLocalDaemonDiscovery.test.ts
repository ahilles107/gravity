import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Endpoint } from "../protocol/connection";
import { DEFAULT_ENDPOINT } from "../settings";
import { FakeDaemon } from "../test/fakeDaemon";
import { useLocalDaemonDiscovery } from "./useLocalDaemonDiscovery";

const localDaemonEndpoint = vi.hoisted(() => vi.fn<() => Promise<Endpoint>>());
vi.mock("../setup", () => ({ localDaemonEndpoint }));

const NEGOTIATED: Endpoint = { host: DEFAULT_ENDPOINT.host, port: 50_123 };

beforeEach(() => {
  localDaemonEndpoint.mockReset();
  localDaemonEndpoint.mockResolvedValue(DEFAULT_ENDPOINT);
});

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

/** Marks the window as the Tauri shell, the way the runtime does. */
function inTauri(): void {
  Object.assign(window, { __TAURI_INTERNALS__: {} });
}

function setup(endpoint?: Endpoint): {
  readonly daemon: FakeDaemon;
  readonly changeEndpoint: ReturnType<typeof vi.fn<(endpoint: Endpoint) => void>>;
} {
  const daemon = new FakeDaemon();
  daemon.status = "disconnected";
  if (endpoint !== undefined) {
    daemon.setEndpoint(endpoint);
  }
  const changeEndpoint = vi.fn<(endpoint: Endpoint) => void>();
  renderHook(() => {
    useLocalDaemonDiscovery(daemon, changeEndpoint);
  });
  return { daemon, changeEndpoint };
}

describe("useLocalDaemonDiscovery", () => {
  it("follows a negotiated port while a local connection is down", async () => {
    inTauri();
    localDaemonEndpoint.mockResolvedValue(NEGOTIATED);
    const { changeEndpoint } = setup();

    await waitFor(() => {
      expect(changeEndpoint).toHaveBeenCalledWith(NEGOTIATED);
    });
  });

  it("leaves a connected client alone", async () => {
    inTauri();
    localDaemonEndpoint.mockResolvedValue(NEGOTIATED);
    const { daemon, changeEndpoint } = setup();
    act(() => {
      daemon.setStatus("connected");
    });

    await Promise.resolve();
    expect(changeEndpoint).not.toHaveBeenCalled();
  });

  it("leaves a remote endpoint alone", async () => {
    inTauri();
    localDaemonEndpoint.mockResolvedValue(NEGOTIATED);
    const { changeEndpoint } = setup({ host: "mini", port: 49_777 });

    await Promise.resolve();
    expect(changeEndpoint).not.toHaveBeenCalled();
  });

  it("does nothing outside the Tauri shell", async () => {
    localDaemonEndpoint.mockResolvedValue(NEGOTIATED);
    const { changeEndpoint } = setup();

    await Promise.resolve();
    expect(changeEndpoint).not.toHaveBeenCalled();
    expect(localDaemonEndpoint).not.toHaveBeenCalled();
  });
});
