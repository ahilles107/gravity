# gravityd control plane protocol (v2)

Transport: WebSocket at `ws://<host>:49777/ws`. All frames are JSON text frames.
Every client→server message has a `req_id` (string, client-chosen, unique per connection).
Every server reply carries the same `req_id`. Server pushes (unsolicited) have no `req_id`.

## Handshake

Client sends first, immediately after connect:

```json
{ "type": "hello", "req_id": "1", "protocol_version": 2, "token": "<client token>", "client": "desktop/0.1.0" }
```

Server replies:

```json
{ "type": "hello_ok", "req_id": "1", "protocol_version": 2, "server_version": "0.1.0",
  "capabilities": ["terminal_attach", "search", "routines", "devices", "config"],
  "grants": ["read", "control", "approve"], "device_id": null }
```

or `{ "type": "error", "req_id": "1", "code": "auth_failed" | "unsupported_version", "message": "..." }`
followed by close.

Two credential kinds are accepted as `token`:

- **Owner token** (`~/.gravity/secrets/client.token`) — full grants, same machine.
- **Device token** — issued via `create_device`, carries scoped `grants`
  (`read` and/or `control`; `approve` is reserved) and can be revoked. A
  revoked device cannot reconnect.

Requests are gated by grants: `list_*`, `attach`, `detach`, `search`, and
`diagnostics` need `read`; everything else needs `control`. Violations get
`{ "code": "forbidden" }`.

Browser-context connections are additionally subject to Origin validation:
requests with an Origin header must match localhost/tauri defaults or the
daemon's `allowed_origins` config, otherwise the upgrade is rejected with 403.

## Errors

`{ "type": "error", "req_id": "...", "code": "<snake_case>", "message": "human text" }`
Codes: `auth_failed`, `unsupported_version`, `not_found`, `invalid_request`,
`runtime_unavailable`, `conflict`, `forbidden`, `internal`.

## Client → server requests

| type | fields | reply |
|---|---|---|
| `list_projects` | – | `projects` |
| `create_project` | `name` | `project` |
| `update_project` | `project_id, name` | `project` |
| `delete_project` | `project_id` | `ok` — archives the project and every bot in it; see Semantics |
| `list_bots` | `project_id?` | `bots` |
| `list_bot_activity` | `project_id?` | `bot_activity` — one preview line per bot; see Semantics |
| `create_bot` | `project_id, name?, description?, instructions?, avatar?` | `bot` (starts running immediately) |
| `update_bot` | `bot_id, name?, description?, instructions?, avatar?` | `bot` |
| `delete_bot` | `bot_id, reason?` | `ok` — archives the bot; see Semantics |
| `list_bot_revisions` | `bot_id, limit?` | `bot_revisions` |
| `revert_bot_revision` | `revision_id` | `bot` |
| `attach` | `bot_id, after_seq?` (number) | `attached` then `term` pushes |
| `detach` | `bot_id` | `ok` |
| `input` | `bot_id, data` (utf8 string, may contain control chars) | none (fire-and-forget; requires `control` grant) |
| `resize` | `bot_id, cols, rows, force?` | none (requires `control` grant) |
| `send_user_message` | `to_bot_id, body` | `message` |
| `list_messages` | `conversation_id, before_id?, limit?` | `messages` |
| `list_conversations` | `project_id?` | `conversations` |
| `list_routines` | `bot_id` | `routines` |
| `create_routine` | `bot_id, name, trigger, prompt, overlap_policy, max_duration_seconds?, max_attempts?` | `routine` |
| `set_routine_enabled` | `routine_id, enabled` | `routine` |
| `run_routine_now` | `routine_id` | `ok` |
| `cancel_routine_run` | `routine_run_id` | `routine_run` |
| `list_routine_runs` | `routine_id, limit?` | `routine_runs` |
| `emit_signal` | `project_id, name, payload?` | `signal` |
| `list_deliveries` | `bot_id?, state?` | `deliveries` |
| `retry_delivery` | `delivery_id` | `ok` |
| `search` | `query, project_id?` | `search_results` |
| `diagnostics` | – | `diagnostics` |
| `get_config` | – | `config` |
| `set_config` | `auto_compact_window` (number or `null`) | `config` |
| `list_devices` | – | `devices` |
| `create_device` | `name, capabilities` (array of `"read"`/`"control"`) | `device` (includes `token`, shown once) |
| `revoke_device` | `device_id` | `device` |

