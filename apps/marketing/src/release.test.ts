import { describe, expect, it } from "vitest";

import { resolveMacDownloadUrl } from "./release";

describe("resolveMacDownloadUrl", () => {
  it("turns the current macOS updater archive into its matching DMG", () => {
    const manifest = {
      platforms: {
        "darwin-aarch64": {
          url: "https://downloads.example/desktop/gravity/0.9.0/gravity.app.tar.gz",
        },
      },
    };

    expect(resolveMacDownloadUrl(manifest)).toBe(
      "https://downloads.example/desktop/gravity/0.9.0/gravity.dmg",
    );
  });

  it.each([
    undefined,
    {},
    { platforms: null },
    { platforms: { "darwin-aarch64": {} } },
    { platforms: { "darwin-aarch64": { url: "http://downloads.example/gravity.app.tar.gz" } } },
    { platforms: { "darwin-aarch64": { url: "https://downloads.example/gravity.zip" } } },
  ])("rejects an unsupported manifest", (manifest) => {
    expect(resolveMacDownloadUrl(manifest)).toBeNull();
  });
});
