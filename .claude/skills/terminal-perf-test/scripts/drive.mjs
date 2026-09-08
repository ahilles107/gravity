// Drives a dev gravityd over its WS control plane for terminal-rendering
// performance tests. Requires the daemon to run with `runtime = "double"`:
// the double echoes every input byte back as terminal output, and prints
// `[resize CxR]` markers for every pty resize it receives, which makes the
// forced-repaint nudge observable on screen.
//
// Usage:
//   node drive.mjs setup                      # create project + 5 bots, pump data
//   node drive.mjs stream <botId> [seconds]   # continuous live output (~800 lines/s)
//
// Environment:
//   GRAVITY_WS      WebSocket URL     (default ws://127.0.0.1:49555/ws)
//   GRAVITY_TOKEN   client token, or
//   GRAVITY_HOME    daemon home dir to read secrets/client.token from
//                (default /tmp/gravityd-uidev)

import { readFileSync } from "node:fs";

const URL = process.env.GRAVITY_WS ?? "ws://127.0.0.1:49555/ws";
const HOME = process.env.GRAVITY_HOME ?? "/tmp/gravityd-uidev";
const TOKEN = process.env.GRAVITY_TOKEN ?? readFileSync(`${HOME}/secrets/client.token`, "utf8").trim();

const ws = new WebSocket(URL);
let nextReq = 1;
const pending = new Map();
const states = new Map();

ws.onmessage = (ev) => {
  const v = JSON.parse(ev.data);
  if (v.type === "bot_state") {
    states.set(v.bot_id, v.state);
  }
  if (v.req_id && pending.has(v.req_id)) {
    pending.get(v.req_id)(v);
    pending.delete(v.req_id);
  }
};

function request(body) {
  const req_id = String(nextReq++);
  return new Promise((resolve) => {
    pending.set(req_id, resolve);
    ws.send(JSON.stringify({ ...body, req_id }));
  });
}

function fire(body) {
  ws.send(JSON.stringify(body));
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitReady(botId) {
  for (let i = 0; i < 100; i++) {
    if (states.get(botId) === "ready") return;
    await sleep(100);
  }
  throw new Error(`bot ${botId} never became ready`);
}

// A colored, line-terminated chunk that resembles heavily-styled TUI output.
const COLORS = [31, 32, 33, 34, 35, 36, 91, 92, 93, 94, 95, 96];
function chunk(botName, i) {
  let s = "";
  for (let l = 0; l < 20; l++) {
    const c = COLORS[(i + l) % COLORS.length];
    s += `\x1b[${c}m${botName} line ${i * 20 + l} \x1b[1m${"█".repeat(10)}\x1b[0m\x1b[${c}m ${"▓░▒".repeat(15)} tok=${(i * 7919 + l) % 99991}\x1b[0m\r\n`;
  }
  return s;
}

await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});
const hello = await request({
  type: "hello",
  protocol_version: 1,
  token: TOKEN,
  client: "perf-driver/0",
});
if (hello.type !== "hello_ok") throw new Error("handshake failed: " + JSON.stringify(hello));

const mode = process.argv[2] ?? "setup";

if (mode === "setup") {
  const project = await request({ type: "create_project", name: "perf" });
  const projectId = project.project.id;
  const bots = {};
  for (const name of ["turbo1", "turbo2", "turbo3", "turbo4", "turbo5"]) {
    const created = await request({ type: "create_bot", project_id: projectId, name });
    bots[name] = created.bot.id;
    console.log(`created ${name} = ${created.bot.id}`);
  }
  for (const name of Object.keys(bots)) await waitReady(bots[name]);
  console.log("all ready");

  // turbo1 gets ~2.3 MiB — well past the daemon's 1 MiB scrollback ring, so
  // a fresh attach exercises the trimmed, non-resumable replay path (the
  // historical black-screen scenario). The rest get ~300 KiB each.
  const sizes = { turbo1: 1200, turbo2: 160, turbo3: 160, turbo4: 160, turbo5: 160 };
  for (const [name, chunks] of Object.entries(sizes)) {
    for (let i = 0; i < chunks; i++) {
      fire({ type: "input", bot_id: bots[name], data: chunk(name, i) });
      if (i % 100 === 0) await sleep(20); // let the socket drain
    }
    console.log(`${name}: ~${Math.round((chunks * chunk(name, 0).length) / 1024)} KiB pumped`);
  }
  await sleep(500);
  console.log(JSON.stringify(bots));
} else if (mode === "stream") {
  const botId = process.argv[3];
  if (!botId) throw new Error("stream mode needs a bot id");
  const seconds = Number(process.argv[4] ?? 15);
  let i = 0;
  const end = Date.now() + seconds * 1000;
  while (Date.now() < end) {
    fire({ type: "input", bot_id: botId, data: chunk("live", i++) });
    await sleep(25);
  }
  console.log(`streamed ${i} chunks`);
}
ws.close();
process.exit(0);
