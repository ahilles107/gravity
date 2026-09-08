import { describe, expect, it } from "vitest";
import { isReply, parseServerMessage, replyIs } from "./wire";

describe("parseServerMessage", () => {
  it("rejects malformed JSON", () => {
    expect(parseServerMessage("{")).toBeNull();
  });

  it("rejects frames without a known type", () => {
    expect(parseServerMessage(JSON.stringify({ type: "nope", req_id: "1" }))).toBeNull();
    expect(parseServerMessage(JSON.stringify({ req_id: "1" }))).toBeNull();
  });

  it("rejects replies without a string req_id", () => {
    expect(parseServerMessage(JSON.stringify({ type: "ok" }))).toBeNull();
    expect(parseServerMessage(JSON.stringify({ type: "ok", req_id: 1 }))).toBeNull();
  });

  it("accepts pushes, which carry no req_id", () => {
    const raw = JSON.stringify({
      type: "approval_pending",
      bot_id: "bot-1",
      detail: "why",
    });
    const message = parseServerMessage(raw);
    expect(message).not.toBeNull();
    expect(message === null ? true : isReply(message)).toBe(false);
  });

  it("accepts every reply type the daemon can send", () => {
    // A reply this guard rejects never settles its pending request, so the
    // caller hangs; these are the types added after the guard was written.
    for (const type of ["bot_activity", "bot_revisions"]) {
      expect(parseServerMessage(JSON.stringify({ type, req_id: "1" }))).not.toBeNull();
    }
  });

  it("accepts replies and narrows them by type", () => {
    const raw = JSON.stringify({
      type: "attached",
      req_id: "7",
      bot_id: "b",
      seq: 3,
    });
    const message = parseServerMessage(raw);
    if (message === null || !isReply(message)) {
      throw new Error("expected a reply");
    }
    expect(replyIs(message, "ok")).toBe(false);
    if (!replyIs(message, "attached")) {
      throw new Error("expected an attached reply");
    }
    expect(message.seq).toBe(3);
  });
});
