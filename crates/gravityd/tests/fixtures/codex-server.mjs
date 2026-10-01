import { appendFileSync } from "node:fs";
import { createInterface } from "node:readline";

const log = process.env.GRAVITY_TEST_LOG ?? "../codex-fixture-rpc.jsonl";
const send = (value) => { process.stdout.write(`${JSON.stringify(value)}\n`); };
const reply = (id, result) => { send({ id, result }); };
const notify = (method, params) => { send({ method, params }); };
let active = false;
for await (const line of createInterface({ input: process.stdin })) {
  const request = JSON.parse(line);
  appendFileSync(log, `${line}\n`);
  const { id, method, params } = request;
  if (method === "initialize") { reply(id, { userAgent: "fixture" }); }
  else if (method === "initialized") { /* notification */ }
  else if (method === "thread/start" || method === "thread/resume") {
    reply(id, { thread: { id: "thread-fixture" } });
  } else if (method === "turn/start") {
    const text = params.input[0].text;
    active = true;
    notify("turn/started", { turn: { id: "turn-fixture", status: "inProgress" } });
    reply(id, { turn: { id: "turn-fixture", status: "inProgress" } });
    if (text === "approval") {
      send({ id: "approval-1", method: "item/commandExecution/requestApproval", params: { command: "echo hello", threadId: "thread-fixture", turnId: "turn-fixture" } });
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
    active = false;
    notify("turn/completed", { turn: { id: "turn-fixture", status: "completed" } });
  } else { throw new Error(`Unexpected request ${line}`); }
}
