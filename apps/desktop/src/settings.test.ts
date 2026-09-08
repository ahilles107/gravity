import { afterEach, describe, expect, it, vi } from "vitest";
import {
  DEFAULT_ENDPOINT,
  loadBotInfoPanel,
  loadDeviceToken,
  loadEndpoint,
  loadSetupComplete,
  markSetupComplete,
  saveBotInfoPanel,
  saveDeviceToken,
  saveEndpoint,
} from "./settings";

function stubStorage(initial: string | null): { readonly written: string[] } {
  const written: string[] = [];
  vi.stubGlobal("localStorage", {
    getItem: (): string | null => initial,
    setItem: (_key: string, value: string): void => {
      written.push(value);
    },
  });
  return { written };
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});

/** What `scripts/dev.sh` bakes in so a dev build finds its own daemon. */
function stubDevEnv(): void {
  vi.stubEnv("VITE_GRAVITY_DEV_PORT", "55571");
  vi.stubEnv("VITE_GRAVITY_DEV_TOKEN", "dev-daemon-token");
}

describe("loadEndpoint", () => {
  it("falls back to the default when storage is unavailable", () => {
    expect(loadEndpoint()).toEqual(DEFAULT_ENDPOINT);
  });

  it("falls back to the default when nothing is stored", () => {
    stubStorage(null);
    expect(loadEndpoint()).toEqual(DEFAULT_ENDPOINT);
  });

  it("falls back to the default for malformed or partial values", () => {
    stubStorage("{oops");
    expect(loadEndpoint()).toEqual(DEFAULT_ENDPOINT);
    stubStorage(JSON.stringify({ host: "", port: 1 }));
    expect(loadEndpoint()).toEqual(DEFAULT_ENDPOINT);
    stubStorage(JSON.stringify({ host: "h", port: 1.5 }));
    expect(loadEndpoint()).toEqual(DEFAULT_ENDPOINT);
  });

  it("uses the dev daemon when nothing is stored", () => {
    stubDevEnv();
    stubStorage(null);
    expect(loadEndpoint()).toEqual({ host: "127.0.0.1", port: 55571 });
  });

  it("prefers a stored endpoint over the dev daemon", () => {
    stubDevEnv();
    stubStorage(JSON.stringify({ host: "mini.ts.net", port: 7788 }));
    expect(loadEndpoint()).toEqual({ host: "mini.ts.net", port: 7788 });
  });

  it("ignores a malformed dev port", () => {
    vi.stubEnv("VITE_GRAVITY_DEV_PORT", "not-a-port");
    stubStorage(null);
    expect(loadEndpoint()).toEqual(DEFAULT_ENDPOINT);
  });

  it("reads a stored endpoint", () => {
    stubStorage(JSON.stringify({ host: "mini.ts.net", port: 7788 }));
    expect(loadEndpoint()).toEqual({ host: "mini.ts.net", port: 7788 });
  });
});

describe("saveEndpoint", () => {
  it("serializes the endpoint", () => {
    const storage = stubStorage(null);
    saveEndpoint({ host: "h", port: 1 });
    expect(storage.written).toEqual([JSON.stringify({ host: "h", port: 1 })]);
  });

  it("ignores storage failures", () => {
    vi.stubGlobal("localStorage", {
      getItem: (): string | null => null,
      setItem: (): never => {
        throw new Error("quota");
      },
    });
    expect(() => {
      saveEndpoint({ host: "h", port: 1 });
    }).not.toThrow();
  });
});

describe("device token", () => {
  it("returns an empty token when storage is unavailable", () => {
    expect(loadDeviceToken()).toBe("");
  });

  it("falls back to the dev daemon token", () => {
    stubDevEnv();
    stubStorage(null);
    expect(loadDeviceToken()).toBe("dev-daemon-token");
  });

  it("round-trips through storage", () => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string): string | null => store.get(key) ?? null,
      setItem: (key: string, value: string): void => void store.set(key, value),
      removeItem: (key: string): void => void store.delete(key),
    });
    saveDeviceToken("dev-token");
    expect(loadDeviceToken()).toBe("dev-token");
    saveDeviceToken("");
    expect(loadDeviceToken()).toBe("");
  });
});

describe("setup complete flag", () => {
  it("defaults to false and latches to true", () => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string): string | null => store.get(key) ?? null,
      setItem: (key: string, value: string): void => void store.set(key, value),
      removeItem: (key: string): void => void store.delete(key),
    });
    expect(loadSetupComplete()).toBe(false);
    markSetupComplete();
    expect(loadSetupComplete()).toBe(true);
  });

  it("skips the wizard for a dev build", () => {
    stubDevEnv();
    stubStorage(null);
    expect(loadSetupComplete()).toBe(true);
  });

  it("survives storage failures", () => {
    expect(loadSetupComplete()).toBe(false);
    expect(() => {
      markSetupComplete();
    }).not.toThrow();
  });
});

describe("bot info panel", () => {
  it("loads a saved layout", () => {
    stubStorage(JSON.stringify({ collapsed: true, width: 418 }));
    expect(loadBotInfoPanel()).toEqual({ collapsed: true, width: 418 });
  });

  it("falls back for malformed layouts", () => {
    stubStorage(JSON.stringify({ collapsed: "yes", width: 1 }));
    expect(loadBotInfoPanel()).toEqual({ collapsed: false, width: 360 });
  });

  it("saves the layout", () => {
    const storage = stubStorage(null);
    saveBotInfoPanel({ collapsed: false, width: 392 });
    expect(storage.written).toEqual([JSON.stringify({ collapsed: false, width: 392 })]);
  });
});
