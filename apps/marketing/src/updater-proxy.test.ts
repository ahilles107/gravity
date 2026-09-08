import { afterEach, describe, expect, it, vi } from "vitest";

import { proxyUpdateManifest } from "./updater-proxy";

const upstream = "https://github.com/example/app/releases/latest/download/latest.json";
const legacy = "https://updates.example/desktop/gravity/latest.json";
const body = JSON.stringify({
  version: "0.12.4",
  platforms: {
    "darwin-aarch64": {
      url: "https://github.com/example/app/releases/download/v0.12.4/gravity.app.tar.gz",
      signature: "original-signature",
    },
  },
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("legacy update manifest bridge", () => {
  it("preserves manifest bytes and does not forward caller credentials or query parameters", async () => {
    const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(new Response(body));
    vi.stubGlobal("fetch", fetch);
    const response = await proxyUpdateManifest(
      new Request(`${legacy}?old-client=true`, {
        headers: { authorization: "private-client-token" },
      }),
      upstream,
    );

    expect(response.status).toBe(200);
    expect(await response.text()).toBe(body);
    expect(response.headers.get("content-type")).toBe("application/json");
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(fetch).toHaveBeenCalledExactlyOnceWith(upstream, { redirect: "follow" });
  });

  it("supports HEAD without a response body", async () => {
    vi.stubGlobal("fetch", vi.fn<typeof globalThis.fetch>().mockResolvedValue(new Response(body)));
    const response = await proxyUpdateManifest(new Request(legacy, { method: "HEAD" }), upstream);
    expect(response.status).toBe(200);
    expect(await response.text()).toBe("");
  });

  it.each([404, 403, 500])("fails without caching an upstream HTTP %i", async (status) => {
    vi.stubGlobal(
      "fetch",
      vi.fn<typeof globalThis.fetch>().mockResolvedValue(new Response("unavailable", { status })),
    );
    const response = await proxyUpdateManifest(new Request(legacy), upstream);
    expect(response.status).toBe(502);
    expect(response.headers.get("cache-control")).toBe("no-store");
  });

  it.each([
    "<html>error</html>",
    "{}",
    '{"platforms":{"darwin-aarch64":{"url":"bad.app.tar.gz"}}}',
  ])("rejects invalid upstream manifests", async (invalid) => {
    vi.stubGlobal(
      "fetch",
      vi.fn<typeof globalThis.fetch>().mockResolvedValue(new Response(invalid)),
    );
    expect((await proxyUpdateManifest(new Request(legacy), upstream)).status).toBe(502);
  });

  it("handles an upstream network failure", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn<typeof globalThis.fetch>().mockRejectedValue(new Error("offline")),
    );
    expect((await proxyUpdateManifest(new Request(legacy), upstream)).status).toBe(502);
  });

  it("does not fetch for other paths, unsupported methods or missing configuration", async () => {
    const fetch = vi.fn<typeof globalThis.fetch>();
    vi.stubGlobal("fetch", fetch);
    expect((await proxyUpdateManifest(new Request(`${legacy}/other`), upstream)).status).toBe(404);
    expect(
      (await proxyUpdateManifest(new Request(legacy, { method: "POST" }), upstream)).status,
    ).toBe(405);
    expect((await proxyUpdateManifest(new Request(legacy), "")).status).toBe(503);
    expect(fetch).not.toHaveBeenCalled();
  });
});
