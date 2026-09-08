#!/usr/bin/env node
// Control-plane driver for a local gravityd: create projects/bots and speak
// as the user, over the same WebSocket protocol the desktop app uses.
// Requires Node 22+ (global WebSocket).
//
//   node bus.mjs setup <home> <port> <name[:instructions]> ...
//   node bus.mjs chat  <home> <port> <bot-id> <body>
//   node bus.mjs mcp   <home> <port> <bot-id> <tool> <json-arguments>
//
// `setup` prints {project, bots: {name: id}} as JSON. Bot MCP tokens are on
// disk at <home>/secrets/bot-<id>.token.

import fs from "node:fs";
import { localDaemon } from "../../lib/local-daemon.mjs";

const [cmd, home, port, ...rest] = process.argv.slice(2);
if (!cmd || !home || !port) {
  console.error("usage: bus.mjs setup|chat|mcp <home> <port> ...");
  process.exit(2);
}

const daemon = localDaemon(home, `ws://127.0.0.1:${port}/ws`);
if (cmd === "mcp") {
  const [botId, tool, rawArguments] = rest;
  if (
    !botId ||
    !/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(botId) ||
    !tool ||
    !rawArguments
  ) {
    throw new Error("mcp requires a bot UUID, tool name, and JSON arguments");
  }
  const args = JSON.parse(rawArguments);
  const botToken = fs.readFileSync(`${daemon.home}/secrets/bot-${botId}.token`, "utf8").trim();
  const response = await fetch(`http://127.0.0.1:${port}/mcp`, {
    method: "POST",
    redirect: "error",
    signal: AbortSignal.timeout(10_000),
    headers: { Authorization: `Bearer ${botToken}`, "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "tools/call",
      params: { name: tool, arguments: args },
    }),
  });
  if (!response.ok) {
    throw new Error(`MCP request failed (HTTP ${response.status})`);
  }
  console.log(JSON.stringify(await response.json()));
  process.exit(0);
}
const token = fs.readFileSync(`${daemon.home}/secrets/client.token`, "utf8").trim();
const ws = new WebSocket(daemon.endpoint);
const watchdog = setTimeout(() => {
  console.error("driver timed out");
  process.exit(1);
}, 30_000);
let id = 0;
const pending = new Map();
const req = (msg) =>
  new Promise((res) => {
    const req_id = String(++id);
    pending.set(req_id, res);
    ws.send(JSON.stringify({ ...msg, req_id }));
  });
ws.onmessage = (e) => {
  const v = JSON.parse(e.data);
  if (v.req_id && pending.has(v.req_id)) {
    pending.get(v.req_id)(v);
    pending.delete(v.req_id);
  }
};
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});
const hello = await req({ type: "hello", protocol_version: 2, token, client: "bus-live-test/0" });
if (hello.type !== "hello_ok") {
  console.error("handshake failed");
  process.exit(1);
}

if (cmd === "setup") {
  const proj = await req({ type: "create_project", name: "bustest" });
  const out = { project: proj.project.id, bots: {} };
  for (const spec of rest) {
    const separator = spec.indexOf(":");
    const name = separator < 0 ? spec : spec.slice(0, separator);
    const instructions = separator < 0 ? "" : spec.slice(separator + 1);
    const b = await req({
      type: "create_bot",
      project_id: out.project,
      name,
      description: name,
      instructions,
    });
    if (b.type !== "bot") {
      console.error("create_bot failed", JSON.stringify(b));
      process.exit(1);
    }
    out.bots[name] = b.bot.id;
  }
  console.log(JSON.stringify(out));
} else if (cmd === "chat") {
  const [botId, body] = rest;
  const r = await req({ type: "send_user_message", to_bot_id: botId, body });
  console.log(JSON.stringify(r.message ? { sent: r.message.id } : r));
} else {
  console.error(`unknown command '${cmd}'`);
  process.exit(2);
}
clearTimeout(watchdog);
ws.close();
