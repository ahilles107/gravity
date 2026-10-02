// Small local JSON WebSocket fixture; no external service or dependencies.
import { createServer } from "node:http";
import { createHash } from "node:crypto";
import { createConnection } from "node:net";

export function listen(address, verifier, receive) {
  const peers = new Set();
  const server = createServer();
  server.on("upgrade", (request, socket) => {
    const token = request.headers.authorization?.replace(/^Bearer /, "") ?? "";
    if (createHash("sha256").update(token).digest("hex") !== verifier) {
      socket.end("HTTP/1.1 401 Unauthorized\r\n\r\n");
      return;
    }
    const accept = createHash("sha1").update(request.headers["sec-websocket-key"] + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").digest("base64");
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
    const send = (value) => {
      const body = Buffer.from(JSON.stringify(value));
      const header = Buffer.alloc(body.length < 126 ? 2 : 4);
      header[0] = 0x81;
      header[1] = body.length < 126 ? body.length : 126;
      if (body.length >= 126) { header.writeUInt16BE(body.length, 2); }
      socket.write(Buffer.concat([header, body]));
    };
    peers.add(send);
    socket.on("close", () => { peers.delete(send); });
    socket.on("error", () => { peers.delete(send); });
    let pending = Buffer.alloc(0);
    socket.on("data", (bytes) => {
      pending = Buffer.concat([pending, bytes]);
      while (pending.length >= 2) {
        const opcode = pending[0] & 15;
        let size = pending[1] & 127;
        let offset = 2;
        if (size === 126) {
          if (pending.length < 4) { return; }
          size = pending.readUInt16BE(2); offset = 4;
        }
        if (size === 127) { throw new Error("fixture message too large"); }
        const masked = Boolean(pending[1] & 128);
        const start = offset + (masked ? 4 : 0);
        if (pending.length < start + size) { return; }
        const body = Buffer.from(pending.subarray(start, start + size));
        if (masked) {
          for (let i = 0; i < size; i++) { body[i] ^= pending[offset + i % 4]; }
        }
        pending = pending.subarray(start + size);
        if (opcode === 8) { socket.end(); return; }
        if (opcode === 1) { receive(JSON.parse(body.toString()), send); }
      }
    });
  });
  server.listen(Number(new URL(address).port), "127.0.0.1");
  return (value) => { for (const send of peers) { send(value); } };
}

export async function terminal(address, token) {
  const endpoint = new URL(address);
  const socket = createConnection(Number(endpoint.port), "127.0.0.1");
  await new Promise((resolve, reject) => {
    socket.once("connect", resolve);
    socket.once("error", reject);
  });
  socket.write(`GET / HTTP/1.1\r\nHost: ${endpoint.host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: Z3Jhdml0eS10ZXN0LWtleQ==\r\nSec-WebSocket-Version: 13\r\nAuthorization: Bearer ${token}\r\n\r\n`);
  await new Promise((resolve, reject) => {
    socket.once("data", (bytes) => {
      if (bytes.toString().startsWith("HTTP/1.1 101")) { resolve(); }
      else { reject(new Error("WebSocket upgrade failed")); }
    });
  });
  const write = (value) => {
    const body = Buffer.from(JSON.stringify(value));
    const header = Buffer.alloc(body.length < 126 ? 6 : 8);
    header[0] = 0x81; header[1] = 128 | (body.length < 126 ? body.length : 126);
    if (body.length >= 126) { header.writeUInt16BE(body.length, 2); }
    // A zero mask is legal; these test messages contain no credentials.
    socket.write(Buffer.concat([header, body]));
  };
  let serial = 100;
  const send = (method, params) => { write({ id: serial++, method, params }); };
  send("initialize", { clientInfo: { name: "fixture-tui" } });
  send("thread/resume", { threadId: "thread-fixture" });
  process.stdout.write("\x1b[32mNative Codex CLI ready\x1b[0m\r\n› ");
  process.stdin.setRawMode(true);
  let draft = [];
  let approval;
  let pending = Buffer.alloc(0);
  const receive = (message) => {
    if (message.method === "item/agentMessage/delta") { process.stdout.write(message.params.delta); }
    if (message.method === "item/commandExecution/requestApproval") {
      approval = message.id;
      process.stdout.write("\r\nApprove command? y/n\r\n");
    }
  };
  socket.on("data", (bytes) => {
    pending = Buffer.concat([pending, bytes]);
    while (pending.length >= 2) {
      let size = pending[1] & 127;
      let offset = 2;
      if (size === 126) {
        if (pending.length < 4) { return; }
        size = pending.readUInt16BE(2); offset = 4;
      }
      if (pending.length < offset + size) { return; }
      receive(JSON.parse(pending.subarray(offset, offset + size).toString()));
      pending = pending.subarray(offset + size);
    }
  });
  process.stdin.on("data", (bytes) => {
    for (const byte of bytes) {
      if (approval && (byte === 110 || byte === 121)) {
        write({ id: approval, result: { decision: byte === 110 ? "decline" : "accept" } });
        approval = undefined;
      } else if (byte === 3) { send("turn/interrupt", { threadId: "thread-fixture", turnId: "turn-fixture" }); }
      else if (byte === 13) {
        const text = Buffer.from(draft).toString();
        if (text === "/exit") { process.exit(0); }
        send("turn/start", { threadId: "thread-fixture", input: [{ type: "text", text }] });
        draft = [];
      } else { draft.push(byte); }
    }
  });
}
