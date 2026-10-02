import { describe, expect, it, vi } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import { typeIntoTerminal, uploadAttachment } from "./upload";

describe("uploadAttachment", () => {
  it("sends a large file in ordered chunks under one upload id", async () => {
    let chunks = 0;
    const client = new FakeDaemon().onRequest("write_artifact", (body) => {
      chunks += 1;
      const last = body.type === "write_artifact" && body.last;
      return {
        type: "upload",
        req_id: String(chunks),
        upload: { upload_id: "u1", ...(last ? { path: "/p/artifacts/uploads/big.bin" } : {}) },
      };
    });
    const file = new File([new Uint8Array(600 * 1024)], "big.bin");
    expect(await uploadAttachment(client, "p1", file)).toBe("/p/artifacts/uploads/big.bin");
    const sent = client.requests.map((r) => r.body);
    expect(sent).toHaveLength(2);
    expect(sent[0]).toMatchObject({ last: false });
    expect(sent[0]).not.toHaveProperty("upload_id");
    expect(sent[1]).toMatchObject({ last: true, upload_id: "u1" });
  });

  it("types a slash command into the terminal and submits it", () => {
    vi.useFakeTimers();
    const client = new FakeDaemon();
    typeIntoTerminal(client, "b1", "/compact");
    vi.advanceTimersByTime(250);
    expect(client.fired).toEqual([
      { type: "input", bot_id: "b1", data: "\u001b[200~/compact\u001b[201~" },
      { type: "input", bot_id: "b1", data: "\r" },
    ]);
    vi.useRealTimers();
  });
});
