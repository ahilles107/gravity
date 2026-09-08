#!/usr/bin/env node
// Control-plane driver for a local gravityd: create projects/bots and speak
// as the user, over the same WebSocket protocol the desktop app uses.
// Requires Node 22+ (global WebSocket).
//
//   node bus.mjs setup <home> <port> <name[:instructions]> ...
//   node bus.mjs chat  <home> <port> <bot-id> <body>
//
// `setup` prints {project, bots: {name: id}} as JSON. Bot MCP tokens are on
// disk at <home>/secrets/bot-<id>.token.

import fs from 'fs';

const [cmd, home, port, ...rest] = process.argv.slice(2);
if (!cmd || !home || !port) {
  console.error('usage: bus.mjs setup|chat <home> <port> ...');
  process.exit(2);
}

const token = fs.readFileSync(`${home}/secrets/client.token`, 'utf8').trim();
const ws = new WebSocket(`ws://127.0.0.1:${port}/ws`);
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
await new Promise((r) => (ws.onopen = r));
const hello = await req({ type: 'hello', protocol_version: 2, token, client: 'bus-live-test/0' });
if (hello.type !== 'hello_ok') {
  console.error('handshake failed', JSON.stringify(hello));
  process.exit(1);
}

if (cmd === 'setup') {
  const proj = await req({ type: 'create_project', name: 'bustest' });
  const out = { project: proj.project.id, bots: {} };
  for (const spec of rest) {
    const [name, instructions = ''] = spec.split(':');
    const b = await req({
      type: 'create_bot',
      project_id: out.project,
      name,
      description: name,
      instructions,
    });
    if (b.type !== 'bot') {
      console.error('create_bot failed', JSON.stringify(b));
      process.exit(1);
    }
    out.bots[name] = b.bot.id;
  }
  console.log(JSON.stringify(out));
} else if (cmd === 'chat') {
  const [botId, body] = rest;
  const r = await req({ type: 'send_user_message', to_bot_id: botId, body });
  console.log(JSON.stringify(r.message ? { sent: r.message.id } : r));
} else {
  console.error(`unknown command '${cmd}'`);
  process.exit(2);
}
ws.close();
