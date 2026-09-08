/* oxlint-disable no-console, no-await-in-loop -- CLI script; bots are attached one at a time on purpose */
// Read-only measurement of what a fresh attach paints for every bot on a live
// gravityd: replays each ring into a headless xterm and reports the state the
// desktop would be in. Used to compare Claude Code's renderer modes.
//
//   node scripts/terminal-metrics.mjs [--out file.json] [--port N]
//
// Reads the port and client token from ~/.gravity like the desktop does.
import fs from "node:fs";
import os from "node:os";
import headless from "@xterm/headless";

const { Terminal } = headless;

const COLS = 180;
const ROWS = 50;
const ALT_SCREEN_ON = "\x1b[?1049h";

const args = process.argv.slice(2);
const flag = (name) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};
const home = process.env.GRAVITY_HOME ?? `${os.homedir()}/.gravity`;
const port = Number(flag("--port") ?? fs.readFileSync(`${home}/gravityd.port`, "utf8").trim());
const token = fs.readFileSync(`${home}/secrets/client.token`, "utf8").trim();
const outPath = flag("--out");

const ws = new WebSocket(`ws://127.0.0.1:${port}/ws`);
let nextId = 1;
const pending = new Map();
const frames = new Map(); // bot_id -> [{seq, data}]

// fallow-ignore-next-line complexity
ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (typeof message.req_id === "string" && pending.has(message.req_id)) {
    pending.get(message.req_id)(message);
    pending.delete(message.req_id);
    return;
  }
  if (message.type === "term") {
    if (!frames.has(message.bot_id)) {
      frames.set(message.bot_id, []);
    }
    frames.get(message.bot_id).push({ seq: message.seq, data: message.data });
  }
});

function request(body) {
  return new Promise((resolve) => {
    const id = String(nextId++);
    pending.set(id, resolve);
    ws.send(JSON.stringify({ ...body, req_id: id }));
  });
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// fallow-ignore-next-line complexity
async function replayOf(botId) {
  const attached = await request({ type: "attach", bot_id: botId });
  const target = attached.seq;
  const started = performance.now();
  for (;;) {
    await sleep(50);
    const got = frames.get(botId) ?? [];
    const last = got.length > 0 ? got[got.length - 1].seq : 0;
    if (target === 0 || last >= target || performance.now() - started > 10_000) {
      break;
    }
  }
  await request({ type: "detach", bot_id: botId });
  return { seq: target, frames: frames.get(botId) ?? [] };
}

function write(term, data) {
  return new Promise((resolve) => {
    term.write(data, resolve);
  });
}

function nonBlankRows(buffer, rows) {
  let count = 0;
  for (let row = buffer.viewportY; row < buffer.viewportY + rows; row += 1) {
    const line = buffer.getLine(row);
    if (line !== undefined && line.translateToString(true).trim().length > 0) {
      count += 1;
    }
  }
  return count;
}

async function measure(bot) {
  const { frames: replay } = await replayOf(bot.id);
  const data = replay.map((frame) => frame.data).join("");
  const term = new Terminal({ cols: COLS, rows: ROWS, scrollback: 10_000, allowProposedApi: true });
  await write(term, data);
  const active = term.buffer.active;
  const mouse = term.modes.mouseTrackingMode;
  const altScreen = active.type === "alternate";
  const scrollbackLines = Math.max(0, term.buffer.normal.length - ROWS);
  let wheel;
  if (mouse !== "none") {
    wheel = "sent to Claude Code";
  } else if (altScreen) {
    wheel = "arrow keys";
  } else {
    wheel = "xterm scrollback";
  }
  return {
    name: bot.name,
    state: bot.state,
    replayBytes: data.length,
    replayFrames: replay.length,
    entersAltScreen: data.includes(ALT_SCREEN_ON),
    altScreenActive: altScreen,
    mouseTracking: mouse,
    wheel,
    scrollbackLines,
    visibleRows: nonBlankRows(active, ROWS),
    normalBufferRows: nonBlankRows(term.buffer.normal, ROWS),
  };
}

function pad(value, width, right = false) {
  const text = String(value);
  return right ? text.padStart(width) : text.padEnd(width);
}

// fallow-ignore-next-line complexity
ws.addEventListener("open", async () => {
  const hello = await request({
    type: "hello",
    protocol_version: 2,
    token,
    client: "terminal-metrics/0",
  });
  if (hello.type !== "hello_ok") {
    console.error("hello failed:", hello);
    process.exit(1);
  }
  const { bots } = await request({ type: "list_bots" });
  const live = bots.filter((bot) => bot.state !== "archived");
  const rows = [];
  for (const bot of live) {
    rows.push(await measure(bot));
  }
  rows.sort((a, b) => a.name.localeCompare(b.name));

  console.log(
    `gravityd ${hello.server_version} · ${rows.length} bots · headless xterm ${COLS}x${ROWS}\n`,
  );
  console.log(
    `${pad("bot", 16)} ${pad("alt screen", 10)} ${pad("mouse", 6)} ${pad("wheel", 20)} ${pad("scrollback", 10, true)} ${pad("visible", 8, true)} ${pad("normal buf", 10, true)} ${pad("replay KB", 10, true)}`,
  );
  for (const row of rows) {
    console.log(
      `${pad(row.name, 16)} ${pad(row.altScreenActive ? "yes" : "no", 10)} ${pad(row.mouseTracking, 6)} ${pad(row.wheel, 20)} ${pad(row.scrollbackLines, 10, true)} ${pad(row.visibleRows, 8, true)} ${pad(row.normalBufferRows, 10, true)} ${pad((row.replayBytes / 1024).toFixed(0), 10, true)}`,
    );
  }
  const summary = {
    bots: rows.length,
    altScreen: rows.filter((row) => row.altScreenActive).length,
    mouseTracking: rows.filter((row) => row.mouseTracking !== "none").length,
    wheelScrollsXterm: rows.filter((row) => row.wheel === "xterm scrollback").length,
    blankNormalBuffer: rows.filter((row) => row.normalBufferRows <= 1).length,
    meanScrollbackLines: Math.round(
      rows.reduce((sum, row) => sum + row.scrollbackLines, 0) / rows.length,
    ),
    meanVisibleRows: Math.round(rows.reduce((sum, row) => sum + row.visibleRows, 0) / rows.length),
  };
  console.log("\nsummary:", JSON.stringify(summary));
  if (outPath !== undefined) {
    fs.writeFileSync(
      outPath,
      JSON.stringify(
        { at: new Date().toISOString(), server: hello.server_version, summary, bots: rows },
        null,
        2,
      ),
    );
    console.log(`written ${outPath}`);
  }
  process.exit(0);
});
