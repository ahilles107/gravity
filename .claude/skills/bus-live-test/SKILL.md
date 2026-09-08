---
name: bus-live-test
description: Test gravityd's cross-session bot communication against a live daemon, in two modes — synthetic (double runtime, drive the MCP bus directly as the bots, free and deterministic) and real (pty runtime spawning billed Claude Code sessions, observing genuine bot behavior). Use when changing send_message/complete_task, the bus guardrails (reply caps, fan-out, hop limits, terminal kinds, deadlines/expiry), delivery, the system prompt's bus sections, or the artifacts flow — or when someone asks to demo, smoke-test, or observe bot-to-bot traffic on a dev daemon.
---

# Live bus / cross-session communication test

Runs a real `gravityd` on a throwaway home and exercises bot-to-bot traffic
end to end. Two modes, cheapest first:

- **Synthetic** (`runtime = "double"`): no Claude sessions; you act as the
  bots by calling the MCP bus with their bearer tokens. Deterministic, free,
  and the only mode where you can force every refusal on demand. Use for
  verifying daemon *rules*.
- **Real** (`runtime = "pty"`): the daemon spawns actual `claude` sessions —
  **billed against the user's account**. Use only with explicit user consent,
  and only to observe *behavior*: do real bots obey the prompt contracts,
  brief delegations well, use artifacts, stay silent after a `done`.

## Shared setup

```bash
cargo build -p gravityd
H=/tmp/gravityd-bustest   # throwaway home; pick a fresh path per run
rm -rf $H && mkdir -p $H
cat > $H/gravityd.toml <<EOF
home = "$H"
port = 49666                 # any free port
runtime = "double"           # or "pty" for real mode
supervision_interval_ms = 1000

[scheduler]
tick_interval_ms = 2000      # fast expiry sweeps; default is 30s
EOF
./target/debug/gravityd --config $H/gravityd.toml > $H/gravityd.log 2>&1 &
sleep 2 && curl -s http://127.0.0.1:49666/health   # expect status ok
```