When `create_bot` omits `name` — or sends it as `null` — the daemon allocates the first
available placeholder in the project: `New Bot`, `New Bot 2`, and so on, retrying if another
client claims the same one first. Explicitly named bot creation is unchanged.

`trigger` is `{ "kind": "cron", "expr": "0 0 9 * * MON", "tz": "Europe/Warsaw" }`,
`{ "kind": "interval", "seconds": 3600 }`, or
`{ "kind": "signal", "name": "deploy.finished", "from_bot_id": "..." }`.
`from_bot_id` is optional; omitting it matches any bot in the project.
`overlap_policy`: `"skip" | "queue_one" | "queue_all" | "replace"`.
`max_duration_seconds` must be between 1 and 21600, and `max_attempts` between
1 and 5. Signal payloads are limited to 16 KiB of encoded JSON. A signal-fired
routine receives the signal metadata and payload in a `Signal context` block
appended to its prompt.

Version 2 replaces the former `event` trigger variant with `signal`. This is a
breaking wire-shape change; v1 clients must upgrade before connecting.

## Server replies (carry `req_id`)

- `ok`: `{}`
- `project` / `projects`, `bot` / `bots`, `message` / `messages`,
  `conversations`, `routine` / `routines`, `routine_run` / `routine_runs`, `signal`, `deliveries`, `search_results`,
  `diagnostics`: payload under a field of the same name. `diagnostics` carries
  `stale_build`: true when the daemon binary on disk is newer than the running
  process, i.e. it was rebuilt but not restarted and is still serving the old
  MCP tool list.
- `attached`: `{ "bot_id", "seq", "resumed" }` — `seq` is the last sequence
  number the replay covers and `term` pushes follow up to it. `resumed` says the client's
  `after_seq` was still contiguous with the server ring and its screen is intact; when it is
  false the client must clear its terminal. A replay the ring can no longer start at the first
  byte of the session starts at the first line break it still holds instead, so it never paints
  the half-written line eviction left behind. `force` on `resize` asks the
  daemon to nudge the pty size so the runtime repaints even when the dimensions match.
- `bot_activity`: `{ "activity": [{ "bot_id", "from", "text", "at" }] }` — `from` is empty
  when the bot itself spoke. Bots that have said nothing are omitted.
- `config`: the daemon configuration under `config` (shape below). Replied to both
  `get_config` and `set_config`; the latter echoes the state after the write.
  `port` is what the daemon serves and points bots at; `configured_port` is what
  `gravityd.toml` asked for. They differ when the configured port was occupied
  and a fallback was negotiated, which also moves the bus `/mcp` URL.

## Server pushes (no `req_id`)

- `term`: `{ "bot_id", "seq": <u64>, "data": "<utf8 terminal output>" }` — ordered per bot.
  One push may carry several consecutive pty reads merged together, up to 64 KB of output;
  `seq` is then the newest read included, and it is the cursor to resume from. A replay
  arrives the same way, so a full ring is a handful of pushes rather than one per read, and
  `attached.seq` still marks where it ends.
- `bot_state`: `{ "bot_id", "state", "reason", "at" }` — state ∈ `starting|ready|working|waiting_for_user|waiting_for_approval|rate_limited|auth_failed|crashed|stopping|stopped`.
- `message_new`: `{ "message": {...} }` — any new bus message visible to the user.
- `bot_updated`: `{ "bot": {...} }` — a bot was created, edited, or archived. Bots edit themselves unprompted, so clients must not cache identity across this push.
- `project_updated`: `{ "project": {...} }` — a project was created, renamed, or archived. As with `bot_updated`, archival is signalled by `deleted_at` being set rather than by a separate frame.
- `activity_update`: `{ "activity": { "bot_id", "from", "text", "at" } }` — a bot's preview
  line changed. Sent when a finished turn becomes readable in the transcript, which lags
  the `ready` state; see Semantics.
