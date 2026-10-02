# Peer bots: one team across two machines

Status: the daemon side (pairing, linking, forwarding, task mirroring and
artifact transfer) is implemented and covered by `crates/gravityd/tests/peer_bots.rs`.
The desktop UI is not built yet; pair and link with `gravityd peer …` until
it is. Requests are listed in [protocol.md](protocol.md).

A team can span two Gravity daemons. The motivating case is an app built for
macOS and Windows: a lead and a Mac developer run on the Mac, a Windows
developer runs on a Windows PC, and the lead hands the Windows side its work
without the user switching the desktop app between daemons to relay messages
or fetch files.

Each bot still runs, with its own terminal and runtime, on the machine that
owns it. What crosses machines is the bus: messages, tasks, results, and the
artifacts those results attach.

## Setting it up

On the Windows PC, add its Tailscale address to `bind` in `gravityd.toml`
(next to `127.0.0.1`) and restart the daemon. Then create an invite for the
Mac:

```sh
gravityd peer invite mac
```

On the Mac, add the PC with that invite, check that the link is up, and link
the Windows bot into the project the lead works in:

```sh
gravityd peer add win-pc "ws://100.x.y.z:49777/peer#…"
gravityd peer list
gravityd peer bots win-pc
gravityd peer link win-pc windev --project my-app
```

The lead can now `send_message(to: "windev", kind: "task", …)`. The first
message creates a linked `lead` in the Windows bot's project, so its answers
come back without any setup on the PC.

## Assumptions

- Both daemons belong to one owner and reach each other over Tailscale. Like
  remote desktop clients today, the link is plain `ws://` on the tailnet; the
  tailnet is the transport security, and a peer token is the authentication.
- Two machines is the target. Nothing below limits a daemon to one peer, but
  multi-peer routing (A relaying for B to C) is out of scope: messages only
  ever travel one link.
- The desktop UI is due for a rework, so this version adds the minimum UI to
  pair, link, and tell a linked bot apart, and leaves the rest for the rework.

## Concepts

**Peer.** Another daemon this one is paired with. Stored in a `peer` table:
local id, display name (`win-pc`), the remote daemon's stable id, the URL to
dial if this side dials, and `last_seen_at`. Each daemon gets a stable
`daemon_id` in `meta` the first time it starts.

**Linked bot.** A `bot` row whose `peer_id` is set. It stands in for a bot
that runs on the peer, with the peer's id for it in `remote_bot_id`. It has no
workspace and no runtime: the supervisor never starts it. Everything else
treats it as a bot in the project. It appears in `list_bots`, is addressed by
name, owns a DM conversation, and is the `from`/`to` of task rows. Because it is
a real row, every foreign key and guardrail that assumes a local bot keeps
working unchanged.

Linking is symmetric. When the Mac lead first messages the Windows developer,
the Windows daemon creates a linked bot for the lead in the Windows
developer's project. The Windows developer's replies and results are then
ordinary sends to a local bot, which the Windows delivery worker forwards
back.

## Pairing

1. On the Windows daemon (which must bind its Tailscale address, as for any
   remote client), the owner creates an invite. It returns a one-time code
   with the daemon's tailnet URL and a fresh peer token.
2. On the Mac, the owner adds the peer with that code. The Mac daemon stores
   the URL and token (`secrets/peer-<id>.token`), dials, and both sides
   record each other's `daemon_id` and name.
3. Revoking a peer on either side deletes its token, and that side's linked
   bots stop accepting deliveries (they fail with "peer revoked").

The dialing side holds one long-lived WebSocket to `/peer` on the other, and
traffic flows both ways over it. Only the listening daemon needs to be
reachable. The dialer reconnects with backoff, and `last_seen_at` drives the
online/offline badge.

A peer token authenticates only the `/peer` route. It carries no control-plane
grants: a peer cannot list projects, attach terminals, rule on decisions, or
reach a bot that has not been linked to it.

## Linking a bot

The owner picks a project on the Mac, a peer, and one of the peer's bots (the
peer answers a `list_bots` frame with id, name, description, and runtime). The
Mac creates the linked bot under the remote bot's name. If the name is already
taken in the project, the link is refused until one of the bots is renamed,
since names are what bots address each other by.

The peer records the link: the remote bot accepts frames from this peer only
once it is linked. That is the exposure boundary. Linking a bot exposes that
one bot to the peer, and nothing else in its project.

Unlinking archives the linked bot locally, like deleting any bot: its open
tasks are cancelled and its history is kept.

## Message flow

Sending is unchanged up to the delivery worker. `send_message`, `send_user_message`,
`complete_task`, and `cancel_task` insert messages, open or close task rows, and
enqueue a delivery to the recipient bot. The single new branch is in
`DeliveryWorker::attempt`: when the recipient is a linked bot, the worker
forwards the message over the peer link instead of calling
`Supervisor::deliver`.

- Peer offline: `NotReady`, so the delivery waits without spending attempts,
  exactly like a bot that is still starting.
- Peer rejected the frame (bot unlinked, task unknown, peer revoked):
  `Failed`, with the peer's reason as the delivery error.
- Peer acknowledged: `delivered`. The ack means the peer has durably stored
  the message and enqueued its own local delivery. It does not mean the remote
  bot has read it, which matches what `delivered` means locally.

A forwarded frame carries:

```json
{
  "type": "message",
  "id": "<sender-side message id>",
  "to_bot_id": "<recipient's id on the receiving daemon>",
  "from": { "kind": "bot", "bot_id": "<sender-side bot id>", "name": "lead" },
  "kind": "task | reply | note | done | chat",
  "body": "…",
  "ref": "<sender-side message id this answers, if any>",
  "task": { "id": "<sender-side task id>", "deadline_at": "…", "hop_count": 2 },
  "closes_task": { "id": "<receiver-side task id>", "state": "done | cancelled" },
  "artifacts": [{ "name": "build.log", "bytes": "<base64>" }]
}
```