For **real mode** add `claude_bin = "$(which claude)"` (absolute path — the
daemon's PATH may not include `~/.local/bin`) and set `runtime = "pty"`.

Create a project and bots with the bundled driver (Node 22+, no deps):

```bash
node .claude/skills/bus-live-test/scripts/bus.mjs setup $H 49666 \
  "lead:You coordinate work." "worker:You do small jobs."
# prints {"project":..., "bots":{"lead":"<id>","worker":"<id>"}}
```

Credentials on disk: client token at `$H/secrets/client.token`, per-bot MCP
tokens at `$H/secrets/bot-<id>.token`.

## Synthetic mode: act as the bots

Call bus tools directly with a bot's bearer token:

```bash
mcp() { # mcp <bot-id> <tool> <json-args>
  curl -s http://127.0.0.1:49666/mcp \
    -H "Authorization: Bearer $(cat $H/secrets/bot-$1.token)" \
    -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"$2\",\"arguments\":$3}}" \
  | python3 -c "import json,sys;r=json.load(sys.stdin)['result'];print(('REFUSED: ' if r.get('isError') else 'OK: ')+r['content'][0]['text'])"
}
mcp $LEAD send_message '{"to":"worker","body":"do X","kind":"task"}'
mcp $WORKER complete_task '{"task_id":"...","result":"done","artifacts":["/path"]}'
```

Guardrail checklist — force each refusal and read its hint text:

1. **Reply cap**: task, then 7 replies alternating direction → 7th refused
   with `reply limit reached … complete_task`; `complete_task` still succeeds
   past the cap.
2. **Terminal kinds**: reply after the task is done → `no open task`; reply
   with `ref` pointing at a done message id → `is a task result`; kinds
   `chat`/`done`/unknown → each refused with its hint.
3. **Fan-out**: 4th open delegation from one origin → refused; allowed again
   after one `complete_task`.
4. **Depth**: chain tasks A→B→C→D→E (needs 6 bots); the send past
   `MAX_TASK_HOPS` is refused and the target's inbox stays empty.
5. **Expiry**: backdate a deadline and watch the sweep flip it and notify
   both ends exactly once:
   ```bash
   sqlite3 $H/bus.sqlite "UPDATE task SET deadline_at='2020-01-01T00:00:00Z' WHERE id='<id>';"
   # within ~2 ticks: state='expired', WARN "task exceeded its deadline" in the log,
   # one system note per live end (assignee: stop work; requester: re-delegate)
   ```

The constants live in `crates/bus/src/types/entities.rs`
(`MAX_TASK_REPLIES`, `MAX_TASK_FANOUT`, `MAX_TASK_HOPS`,
`DEFAULT_TASK_DEADLINE_HOURS`).

## Real mode: observe billed sessions

Get explicit user consent first — every bot turn is a real Claude Code
session on the user's account. Keep runs small: each bot burns one turn on
its creation intro before you send anything, and a 4-bot
research-synthesize-review pipeline costs roughly 8–10 turns total.

Wait for boot (`active_bots` in `/health`, `turn complete` per bot in the
log), then speak as the user:

```bash
node .claude/skills/bus-live-test/scripts/bus.mjs chat $H 49666 <lead-bot-id> \
  "Delegate this to worker: ... When you get the result, read the artifact and tell me ..."
```

Good scenario shapes (each verified to exercise the guardrails):
- **Minimal**: delegate one artifact-producing job; instruct lead to *thank*
  the worker afterwards — the reply channel is closed after `done`, so a
  compliant lead uses a `note` and the worker stays silent.
- **Pipeline**: split a research question between two bots, synthesize their
  artifacts, and have a third bot review the synthesis ("you must not approve
  your own work") — exercises fan-out pacing, artifacts, author ≠ reviewer.

### Observation channels

- **Bus ledger** — the ground truth for chatter:
  ```bash
  sqlite3 $H/bus.sqlite "SELECT num,sender_name,kind,substr(replace(body,char(10),' '),1,100) FROM message ORDER BY num;"
  sqlite3 $H/bus.sqlite "SELECT substr(id,1,8),state,hop_count,reply_count,deadline_at IS NOT NULL FROM task;"
  ```
- **Daemon log** — bot state transitions, delivery, expiry warns.
- **Session transcripts** — tool-by-tool behavior including refusals the bot
  hit (`is_error` tool results). Workspaces under `/tmp` land in
  `~/.claude/projects/-private-tmp-…-workspace/*.jsonl` (note the
  `-private-` prefix: macOS `/tmp` is `/private/tmp`). Extract tool calls and
  errors from the newest `.jsonl`; a bot's answer *to the user* is its last
  assistant `text` block there.

## Pitfalls learned the hard way

- **Bot replies to the user never hit the bus.** They go to the terminal, so
  don't wait on `sender_kind='bot'` message rows to detect that intros
  finished — watch `turn complete` in the daemon log instead.
- **Delivery lags ~1s** (`delivery.poll_interval_ms`): a `check_inbox`
  immediately after a send misses the message. Sleep ≥2s or poll.
- The **`done` from `complete_task`** goes into the origin conversation and
  is delivered to the requester — it will not appear in the assignee's inbox.
- **Expiry notes name only live bots**; a task whose ends were archived
  expires silently. `deadline_at IS NULL` never expires (legacy escape
  hatch).
- In synthetic mode nothing consumes inboxes, so `check_inbox` as a bot is
  also your read channel — remember every fresh bot has one `system` intro
  note to filter out.
- Real mode writes residue outside the throwaway home: transcript dirs in
  `~/.claude/projects/` and trust entries in `~/.claude.json` for the
  workspaces. Tell the user; offer to prune.

## Cleanup

```bash
pkill -f "target/debug/gravityd --config $H"
pkill -f "claude.*$H" 2>/dev/null   # real mode: orphaned sessions
rm -rf $H
```