- `delivery_update`: `{ "delivery": {...} }`.
- `routine_run_update`: `{ "routine_run": {...} }`.
- `approval_pending`: `{ "bot_id", "detail" }` — bot is waiting on its native permission prompt.
- `notify`: `{ "level": "info|warn|error", "title", "body" }`.

## Entity shapes (JSON)

```jsonc
Project  { "id", "name", "created_at" }
Bot      { "id", "project_id", "name", "description", "avatar", "instructions",
           "state", "state_reason", "unread_count", "workspace_path", "dir_name",
           "created_by_bot_id"?, "deleted_at"?, "created_at" }
BotRevision { "id", "bot_id", "changed_by", "field", "old_value", "new_value",
           "created_at" }
Conversation { "id", "project_id", "bot_id", "title" }
Message  { "id", "conversation_id", "sender": {"kind":"user"|"bot"|"routine", "bot_id?", "name"},
           "kind": "task"|"reply"|"done"|"note"|"chat", "body", "ref_message_id?", "created_at" }
Delivery { "id", "message_id", "bot_id", "state": "queued"|"leased"|"delivered"|"acknowledged"|"failed",
           "attempt_count", "next_attempt_at", "last_error", "created_at" }
Routine  { "id", "bot_id", "name", "trigger": {...}, "prompt", "overlap_policy",
           "enabled", "max_duration_seconds?", "max_attempts", "next_run_at", "created_at" }
Trigger  { "kind": "cron", "expr", "tz" } | { "kind": "interval", "seconds" }
         | { "kind": "signal", "name", "from_bot_id?" }
RoutineRun { "id", "routine_id", "scheduled_for", "state": "scheduled"|"running"|"succeeded"|"failed"|"skipped"|"cancelled",
             "source": "schedule"|"manual"|"signal", "attempt", "signal_id?", "message_id?",
             "delivery_id?", "started_at", "deadline_at?", "next_attempt_at?", "finished_at", "error" }
Signal   { "id", "name", "source": "bot"|"manual", "project_id", "from_bot_id?", "payload",
           "origin_chain", "hop_count", "emitted_at" }
Diagnostics { "daemon_version", "protocol_version", "db_healthy", "runtime": {"kind","available","version?"},
              "delivery_backlog", "active_bots", "uptime_seconds" }
Config   { "bind": ["127.0.0.1"], "port": 49777, "configured_port": 49777,
           "runtime": "pty"|"double", "auto_compact_window": 250000|null }
Device   { "id", "name", "capabilities": ["read"|"control"|"approve"], "created_at",
           "revoked_at?", "last_seen_at?" }
```

Timestamps are RFC 3339 UTC strings. IDs are UUIDv4 strings.

## Semantics

- Bots are always-on: there is no `start_bot`/`stop_bot`. The daemon starts
  every live bot when it boots, starts a newly created bot straight away, and
  restarts crashes with backoff (2s doubling to 5 minutes), so `bot_state` is
  something clients observe, never something they drive. A bot only reaches
  `stopped` when the daemon stops it while archiving.
- Multiple clients may `attach` to the same bot; all receive `term` pushes. Any
  connection with the `control` grant may `input`/`resize` — there is no input
  lease. Bus deliveries are posted to the bot session's inbox socket
  (cross-session messaging), so they are read between tool calls or start a new
  turn when idle, and never interleave with terminal input.
- Reconnect: client re-`attach`es with `after_seq` = last seen seq; server replays buffered
  output after that cursor (in-memory ring buffer; if the cursor fell out of the buffer the
  server replays what it has and sets `attached.seq` accordingly — client should clear screen).
- `list_bot_activity` answers "what did this bot last say?", which the bus alone cannot:
  a bot's conversation with the user happens in its Claude Code terminal, a raw pty that
  never produces bus messages. The daemon reads the newest assistant turn out of Claude
  Code's own JSONL transcript (under `~/.claude/projects/<workspace-path-with-/-and-.-as-->`)
  and returns whichever of that and the bot's newest DM message is more recent. Use it for
  the initial snapshot only, and take live updates from `activity_update`.
