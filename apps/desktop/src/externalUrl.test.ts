import { afterEach, describe, expect, it, vi } from "vitest";
import { openExternalUrl } from "./externalUrl";

const invoke = vi.hoisted(() => vi.fn<(command: string, args: unknown) => Promise<unknown>>());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

afterEach(() => {
  vi.unstubAllGlobals();
  invoke.mockReset();
});

describe("openExternalUrl", () => {
  it("opens web URLs through the Tauri shell", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invoke.mockResolvedValue(null);

    await openExternalUrl("https://example.com/docs");

    expect(invoke).toHaveBeenCalledWith("open_external_url", {
      url: "https://example.com/docs",
    });
  });

  it("opens web URLs in a new tab during browser development", async () => {
    const open = vi.fn<(url: string, target: string, features: string) => null>(() => null);
    vi.stubGlobal("window", { open });

    await openExternalUrl("http://localhost:5173/docs");

    expect(open).toHaveBeenCalledWith(
      "http://localhost:5173/docs",
      "_blank",
      "noopener,noreferrer",
    );
  });

  it("rejects non-web protocols", async () => {
    const open = vi.fn<() => void>();
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {}, open });

    await expect(openExternalUrl("javascript:alert(document.cookie)")).rejects.toThrow(
      /non-web URL/,
    );

    expect(invoke).not.toHaveBeenCalled();
    expect(open).not.toHaveBeenCalled();
  });

  it("surfaces a launcher failure to the caller", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invoke.mockRejectedValue(new Error("open exited with 1"));

    await expect(openExternalUrl("https://example.com/docs")).rejects.toThrow("open exited with 1");
  });
});
