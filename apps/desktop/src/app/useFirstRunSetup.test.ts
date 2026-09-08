import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Endpoint } from "../protocol/connection";
import { DEFAULT_ENDPOINT } from "../settings";
import { FakeDaemon } from "../test/fakeDaemon";
import { useFirstRunSetup } from "./useFirstRunSetup";

afterEach(() => {
  vi.unstubAllGlobals();
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

/** Marks the window as the Tauri shell, the way the runtime does. */
function inTauri(): void {
  Object.assign(window, { __TAURI_INTERNALS__: {} });
}

function stubStorage(): Map<string, string> {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string): string | null => store.get(key) ?? null,
    setItem: (key: string, value: string): void => void store.set(key, value),
    removeItem: (key: string): void => void store.delete(key),
  });
  return store;
}

function setup(): {
  readonly daemon: FakeDaemon;
  readonly changeEndpoint: ReturnType<typeof vi.fn<(endpoint: Endpoint) => void>>;
  readonly result: { readonly current: ReturnType<typeof useFirstRunSetup> };
} {
  const daemon = new FakeDaemon();
  daemon.status = "disconnected";
  const changeEndpoint = vi.fn<(endpoint: Endpoint) => void>();
  const { result } = renderHook(() => useFirstRunSetup(daemon, changeEndpoint));
  return { daemon, changeEndpoint, result };
}

describe("useFirstRunSetup", () => {
  it("hides the wizard outside the Tauri shell", () => {
    stubStorage();
    const { result } = setup();
    expect(result.current.showWizard).toBe(false);
  });

  it("shows the wizard in the Tauri shell until the first connect", () => {
    const store = stubStorage();
    inTauri();
    const { daemon, result } = setup();
    expect(result.current.showWizard).toBe(true);
    act(() => {
      daemon.setStatus("connected");
    });
    expect(result.current.showWizard).toBe(false);
    // The flag latches: a later disconnect never resurfaces the wizard.
    act(() => {
      daemon.setStatus("disconnected");
    });
    expect(result.current.showWizard).toBe(false);
    expect(store.get("gravity.setup-complete")).toBe("true");
  });

  it("stays hidden on later launches once setup completed", () => {
    const store = stubStorage();
    store.set("gravity.setup-complete", "true");
    inTauri();
    const { result } = setup();
    expect(result.current.showWizard).toBe(false);
  });

  it("points the client at the port the local daemon was resolved on", () => {
    stubStorage();
    inTauri();
    const { changeEndpoint, result } = setup();
    act(() => {
      result.current.connectToDaemon({
        method: "local",
        endpoint: { host: DEFAULT_ENDPOINT.host, port: 7777 },
      });
    });
    expect(changeEndpoint).toHaveBeenCalledWith({ host: DEFAULT_ENDPOINT.host, port: 7777 });
  });

  it("stores the device token before switching to the remote endpoint", () => {
    const store = stubStorage();
    inTauri();
    const { changeEndpoint, result } = setup();
    act(() => {
      result.current.connectToDaemon({
        method: "remote",
        endpoint: { host: "mini.ts.net", port: 7788 },
        token: "tok-1",
      });
    });
    expect(store.get("gravity.device-token")).toBe("tok-1");
    expect(changeEndpoint).toHaveBeenCalledWith({ host: "mini.ts.net", port: 7788 });
  });

  it("leaves an existing device token alone when none is supplied", () => {
    const store = stubStorage();
    store.set("gravity.device-token", "tok-old");
    inTauri();
    const { changeEndpoint, result } = setup();
    act(() => {
      result.current.connectToDaemon({
        method: "remote",
        endpoint: { host: "mini.ts.net", port: 7788 },
      });
    });
    expect(store.get("gravity.device-token")).toBe("tok-old");
    expect(changeEndpoint).toHaveBeenCalledWith({ host: "mini.ts.net", port: 7788 });
  });
});
