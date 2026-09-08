import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installFakeWebSocket } from "../test/fakeWebSocket";
import type { FakeSocket, SocketHarness } from "../test/fakeWebSocket";
import { DaemonClient } from "./client";
import { DaemonError } from "./connection";

const ENDPOINT = { host: "mini", port: 7777 };

function helloOk(reqId: string): Record<string, unknown> {
  return {
    type: "hello_ok",
    req_id: reqId,
    protocol_version: 2,
    server_version: "0.1.0",
    capabilities: ["terminal"],
    grants: ["read", "control"],
    device_id: null,
  };
}

/** Opens a socket and completes the handshake, returning the live socket. */
async function connect(client: DaemonClient, harness: SocketHarness): Promise<FakeSocket> {
  client.start();
  const socket = harness.latest();
  socket.open();
  await vi.waitFor(() => {
    expect(socket.sent).toHaveLength(1);
  });
  socket.receive(helloOk(socket.reqId(0)));
  await vi.waitFor(() => {
    expect(client.status).toBe("connected");
  });
  return socket;
}

describe("DaemonClient", () => {
  let harness: SocketHarness;
  let client: DaemonClient;

  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    harness = installFakeWebSocket();
    client = new DaemonClient(ENDPOINT, () => Promise.resolve("token"));
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("connects to the configured endpoint and reports the handshake result", async () => {
    const socket = await connect(client, harness);

    expect(socket.url).toBe("ws://mini:7777/ws");
    expect(client.capabilities).toEqual(["terminal"]);
    expect(client.serverVersion).toBe("0.1.0");
    expect(client.hasGrant("control")).toBe(true);
    expect(client.hasGrant("approve")).toBe(false);
    expect(client.deviceId).toBeNull();
  });

  it("notifies status listeners and stops after unsubscribing", async () => {
    const seen: string[] = [];
    const unsub = client.onStatus((status) => {
      seen.push(status);
    });
    await connect(client, harness);
    unsub();
    harness.latest().close();

    expect(seen).toEqual(["connecting", "connected"]);
  });

  it("resolves a request with the expected reply type", async () => {
    const socket = await connect(client, harness);
    const pending = client.request({ type: "list_projects" }, "projects");
    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(2);
    });
    socket.receive({ type: "projects", req_id: socket.reqId(1), projects: [] });

    await expect(pending).resolves.toMatchObject({ type: "projects", projects: [] });
  });

  it("rejects a request answered with an error reply", async () => {
    const socket = await connect(client, harness);
    const pending = client.request({ type: "list_projects" }, "projects");
    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(2);
    });
    socket.receive({
      type: "error",
      req_id: socket.reqId(1),
      code: "forbidden",
      message: "nope",
    });

    await expect(pending).rejects.toBeInstanceOf(DaemonError);
    await expect(pending).rejects.toThrow("nope");
  });

  it("rejects a request answered with the wrong reply type", async () => {
    const socket = await connect(client, harness);
    const pending = client.request({ type: "list_projects" }, "projects");
    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(2);
    });
    socket.receive({ type: "ok", req_id: socket.reqId(1) });

    await expect(pending).rejects.toThrow("expected projects reply, got ok");
  });

  it("rejects requests while disconnected", async () => {
    await expect(client.request({ type: "list_projects" }, "projects")).rejects.toThrow(
      "not connected to daemon",
    );
  });

  it("fails pending requests when the socket closes", async () => {
    const socket = await connect(client, harness);
    const pending = client.request({ type: "list_projects" }, "projects");
    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(2);
    });
    socket.close();

    await expect(pending).rejects.toThrow("connection closed");
    expect(client.status).toBe("disconnected");
  });

  it("dispatches pushes to subscribers and ignores unparseable frames", async () => {
    const socket = await connect(client, harness);
    const notes: string[] = [];
    const unsub = client.on("notify", (push) => {
      notes.push(push.title);
    });

    socket.receiveRaw("not json");
    socket.receive({ type: "unknown_frame" });
    socket.receive({ type: "notify", level: "info", title: "hi", body: "there" });
    expect(notes).toEqual(["hi"]);

    unsub();
    socket.receive({ type: "notify", level: "info", title: "again", body: "" });
    expect(notes).toEqual(["hi"]);
  });

  it("routes every push type", async () => {
    const socket = await connect(client, harness);
    const seen: string[] = [];
    for (const type of [
      "term",
      "bot_state",
      "message_new",
      "bot_updated",
      "delivery_update",
      "routine_run_update",
      "approval_pending",
      "notify",
    ] as const) {
      client.on(type, () => {
        seen.push(type);
      });
    }

    socket.receive({ type: "term", bot_id: "b1", seq: 4, data: "x" });
    socket.receive({ type: "bot_state", bot_id: "b1", state: "ready", reason: "", at: "now" });
    socket.receive({ type: "message_new", message: { id: "m1" } });
    // Declared in the protocol types and handled by the app, but unparsed and
    // unrouted here — so a bot created by another bot never reached the client.
    socket.receive({ type: "bot_updated", bot: { id: "b2", name: "steve" } });
    socket.receive({ type: "delivery_update", delivery: { id: "d1" } });
    socket.receive({ type: "routine_run_update", routine_run: { id: "rr1" } });
    socket.receive({ type: "approval_pending", bot_id: "b1", detail: "why" });
    socket.receive({ type: "notify", level: "info", title: "t", body: "b" });

    expect(seen).toEqual([
      "term",
      "bot_state",
      "message_new",
      "bot_updated",
      "delivery_update",
      "routine_run_update",
      "approval_pending",
      "notify",
    ]);
  });

  it("drops fire-and-forget frames while disconnected and sends them once connected", async () => {
    client.fire({ type: "input", bot_id: "b1", data: "x" });
    const socket = await connect(client, harness);
    client.fire({ type: "input", bot_id: "b1", data: "y" });

    expect(socket.sent).toHaveLength(2);
    expect(socket.sent[1]).toContain('"data":"y"');
  });

  it("tracks the terminal cursor from term pushes and resumes on re-attach", async () => {
    const socket = await connect(client, harness);

    const first = client.attach("b1", true);
    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(2);
    });
    expect(socket.sent[1]).not.toContain("after_seq");
    socket.receive({
      type: "attached",
      req_id: socket.reqId(1),
      bot_id: "b1",
      seq: 10,
      resumed: false,
    });
    await expect(first).resolves.toEqual({ seq: 10, resumed: false });

    socket.receive({ type: "term", bot_id: "b1", seq: 12, data: "out" });

    const second = client.attach("b1", true);
    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(3);
    });
    expect(socket.sent[2]).toContain('"after_seq":12');
    socket.receive({
      type: "attached",
      req_id: socket.reqId(2),
      bot_id: "b1",
      seq: 12,
      resumed: true,
    });
    await expect(second).resolves.toEqual({ seq: 12, resumed: true });
  });

  it("replays from the start when the caller cannot resume", async () => {
    const socket = await connect(client, harness);

    const first = client.attach("b1", true);
    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(2);
    });
    socket.receive({
      type: "attached",
      req_id: socket.reqId(1),
      bot_id: "b1",
      seq: 10,
      resumed: false,
    });
    await first;
    socket.receive({ type: "term", bot_id: "b1", seq: 12, data: "out" });

    // A fresh terminal has no screen to resume onto, so no cursor is sent even
    // though the client tracked one.
    const second = client.attach("b1", false);
    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(3);
    });
    expect(socket.sent[2]).not.toContain("after_seq");
    socket.receive({
      type: "attached",
      req_id: socket.reqId(2),
      bot_id: "b1",
      seq: 12,
      resumed: true,
    });
    await expect(second).resolves.toEqual({ seq: 12, resumed: false });
  });

  it("reports a gap the server refused to resume", async () => {
    const socket = await connect(client, harness);

    const first = client.attach("b1", true);
    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(2);
    });
    socket.receive({
      type: "attached",
      req_id: socket.reqId(1),
      bot_id: "b1",
      seq: 10,
      resumed: false,
    });
    await first;
    socket.receive({ type: "term", bot_id: "b1", seq: 12, data: "out" });

    const second = client.attach("b1", true);
    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(3);
    });
    expect(socket.sent[2]).toContain('"after_seq":12');
    // Frame 12 fell out of the ring while the client was away.
    socket.receive({
      type: "attached",
      req_id: socket.reqId(2),
      bot_id: "b1",
      seq: 40,
      resumed: false,
    });
    await expect(second).resolves.toEqual({ seq: 40, resumed: false });
  });

  it("enters auth_failed and does not report disconnected on a rejected handshake", async () => {
    client.start();
    const socket = harness.latest();
    socket.open();
    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(1);
    });
    socket.receive({
      type: "error",
      req_id: socket.reqId(0),
      code: "auth_failed",
      message: "bad token",
    });

    await vi.waitFor(() => {
      expect(client.status).toBe("auth_failed");
    });
    expect(socket.closed).toBe(true);
  });

  it("reports a daemon that speaks an older protocol as a version mismatch", async () => {
    client.start();
    const socket = harness.latest();
    socket.open();
    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(1);
    });
    socket.receive({
      type: "error",
      req_id: socket.reqId(0),
      code: "unsupported_version",
      message: "server speaks protocol 1",
    });

    await vi.waitFor(() => {
      expect(client.status).toBe("version_mismatch");
    });
    // The socket closes, but a reconnect must not overwrite the verdict with a
    // plain `disconnected` — the status is what surfaces the update prompt.
    socket.close();
    expect(client.status).toBe("version_mismatch");

    await vi.advanceTimersByTimeAsync(500);
    expect(harness.sockets).toHaveLength(2);
    expect(client.status).toBe("version_mismatch");
  });

  it("sends an empty token when reading it fails", async () => {
    const failing = new DaemonClient(ENDPOINT, () => Promise.reject(new Error("no token")));
    failing.start();
    const socket = harness.latest();
    socket.open();

    await vi.waitFor(() => {
      expect(socket.sent).toHaveLength(1);
    });
    expect(socket.sent[0]).toContain('"token":""');
  });

  it("reconnects with backoff after a close", async () => {
    await connect(client, harness);
    harness.latest().close();
    expect(harness.sockets).toHaveLength(1);

    await vi.advanceTimersByTimeAsync(500);
    expect(harness.sockets).toHaveLength(2);
  });

  it("reconnects immediately against a new endpoint", async () => {
    await connect(client, harness);
    expect(client.connectionGeneration).toBe(0);
    client.setEndpoint({ host: "other", port: 8888 });

    await vi.advanceTimersByTimeAsync(500);
    expect(harness.latest().url).toBe("ws://other:8888/ws");
    expect(client.connectionGeneration).toBe(1);
  });

  it("connects on setEndpoint when no socket is open", () => {
    client.setEndpoint({ host: "first", port: 1234 });
    expect(harness.sockets).toHaveLength(0);

    client.start();
    expect(harness.latest().url).toBe("ws://first:1234/ws");
  });

  it("only starts once", () => {
    client.start();
    client.start();
    expect(harness.sockets).toHaveLength(1);
  });
});
