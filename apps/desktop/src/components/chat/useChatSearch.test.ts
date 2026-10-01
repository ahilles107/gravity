import { describe, expect, it } from "vitest";
import { findMatches } from "./useChatSearch";

describe("findMatches", () => {
  it("finds every occurrence across text nodes, case-insensitively", () => {
    const root = document.createElement("div");
    root.innerHTML = "<p>Signed the <b>MSI</b>.</p><p>msi uploaded; MSI checked</p>";
    const matches = findMatches(root, "msi");
    expect(matches).toHaveLength(3);
    expect(matches.map((range) => range.toString())).toEqual(["MSI", "msi", "MSI"]);
    expect(findMatches(root, "  ")).toEqual([]);
  });
});
