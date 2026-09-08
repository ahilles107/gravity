import { describe, expect, it } from "vitest";
import { compareVersions } from "./version";

describe("compareVersions", () => {
  it("orders release versions numerically", () => {
    expect(compareVersions("0.8.0", "0.7.9")).toBe("newer");
    expect(compareVersions("0.8.0", "0.9.0")).toBe("older");
    expect(compareVersions("10.0.0", "2.0.0")).toBe("newer");
    expect(compareVersions("0.8.0+desktop", "0.8.0+daemon")).toBe("same");
  });

  it("orders prereleases before releases", () => {
    expect(compareVersions("0.8.0-beta.2", "0.8.0-beta.1")).toBe("newer");
    expect(compareVersions("0.8.0-beta.1", "0.8.0")).toBe("older");
  });

  it("rejects non-semantic and invalid versions", () => {
    expect(compareVersions("dev", "0.8.0")).toBe("unknown");
    expect(compareVersions("0.8.0", "0.08.0")).toBe("unknown");
    expect(compareVersions("0.8.0-beta.01", "0.8.0-beta.1")).toBe("unknown");
  });
});