- `activity_update` exists because Claude Code's `Stop` hook fires *before* the finished
  turn reaches the transcript — by roughly 250ms. A client that refetches on `bot_state`
  going `ready` therefore reads the *previous* turn and stays one reply behind. The daemon
  absorbs that race instead: on turn end it polls the transcript until it carries something
  at least as new as the turn, then pushes. A turn that only ran tools produces no text and
  so no push.

- `set_config` (advertised as the `config` capability, requires the `control`
  grant) writes only `auto_compact_window`: a number in 100000–1000000, or
  `null` for the model default. The value is persisted in the daemon's database
  — layered over `gravityd.toml`, which stays untouched — and takes effect the
  next time each bot's session starts. `bind`, `port` and `runtime` in the
  `config` reply are read-only: they describe how the daemon was launched and
  change only via `gravityd.toml` plus a restart.

## Bot self-management

Advertised as the `bot_self_management` capability in `hello_ok`. Every change
in this section is additive — new request types, one new push, new fields on
`Bot` — and did not independently require a protocol-version change.

Bots manage their own identity and each other over the MCP bus
(`get_self`, `update_self`, `rename_self`, `create_bot`, `update_bot`,
`delete_bot`). Nothing is queued for approval — a bot's change is live the
moment the tool returns. Three things bound that:

- **`max_bots_per_project`** (default 12) is the only limit on creation. Since
  a bot may delete only its own children, and archived bots free their slot,
  this caps the live population however deeply bots nest their teams. There is
  no spawn-depth or rate limit; neither would constrain anything the population
  cap does not.
- **Direct parentage only.** A bot may edit or delete bots it created, and
  nothing else. Authority is not transitive: if A created B and B created C,
  A cannot touch C. A bot cannot delete itself.
- **`bot_revision`** records every identity change with its author, and
  `revert_bot_revision` restores the previous value. This is what replaces an
  approval step: changes are not prevented, they are reversible.

Only `name` is required to create a bot, from either entry point. A bot
created without a description or instructions is given a placeholder charter
that tells it to ask its creator what it is for and to record the answer with
`update_self` — so a bare "create a bot called Steve" yields a running bot
rather than a question.

`avatar` is a validated short string — `icon:<name>`, naming one of the twenty
built-in bot icons the client bundles (`orbit`, `ember`, `moss`, `nova`,
`tide`, `quartz`, `volt`, `dusk`, `copper`, `frost`, `halo`, `glitch`, `slate`,
`bloom`, `echo`, `pixel`, `rune`, `cloud`, `comet`, `mint`), or `color:#rrggbb`,
or empty for a client-derived swatch. Paths, `data:` URIs and URLs are rejected,
so a bot names an icon rather than shipping bytes. A bot created without an
avatar is dealt one of the icons at random.

### A project's directory never moves

A project owns `~/.gravity/projects/<dir_name>/`, and every bot's
`workspace_path` points inside it. `dir_name` is derived from the name at
creation and then frozen, so `update_project` changes only what the project is
called: nothing on disk moves and no running bot loses its workspace. The name
still appears in `project.json` and each bot's `bot.json`, and both are
rewritten on rename so the files never contradict the database.

### Deleting a project takes its bots with it

`delete_project` archives every bot in the project through the same path as
`delete_bot` — stopped, credential revoked, open tasks released — and then
archives the project row itself. Skipping that would leave live runtimes holding
valid credentials for a project the user believes is gone.

Like a bot, a project is archived rather than deleted (`bot.project_id` and
`conversation.project_id` are foreign keys into `project`) and its name is
tombstoned so it can be reused immediately. Its directory is kept; the
workspaces inside it are reclaimed by retention on the usual
`archived_bot_days` schedule.

### Deletion is archival

`delete_bot` does not remove the row: `message.sender_bot_id` and
`conversation.bot_id` are foreign keys into `bot`, so deleting it would orphan
every message the bot ever sent. Instead the daemon stops the runtime, revokes
the bot's token, cancels its open tasks and tells each requester, archives the
DM conversation, and sets `deleted_at`. The bot leaves `list_bots`, addressing
and the population cap; its history stays readable.

The name is freed immediately (the archived row is tombstoned to
`<name>#<id-prefix>`), so create → delete → create with the same name works.
The workspace is kept and reclaimed by retention after `archived_bot_days`,
because deleting a bot should never destroy work it produced.
