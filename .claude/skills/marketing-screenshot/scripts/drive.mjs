// Drives a sandbox gravityd over its WS control plane while staging a
// marketing screenshot. Talks to the throwaway daemon only — never the
// installed one on 49777.
//
//   node drive.mjs list
//   node drive.mjs project <name>                  -> project id
//   node drive.mjs bot <projectId> <name> <description> <instructions> [avatar]
//   node drive.mjs say <botId> <text>              # type into a bot's terminal
//   node drive.mjs peek <botId>                    # dump a bot's screen
//   node drive.mjs update <botId> <description> <instructions>
//   node drive.mjs delete <botId>
//
// Env: GRAVITY_WS (default ws://127.0.0.1:49888/ws),
//      GRAVITY_HOME (required; use a fresh disposable home).

import { readFileSync } from "node:fs";
import { localDaemon } from "../../lib/local-daemon.mjs";

const daemon = localDaemon(
  process.env.GRAVITY_HOME,
  process.env.GRAVITY_WS ?? "ws://127.0.0.1:49888/ws",
);
const token = readFileSync(`${daemon.home}/secrets/client.token`, "utf8").trim();
const ws = new WebSocket(daemon.endpoint);
const watchdog = setTimeout(() => {
  console.error("driver timed out");
  process.exit(1);
}, 360_000);
let nextReq = 1;
const pending = new Map();
const states = new Map();
const term = [];

ws.onmessage = (ev) => {
  const v = JSON.parse(ev.data);
  if (v.type === "bot_state") {
    states.set(v.bot_id, v.state);
  }
  // `term.data` is plain UTF-8, not base64.
  if (v.type === "term") {
    term.push(v.data);
  }
  if (v.req_id && pending.has(v.req_id)) {
    pending.get(v.req_id)(v);
    pending.delete(v.req_id);
  }
};

const request = (body) => {
  const req_id = String(nextReq++);
  return new Promise((resolve) => {
    pending.set(req_id, resolve);
    ws.send(JSON.stringify({ ...body, req_id }));
  });
};
const fire = (body) => ws.send(JSON.stringify(body));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitReady(id, seconds = 300) {
  for (let i = 0; i < seconds * 2; i++) {
    if (states.get(id) === "ready") {
      return true;
    }
    await sleep(500);
  }
  throw new Error(`bot ${id} never became ready`);
}

await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});
const hello = await request({
  type: "hello",
  protocol_version: 2,
  token,
  client: "shot-driver/0",
});
if (hello.type !== "hello_ok") {
  throw new Error("handshake failed");
}

const [, , mode, ...rest] = process.argv;

if (mode === "list") {
  const projects = await request({ type: "list_projects" });
  for (const project of projects.projects ?? []) {
    console.log(`project ${project.id}  ${project.name}`);
    const bots = await request({ type: "list_bots", project_id: project.id });
    for (const bot of bots.bots ?? []) {
      console.log(`  bot ${bot.id}  ${bot.name.padEnd(12)} ${bot.state}`);
    }
  }
} else if (mode === "project") {
  const res = await request({ type: "create_project", name: rest[0] });
  console.log(res.project?.id ?? JSON.stringify(res));
} else if (mode === "bot") {
  const [project_id, name, description, instructions, avatar] = rest;
  const res = await request({
    type: "create_bot",
    project_id,
    name,
    description,
    instructions,
    ...(avatar ? { avatar } : {}),
  });
  if (!res.bot) {
    throw new Error("create_bot failed");
  }
  console.log(`${res.bot.id}  ${res.bot.workspace_path}`);
  console.log(`ready: ${await waitReady(res.bot.id)}`);
} else if (mode === "say") {
  // Claude Code's TUI treats a large single write as a paste, so the newline
  // that submits it has to arrive as its own write a beat later.
  fire({ type: "input", bot_id: rest[0], data: rest[1] });
  await sleep(600);
  fire({ type: "input", bot_id: rest[0], data: "\r" });
  await sleep(600);
} else if (mode === "peek") {
  fire({ type: "attach", bot_id: rest[0] });
  await sleep(3000);
  // ANSI escape sequences are intentionally stripped from terminal output.
  /* eslint-disable no-control-regex */
  console.log(
    term
      .join("")
      .slice(-4000)
      .replace(/\x1b\[[0-9;?]*[a-zA-Z]/g, "")
      .replace(/\x1b\][^\x07]*\x07/g, ""),
  );
  /* eslint-enable no-control-regex */
} else if (mode === "update") {
  const res = await request({
    type: "update_bot",
    bot_id: rest[0],
    description: rest[1],
    instructions: rest[2],
  });
  console.log(res.type, res.message ?? "");
} else if (mode === "delete") {
  const res = await request({ type: "delete_bot", bot_id: rest[0], reason: "screenshot reset" });
  console.log(res.type, res.message ?? "");
} else {
  console.error("unknown mode; see the header for usage");
  process.exitCode = 1;
}

clearTimeout(watchdog);
ws.close();
process.exit(process.exitCode ?? 0);
