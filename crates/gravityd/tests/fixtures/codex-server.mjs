import { appendFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { listen, terminal } from "./codex-websocket.mjs";

const log = process.env.GRAVITY_TEST_LOG ?? "../codex-fixture-rpc.jsonl";
let send = (value) => { process.stdout.write(`${JSON.stringify(value)}\n`); };
const notify = (method, params) => { send({ method, params }); };
let active = false;
function receive(request, respond) {
  const line = JSON.stringify(request);
  const reply = (id, result) => { respond({ id, result }); };
  appendFileSync(log, `${line}\n`);
  const { id, method, params } = request;
  if (method === "initialize") { reply(id, { userAgent: "fixture" }); }
  else if (method === "initialized") { /* notification */ }
  else if (method === "thread/name/set") { reply(id, {}); }
  else if (method === "thread/start" || method === "thread/resume" || method === "thread/read") {
    reply(id, { thread: { id: "thread-fixture" } });
  } else if (method === "turn/start") {
    const text = params.input[0].text;
    active = true;
    notify("turn/started", { turn: { id: "turn-fixture", status: "inProgress" } });
    notify("item/completed", { item: { type: "userMessage", content: [{ type: "text", text }] } });
    reply(id, { turn: { id: "turn-fixture", status: "inProgress" } });
    if (text === "approval") {
      send({ id: "approval-1", method: "item/commandExecution/requestApproval", params: { command: "echo hello", threadId: "thread-fixture", turnId: "turn-fixture" } });
    } else if (text === "tools") {
      const change = { id: "fc-1", type: "fileChange", status: "completed",
        changes: [{ path: "/w/src/update.rs", kind: { type: "update", move_path: null },
          diff: "@@ -1,1 +1,2 @@\n-old\n+new\n+more\n" }] };
      notify("item/started", { item: change });
      notify("item/completed", { item: { id: "cmd-1", type: "commandExecution", command: "cargo test",
        cwd: "/w", aggregatedOutput: "test result: ok", exitCode: 0, status: "completed" } });
      notify("item/completed", { item: change });
      notify("item/completed", { item: { id: "mcp-1", type: "mcpToolCall", server: "gravity-bus",
        tool: "send_message", status: "completed", arguments: { to: "lead", kind: "note", body: "built" },
        result: { content: [{ type: "text", text: "{}" }] }, error: null } });
      notify("item/completed", { item: { type: "agentMessage", text: "Built it." } });
      active = false;
      notify("turn/completed", { turn: { id: "turn-fixture", status: "completed" } });
    } else if (text === "crash") { process.exit(7); }
    else if (text !== "hold") {
      notify("item/agentMessage/delta", { delta: "Hello 🪟" });
      notify("item/completed", { item: { type: "agentMessage", text: "Hello 🪟" } });
      active = false;
      notify("turn/completed", { turn: { id: "turn-fixture", status: "completed" } });
    }
  } else if (method === "turn/steer") { reply(id, { turnId: "turn-fixture" }); }
  else if (method === "turn/interrupt") {
    reply(id, {});
    active = false;
    notify("turn/completed", { turn: { id: "turn-fixture", status: "interrupted" } });
  } else if (id === "approval-1" && request.result) {
    if (!active) { throw new Error("approval after completed turn"); }
    // Every client hears that the request is settled, as the real server says.
    notify("serverRequest/resolved", { threadId: "thread-fixture", requestId: "approval-1" });
    active = false;
    notify("turn/completed", { turn: { id: "turn-fixture", status: "completed" } });
  } else { throw new Error(`Unexpected request ${line}`); }
}

const args = process.argv.slice(2);
if (args.includes("--remote")) {
  await terminal(args[args.indexOf("--remote") + 1], process.env.GRAVITY_CODEX_REMOTE_TOKEN);
} else if (args.includes("--ws-auth")) {
  send = listen(args[args.indexOf("--listen") + 1], args[args.indexOf("--ws-token-sha256") + 1], receive);
} else {
  for await (const line of createInterface({ input: process.stdin })) { receive(JSON.parse(line), send); }
}
