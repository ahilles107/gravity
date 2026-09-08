import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { localDaemon } from "./local-daemon.mjs";

test("reject unsafe endpoints before reading any home or credentials", () => {
  for (const endpoint of [
    "ws://example.org:49555/ws",
    "wss://127.0.0.1:49555/ws",
    "ws://127.0.0.1:49777/ws",
    "ws://user:password@127.0.0.1:49555/ws",
    "ws://127.0.0.1:49555/ws?token=example",
    "ws://127.0.0.1:49555/other",
  ]) {
    assert.throws(() => localDaemon(undefined, endpoint), /test-port/);
  }
});

test("require an explicit home and match its published port", (context) => {
  assert.throws(() => localDaemon(undefined, "ws://127.0.0.1:49555/ws"), /GRAVITY_HOME/);
  const home = mkdtempSync(join(tmpdir(), "gravity-skill-test-"));
  context.after(() => rmSync(home, { recursive: true }));
  writeFileSync(join(home, "gravityd.port"), "49555\n");
  assert.equal(localDaemon(home, "ws://127.0.0.1:49555/ws").endpoint.port, "49555");
  assert.throws(() => localDaemon(home, "ws://127.0.0.1:49888/ws"), /published port/);
});
