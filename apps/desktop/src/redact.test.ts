import { describe, expect, it } from "vitest";
import { redact, redactTail } from "./redact";

describe("redact", () => {
  it("replaces the home directory but keeps the rest of the path", () => {
    expect(redact("cannot read /Users/alice/.gravity/gravityd.toml", 500)).toBe(
      "cannot read /Users/~/.gravity/gravityd.toml",
    );
  });

  it("replaces daemon tokens, which are 32 hex-encoded bytes", () => {
    expect(redact(`auth failed for ${"9f".repeat(32)}`, 500)).toBe("auth failed for [redacted]");
  });

  it("keeps ids that are worth correlating on", () => {
    const id = "018f2c1e-4c3a-7b6d-9e21-5a7f3b2c8d10";
    expect(redact(`bot ${id} is archived`, 500)).toBe(`bot ${id} is archived`);
  });

  it("keeps the start when clamping a message", () => {
    expect(redact("abcdef", 3)).toBe("abc…");
  });

  it("keeps the end when clamping a log tail, where the last lines matter", () => {
    expect(redactTail("abcdef", 3)).toBe("…def");
  });

  it("leaves text within the limit untouched", () => {
    expect(redact("short", 500)).toBe("short");
    expect(redactTail("short", 500)).toBe("short");
  });
});
