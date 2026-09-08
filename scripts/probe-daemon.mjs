// Read-only probe of the live gravityd control plane: times list requests and
// measures what an uncursored `attach` replays per bot.
import fs from "node:fs";
import os from "node:os";

const port = Number(process.argv[2] ?? fs.readFileSync(`${os.homedir()}/.gravity/gravityd.port`, "utf8").trim());
const token = fs.readFileSync(`${os.homedir()}/.gravity/secrets/client.token`, "utf8").trim();
const ws = new WebSocket(`ws://127.0.0.1:${port}/ws`);

let nextId = 1;
const pending = new Map();
const term = new Map(); // bot_id -> stats
let parseMs = 0;
let frames = 0;

function stat(botId) {
  let s = term.get(botId);
  if (!s) {
    s = { frames: 0, data: 0, wire: 0, first: 0, last: 0, min: Infinity, max: 0, sizes: [], lastSeq: 0 };
    term.set(botId, s);
  }
  return s;
}

ws.addEventListener("message", (ev) => {
  const raw = ev.data;
  const t0 = performance.now();
  const m = JSON.parse(raw);
  parseMs += performance.now() - t0;
  frames += 1;
  if (typeof m.req_id === "string" && pending.has(m.req_id)) {
    const p = pending.get(m.req_id);
    pending.delete(m.req_id);
    p.resolve({ reply: m, ms: performance.now() - p.t0, wire: raw.length });
    return;
  }
  if (m.type === "term") {
    const s = stat(m.bot_id);
    const now = performance.now();
    if (s.frames === 0) s.first = now;
    s.last = now;
    s.frames += 1;
    s.data += m.data.length;
    s.wire += raw.length;
    s.min = Math.min(s.min, m.data.length);
    s.max = Math.max(s.max, m.data.length);
    s.sizes.push(m.data.length);
    s.lastSeq = m.seq;
  }
});

function request(body) {
  return new Promise((resolve) => {
    const id = String(nextId++);
    pending.set(id, { resolve, t0: performance.now() });
    ws.send(JSON.stringify({ ...body, req_id: id }));
  });
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function timed(label, body) {
  const { reply, ms, wire } = await request(body);
  const count = Array.isArray(reply[label]) ? reply[label].length : "";
  console.log(`${body.type.padEnd(20)} ${ms.toFixed(1).padStart(8)} ms  ${String(wire).padStart(8)} B wire  ${count}`);
  return reply;
}

async function waitReplay(botId, target) {
  const s = stat(botId);
  const start = performance.now();
  // Done when the replay cursor is reached, or after 1.5 s of silence.
  for (;;) {
    await sleep(50);
    if (target > 0 && s.lastSeq >= target) break;
    if (performance.now() - (s.last || start) > 1500) break;
    if (performance.now() - start > 20000) break;
  }
}

function median(a) {
  if (a.length === 0) return 0;
  const b = [...a].sort((x, y) => x - y);
  return b[Math.floor(b.length / 2)];
}

await new Promise((resolve) => ws.addEventListener("open", resolve));
const hello = await request({ type: "hello", protocol_version: 2, token, client: "probe/0" });
console.log("hello:", hello.reply.type, hello.reply.server_version, `${hello.ms.toFixed(1)} ms`);

console.log("\n--- request latency (cold, then warm) ---");
const bots = (await timed("bots", { type: "list_bots" })).bots;
await timed("projects", { type: "list_projects" });
await timed("conversations", { type: "list_conversations" });
await timed("routines", { type: "list_routines" });
await timed("deliveries", { type: "list_deliveries", state: "failed" });
await timed("activity", { type: "list_bot_activity" });
await timed("activity", { type: "list_bot_activity" });
await timed("activity", { type: "list_bot_activity" });
await timed("bots", { type: "list_bots" });
await timed("search_results", { type: "search", query: "the" });

console.log(`\n--- attach replay per bot (${bots.length} bots) ---`);
console.log("name".padEnd(18), "state".padEnd(8), "attach ms".padStart(9), "frames".padStart(7), "data KB".padStart(8), "wire KB".padStart(8), "min".padStart(5), "med".padStart(5), "max".padStart(6), "replay ms".padStart(10), "resumed");
let totalFrames = 0;
let totalWire = 0;
let totalReplayMs = 0;
for (const bot of bots) {
  const { reply, ms } = await request({ type: "attach", bot_id: bot.id });
  await waitReplay(bot.id, reply.seq);
  const s = stat(bot.id);
  const replayMs = s.frames > 0 ? s.last - s.first : 0;
  totalFrames += s.frames;
  totalWire += s.wire;
  totalReplayMs += replayMs;
  console.log(
    bot.name.slice(0, 18).padEnd(18),
    String(bot.state).padEnd(8),
    ms.toFixed(1).padStart(9),
    String(s.frames).padStart(7),
    (s.data / 1024).toFixed(0).padStart(8),
    (s.wire / 1024).toFixed(0).padStart(8),
    String(s.min === Infinity ? 0 : s.min).padStart(5),
    String(median(s.sizes)).padStart(5),
    String(s.max).padStart(6),
    replayMs.toFixed(0).padStart(10),
    reply.resumed,
  );
  await request({ type: "detach", bot_id: bot.id });
}
console.log(`\ntotal: ${totalFrames} term frames, ${(totalWire / 1024 / 1024).toFixed(2)} MB wire, JSON.parse total ${parseMs.toFixed(0)} ms over ${frames} messages`);
ws.close();
