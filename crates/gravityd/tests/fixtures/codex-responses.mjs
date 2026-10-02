// Deterministic Responses provider for opt-in tests of the real Codex binary.
import { createServer } from "node:http";
import { gunzipSync } from "node:zlib";
let serial = 0;
const server = createServer(async (request, response) => {
  try {
  const chunks = [];
  for await (const chunk of request) { chunks.push(chunk); }
  let bytes = Buffer.concat(chunks);
  if (request.headers["content-encoding"] === "gzip") { bytes = gunzipSync(bytes); }
  if (!request.url.endsWith("/responses")) {
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify({ data: [] }));
    return;
  }
  const input = JSON.parse(bytes.toString());
  const users = input.input.filter((item) => item.role === "user");
  const text = users.at(-1)?.content?.map((item) => item.text ?? "").join("") ?? "";
  const called = input.input.some((item) => item.type === "function_call_output");
  const shell = (input.tools ?? []).find((tool) => ["exec_command", "shell_command"].includes(tool.name)) ?? { name: "exec_command" };
  const id = `local-${++serial}`;
  const events = [{ type: "response.created", response: { id } }];
  if (text.includes("approval") && !called && shell) {
    const args = shell.name === "exec_command" ? { cmd: "echo local-test", sandbox_permissions: "require_escalated", justification: "Local test of native approval" } : { command: "echo local-test", sandbox_permissions: "require_escalated", justification: "Local test of native approval" };
    events.push({ type: "response.output_item.done", item: { type: "function_call", call_id: "local-command", name: shell.name, arguments: JSON.stringify(args) } });
  } else {
    events.push({ type: "response.output_item.done", item: { type: "message", role: "assistant", id: "local-message", content: [{ type: "output_text", text: `Local response: ${text}` }] } });
  }
  events.push({ type: "response.completed", response: { id, usage: { input_tokens: 1, output_tokens: 1, total_tokens: 2 } } });
  response.writeHead(200, { "Content-Type": "text/event-stream" });
  response.end(events.map((event) => `event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`).join(""));
  } catch (error) {
    response.writeHead(400, { "Content-Type": "application/json" });
    response.end(JSON.stringify({ error: { message: `${request.headers["content-encoding"]}: ${error.message}` } }));
  }
});
server.listen(0, "127.0.0.1", () => { console.log(`http://127.0.0.1:${server.address().port}/v1`); });