The receiver handles it in one transaction:

1. **Dedupe.** `(peer_id, id)` in `peer_message` means it was already stored,
   so the receiver acks again and does nothing else.
2. **Resolve the recipient.** `to_bot_id` must be a live, local, non-linked
   bot linked to this peer.
3. **Resolve the sender.** `kind: bot` uses the linked bot for
   `(peer, from.bot_id)`, creating it in the recipient's project on first
   contact. `kind: user` is the owner speaking from the other machine; it
   arrives as `chat` with the user's authority, which is the reason peer
   tokens are only issued between the owner's own daemons.
4. **Insert and enqueue** with `messaging::send_dm`, mapping `ref` through
   `peer_message` so threading survives the hop.
5. **Mirror the task.** A `task` frame opens a local task from the linked
   sender to the recipient. It keeps the sender's `deadline_at`, and its
   `hop_count` is the sender's, so the hop limit holds across the whole chain.
   `peer_task` maps the two task ids. A `closes_task` frame flips the mapped
   local task (`try_close_task`) before inserting the `done` or cancel note,
   exactly as the local tool would.

Replies count against the reply budget on both sides, because each side holds
its own copy of the task. Loops are refused locally by the existing chain
check: the Windows developer cannot delegate back to the lead's linked bot
while working the lead's task, because that linked bot is already in the
chain. Deadlines expire independently on both sides from the same timestamp.

### Artifacts

`complete_task` already lists artifact paths in the `done` body. When the
recipient is a linked bot, the forwarder also reads each listed file and
sends its bytes in the frame. Only regular files under the sending project's
artifacts directory or the sending bot's workspace are sent. The cap is
16 MiB per file and 48 MiB per message. A file over the cap stays listed by its
remote path, with a note saying it was not transferred.

The receiver writes the files to
`<project artifacts>/peers/<peer name>/<task id>/` and rewrites the listing in
the stored body to those local paths. The lead reads the result exactly as it
would read a local one.

## What bots are told

- `list_bots` adds `"machine": "<peer name>"` and `"online": bool` for linked
  bots.
- The envelope header names the machine: `from windows-dev @ win-pc`. The
  sender is still daemon-authenticated, but the authority is a peer bot's,
  never the user's, unless the frame came from the owner (`kind: user`).
- The system prompt's bus section says a linked bot runs on another machine
  and that paths it mentions outside a transferred artifact are not readable
  here.

## Desktop app (minimum for this version)

- Settings → Peers: create an invite, add a peer from a code, see each peer's
  status, and revoke a peer.
- Project → Link a bot from a peer.
- A linked bot shows its machine and online state in the bot list. Selecting
  it shows its conversation and the composer instead of a terminal. Sending to
  it from the composer is a forwarded `chat`.

A linked bot's chat (turns, step details, images, and files from its own
directory) is read from the daemon it runs on: the app asks its own daemon,
which asks the peer over the link (`chat`, `chat_step`, `chat_image` and
`read_file` frames). While the chat is open, the peer streams `chat_turns`
event frames back for it. A peer serves these only for bots linked to it.
Daemons advertise this as the `peer_chat` capability; with an older daemon the
app falls back to the bot's bus conversation. Permission prompts of a linked
bot are answered on its own machine.

Watching the Windows bot's terminal from the Mac is left to the UI rework.
It needs the app to hold connections to two daemons at once.

## Schema

One append-only migration:

```sql
CREATE TABLE peer (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL UNIQUE,
    daemon_id     TEXT UNIQUE,      -- learned at first handshake
    url           TEXT,             -- set on the dialing side
    created_at    TEXT NOT NULL,
    last_seen_at  TEXT,
    revoked_at    TEXT
);
ALTER TABLE bot ADD COLUMN peer_id TEXT REFERENCES peer(id);
ALTER TABLE bot ADD COLUMN remote_bot_id TEXT;
CREATE UNIQUE INDEX idx_bot_remote ON bot(peer_id, remote_bot_id, project_id)
    WHERE peer_id IS NOT NULL AND deleted_at IS NULL;

CREATE TABLE peer_link (            -- local bots exposed to a peer
    peer_id TEXT NOT NULL REFERENCES peer(id),
    bot_id  TEXT NOT NULL REFERENCES bot(id),
    PRIMARY KEY (peer_id, bot_id)
);
CREATE TABLE peer_message (         -- dedupe and ref mapping
    peer_id           TEXT NOT NULL REFERENCES peer(id),
    remote_message_id TEXT NOT NULL,
    message_id        TEXT NOT NULL,
    PRIMARY KEY (peer_id, remote_message_id)
);
CREATE TABLE peer_task (            -- the two halves of a mirrored task
    peer_id        TEXT NOT NULL REFERENCES peer(id),
    remote_task_id TEXT NOT NULL,
    task_id        TEXT NOT NULL,
    PRIMARY KEY (peer_id, remote_task_id)
);
```

## Delivery plan

1. **Peers and forwarding (daemon).** The schema, `daemon_id`, pairing and
   revocation as control-plane requests (plus `gravityd peer …` commands that
   drive the local daemon, so this is usable before the UI lands), the `/peer`
   route and dialer, linking, and forwarding of every message kind with task
   mirroring. Tested with two in-process daemons on the double runtime.
2. **Artifacts.** File transfer on `done`, with path rewriting and the caps.
3. **Desktop app.** The Peers settings, link flow, and linked-bot view.
4. **Live check.** Mac and Windows over Tailscale with real runtimes, then a
   `bus-live-test` mode for two daemons.
