# Chat pane: the default bot surface

Issue: [ahilles107/gravity#13](https://github.com/ahilles107/gravity/issues/13).
Status: phases 1 (daemon chat model), 2 (desktop chat) and 3 (permission
cards) implemented; the live Claude Code check for phase 3 is pending. Branch:
`codex/chat-pane`
(stacked on `codex/peer-bots`).

Opening a bot today shows its terminal. The terminal is the truth, but it is a
poor way to read what a bot did. Images are invisible, diffs scroll past, files
it wrote are paths you have to go and open, and a permission prompt is a
keystroke you have to type. This design makes a chat the default surface. The
chat has message bubbles, folded tool steps, inline diffs and images, an
artifacts panel, and permission cards that actually decide the tool. The
Claude Code terminal stays one tab away.

The issue's v1 is the core. This document also covers what the issue left out,
because the goal is a full rework of how a bot is read and driven, not a
second view bolted on.

## What exists today

- **Desktop.** `BotView` has two tabs, Terminal and Routines, plus an info
  panel. Nothing renders a bot's conversation, and nothing calls
  `send_user_message`. A permission prompt raises a toast and an amber dot,
  and the only way to answer it is to type into the terminal. Bot markdown is
  rendered only in the Control Center, with links and images turned into
  inert text by policy.
- **Daemon.**
  - `activity.rs` finds a bot's Claude Code transcript
    (`~/.claude/projects/<mangled workspace>/*.jsonl`, newest file) but only
    reads the newest assistant text, for the sidebar preview.
  - Hooks are fire-and-forget curls for six events, and no hook ever returns
    a decision.
  - Codex bots write `codex-observations.jsonl`, which holds text only.
  - Nothing serves a file's bytes to a client.
- **gravity_os** (the iPhone app) reads turns, diffs, screenshots and reports.
  It gets them from **Gravity Lens**, a companion Python service that parses
  the same transcripts because the daemon does not serve them. Its turn model
  is the best prior art we have, and this design moves it into the daemon so
  the desktop and the phone read one source.

## Principles

1. **The transcript is the log.** The human↔bot conversation is read from the
   runtime's own record. It is never reconstructed from terminal output, and
   it is never duplicated into the bus database.
2. **The daemon parses, clients render.** One parser in Rust, one wire model.
   The desktop, gravity_os and any future client read the same thing.
3. **Additive protocol.** New requests, pushes and a `chat` capability. The
   protocol stays at version 2. Old clients keep working. New clients hide the
   chat when the capability is absent and fall back to the terminal.
4. **The terminal stays authoritative.** Anything the chat cannot do, such as
   slash commands, raw output or an unanswered prompt, is one tab away, and
   the chat says when to go there.
5. **Permission answers never default to yes.** A card that is not answered in
   time is a deny. If the daemon cannot be reached, Claude Code's own prompt
   appears in the terminal. Nothing persists an always-allow.

## The wire model

A bot's chat is a list of **turns**. A turn starts with what woke the bot and
ends when the runtime reports it stopped. This is the shape gravity_os already
proved on a phone.

```text
ChatTurn {
  id                      // stable: the transcript uuid of the first record
  bot_id
  started_at, ended_at?   // ended_at absent while the turn is open
  open: bool
  trigger: Trigger
  items: [ChatItem]       // in transcript order
  stats: { commands, reads, edits, added, removed, sent, images, errors }
}

Trigger =
  | Owner   { text, via: "chat" | "terminal" }   // composer, or typed in the PTY
  | Bus     { from, kind, num, text, task_id? }  // another bot's message
  | Routine { name, run_id? }
  | Ruling  { decision_id }                      // a decision answer delivered
  | Resumed                                      // a turn continued after compaction
  | Background { text }                          // task notification, queued command

ChatItem =
  | Text       { id, markdown }                  // consecutive assistant text merges
  | Step       { id, tool, title, subtitle?, status: running|ok|error|denied,
                 added?, removed?, images: [ImageRef], detail: bool }
  | Sent       { id, to, kind, body }            // send_message on the bus
  | Completed  { id, task_id, result, artifacts: [FileRef] }
  | Decision   { id, decision_id, title }        // raise_decision
  | Permission { id, request_id, tool, summary, state }   // see Permissions
  | Aside      { id, kind: compacted|interrupted|incoming|model, text }
```

`Step.title` is human ("Edited `src/app.ts`", "Ran tests"), using the
descriptions bots already give their Bash calls. Gravity's own bus tools are
not steps: they become `Sent`, `Completed` and `Decision` items, because those
are what the owner wants to see. Housekeeping tools (`check_inbox`,
`list_bots`, ToolSearch, Skill) are steps marked minor and fold away.

Heavy content is not in the turn. It is fetched on demand:

- **Step detail.** Tool input, command, output (tail-truncated), and the diff.
  The diff comes from Claude Code's `toolUseResult.structuredPatch`, not from
  re-diffing the Edit inputs, and is capped at 400 lines.
- **Images.** Base64 image blocks inside tool results, plus image paths
  mentioned by steps (screenshots a bot took), served by id with a thumbnail
  size.
- **Files.** Artifacts and any file under the bot's directory or its project's
  artifacts directory.

### Recognising who spoke

The parser classifies user records with markers Claude Code already writes:

- **Typed in the terminal:** `origin.kind == "human"` → `Owner{via: terminal}`.
- **Delivered by Gravity:** `isMeta` with `origin.kind == "peer"`, wrapped in
  "Another Claude session sent a message:" plus a trailer. The parser strips
  the wrapper and reads the envelope (`[msg #N from X · kind · …]`):
  - `from USER · chat` is the owner writing from the composer
    (`Owner{via: chat}`);
  - any other sender is a `Bus` trigger;
  - `[routine "…"]` is a `Routine` trigger.
- **Mid-turn deliveries** (queued commands, a message arriving while the bot
  works) become `Aside{incoming}` inside the open turn rather than starting a
  new one.
- Sidechain records (sub-agents) are skipped in v1 and summarised by the step
  that launched them.

## Protocol additions

New capability `chat` in `hello_ok`. All additive.

| Request | Grant | Fields | Reply |
| --- | --- | --- | --- |
| `list_chat` | read | `bot_id, before?, limit?` | `chat { bot_id, turns, has_more }`, newest last |
| `get_chat_step` | read | `bot_id, turn_id, item_id` | `chat_step { input?, command?, output?, diff?, content? }` |
| `get_chat_image` | read | `bot_id, image_id, size: thumb\|full` | `file` |
| `list_artifacts` | read | `project_id` | `artifacts [{ path, name, size, modified, mime, title? }]` |
| `read_file` | read | `project_id \| bot_id, path, max_bytes?` | `file { name, mime, size, text? \| base64?, truncated }` |
| `list_permissions` | read | `bot_id?` | `permissions [PermissionRequest]` (pending only) |
| `answer_permission` | control | `request_id, decision: allow_once\|allow_session\|deny, reason?` | `permission` |

| Push | Payload |
| --- | --- |
| `chat_turns` | `{ bot_id, turns }`: new or changed turns (the open one, mostly) |
| `permission_request` | `{ request }` |
| `permission_resolved` | `{ request_id, bot_id, outcome: allowed_once\|allowed_session\|denied\|expired\|answered_in_terminal }` |

- **Paths.** `read_file` accepts only paths that resolve inside the bot's own
  directory or its project's artifacts directory, using the same rule as
  peer-bot artifact transfer. Text is returned as text and anything else as
  base64, capped at 16 MiB.
- **Grants.** Answering a permission is `control`, the same grant as typing
  the answer into the terminal. `approve` stays the authority to rule on
  decisions. A device with `read` + `approve` (the phone) can rule on
  decisions but cannot let a bot run a tool.

### Live updates

The daemon keeps a per-bot reader that tracks the transcript file and the
byte offset it has parsed, and re-parses only the open turn. It reads on the
hooks it already receives (`UserPromptSubmit`, `PostToolUse`, `Stop`,
`Notification`), plus a 1 s poll while a bot is `working`, and pushes
`chat_turns` when something changed. Claude Code writes each assistant message
when it completes, so text appears message by message and steps appear as
they finish. That is live enough without token streaming.

The same reader replaces the sidebar's `activity.rs` scan. The preview line
becomes the open turn's current step or last text, which also fixes turns
that only used tools showing no preview.

## Permissions

The issue asks for a blocking `PreToolUse` hook. **We use Claude Code's
`PermissionRequest` hook instead**, which also blocks and decides the tool.
`PreToolUse` fires before every tool call, including the many the bot is
already allowed to run, so a blocking `PreToolUse` would either stall every
step behind the daemon or force the daemon to re-implement Claude Code's
permission rules to know which calls need asking. `PermissionRequest` fires
exactly when Claude Code would show its own prompt, which is the case the card
replaces. The hook, its allow/deny `behavior` and `updatedPermissions` are
present in the installed Claude Code (2.1.286). The implementation must still
confirm the exact input and output schema against that version before relying
on it.

Flow:

1. The hook posts its stdin (tool name, input, session, permission
   suggestions) to `POST /hook/permission` with the bot's token, and waits.
   The hook entry sets a `timeout` longer than the daemon's answer window.
2. The daemon records a `PermissionRequest { id, bot_id, tool, input, summary,
   created_at, expires_at }`, sets the bot to `waiting_for_approval`, pushes
   `permission_request`, raises a system notification, and holds the HTTP
   request open.
3. The card offers **Allow once**, **Allow for this session** and **Deny**
   (with an optional reason sent back to the bot).
   - Allow once → `behavior: allow`.
   - Allow for this session → `allow` plus the session-scoped rule Claude Code
     suggested (`updatedPermissions`, destination `session`). It is never
     written to a settings file.
   - Deny → `behavior: deny` with the reason as the message.
4. If the request is not answered within the answer window (default 10 min,
   `permission_timeout_minutes` in config), the daemon answers deny with
   "The owner did not answer in time", resolves the card as `expired` and
   the bot moves on.
5. The daemon only holds a prompt while an app that shows cards is connected
   with `control` (it says so with `features: ["permission_cards"]` in
   `hello`). With no such app open, it answers at once with no decision, so an
   older app, or none, leaves the prompt in the terminal as before.
6. If the daemon is unreachable or answers nothing, the hook prints nothing
   and exits 0. Claude Code then shows its own prompt in the terminal, and the
   chat shows "answer in the terminal". This is the permission-relay fallback,
   and it is still closed: nothing is allowed without someone answering.

The card is rendered inline in the open turn and pinned above the composer
until it is answered. The bot header, the sidebar dot and the toast all point
to it.

**Done means proven live.** The issue stays open until a real Claude Code
session has shown that Allow from the card runs the tool without the terminal
prompt, Deny blocks it, and expiry denies it. A test runs this against the
installed `claude` binary; it is gated behind an env var because it is billed.

## The desktop

```text
┌ sidebar ┐┌ header: avatar · name · state · runtime · ⓘ ────────────────────┐
│         ││ [ Chat ] [ Terminal ] [ Routines ]        │ [ Info ] [ Files ]  │
│ bots    ││                                           │                     │
│ ...     ││  ┌ turn ──────────────────────────────┐   │  artifacts list     │
│         ││  │ Task from lead · 14:02 · took 3m    │   │  ─ report.md        │
│         ││  │ "port the updater to Windows"       │   │  ─ screenshot.png   │
│         ││  │  ▸ 6 steps · 3 edits +120 −4 · 2 🖼 │   │                     │
│         ││  │  [img][img]                          │   │  preview:           │
│         ││  │  Done — the updater builds on…      │   │  rendered markdown, │
│         ││  │  ✓ Completed task · report.md       │   │  highlighted code,  │
│         ││  └─────────────────────────────────────┘   │  image, diff        │
│         ││  ┌ permission ─────────────────────────┐   │                     │
│         ││  │ Bash: rm -rf build/  [Allow once]   │   │                     │
│         ││  │ [Allow for session] [Deny…]          │   │                     │
│         ││  └─────────────────────────────────────┘   │                     │
│         ││  ┌ composer ──────────────────── ⏎ ┐       │                     │
└─────────┘└──────────────────────────────────────────────────────────────────┘
```

- **Tabs.** Chat is the default and opens on the newest turn. Terminal stays
  mounted so switching is instant, as it is today. The last tab per bot is
  remembered.
- **Turns.** Each turn has a header naming its trigger ("You", "Task from
  lead", "Routine nightly-report", "You, in the terminal"), the time and the
  duration. Then come the trigger text, the items, and a stats line.
  - Consecutive steps fold into a group ("6 steps · 3 edits +120 −4"), open by
    default only while the turn is running or when it has four steps or fewer.
  - Clicking a step loads its detail: command, output and a diff view with
    added and removed lines.
  - Images show as a thumbnail strip and open in a viewer.
- **Bubbles.** Your messages are right-aligned. The bot's text is
  left-aligned, rendered as markdown with highlighted code blocks and a copy
  button. Bus traffic and routine prompts are labelled asides ("From lead ·
  task", "To windev · task"), never styled as you.
- **Composer.** Enter sends through `send_user_message`; Shift+Enter adds a
  newline. Each message shows its delivery state from `delivery_update`
  (queued, delivered, failed with retry). The composer is disabled with a
  reason when the connection is read-only or the bot is stopped. While the
  bot works, the composer still sends: the message is queued into the running
  turn, as the bus already does.
- **Files panel.** A second tab in the right panel lists the project's
  artifacts, newest first, including files that arrived from peers. It
  previews markdown, code with syntax highlighting, images and plain text.
  Artifact paths in a `Completed` item, and file paths the bot mentions,
  open here.
- **Linked bots** (from peer bots) have no local transcript. Their chat shows
  their DM conversation from the bus (`list_messages`), labelled with the
  machine, until phase 5 proxies their real chat over the peer link.
- **Markdown policy.** Bot output stays untrusted. Images render only from
  daemon-served bytes (`read_file`, `get_chat_image`) and never from a URL in
  the text. `http(s)` links open in the system browser after a hover shows the
  full URL. Every other scheme stays text.
- **Keyboard.**
  - ⌘1/⌘2/⌘3 switch between Chat, Terminal and Routines.
  - ⌘L focuses the composer.
  - In a pending permission card, `a` allows once, `s` allows for the session
    and `d` denies, with the key hints visible.
  - Esc returns focus to the turn list.

## Beyond the issue's v1

The issue left four things out. Here is where each one lands:

- **Answering decisions in the thread.** In phase 6. A `Decision` item shows
  the options and answers through `answer_decision`, the same request the
  Control Center uses. The Control Center stays the inbox and the record.
- **Token streaming.** Not planned. Message-by-message updates from the
  transcript are what the runtimes give us without a different integration.
- **Parsing xterm into bubbles.** Rejected. The transcript is structured,
  and scraping a screen is not.
- **Agent SDK.** Out of scope for the UI. It is a new runtime adapter and
  deserves its own issue. The chat model above would serve it unchanged.

Gaps the issue did not mention, and the phase that covers each:

- **Codex bots** (phase 4). The worker already sees every App Server item.
  `codex-observations.jsonl` will record tool items too (commands, file
  changes with diffs) in the same turn shape. Approval requests become the
  same permission cards, answered over the App Server.
- **Linked bots** (phase 5). The peer link gains a `chat` request, so a
  bot on the other machine reads like a local one, and its artifacts are
  readable through the link.
- **Attachments in the composer** (phase 6). Dropping a file copies it into
  the project's artifacts directory and adds its path to the message.
- **Slash commands from the composer** (phase 6). A message starting with
  `/` is typed into the terminal, with a hint that it runs there.
- **Search in chat** (phase 6), across the turns already loaded and the
  bus search the daemon already has.
- **gravity_os** (phase 6). Moves from Gravity Lens to `list_chat`,
  `get_chat_step`, `get_chat_image`, `list_artifacts` and `read_file`, and
  Lens is retired.

## Phases

1. **Daemon chat model.** The transcript parser and reader, `list_chat`,
   `get_chat_step`, `get_chat_image`, `chat_turns`, `list_artifacts`,
   `read_file`, and the `chat` capability. Fixture transcripts with real
   record shapes (typed, peer-delivered, tool results with images and
   structured patches, compaction, interruptions).
2. **Desktop chat.** The Chat tab as default, turns, steps, detail, diffs,
   images, the composer with delivery state, the Files panel with previews
   and highlighting, and the linked-bot DM view. Ladle stories for every
   item kind, with CI-generated baselines.
3. **Permissions.** The `PermissionRequest` hook, `/hook/permission`, cards,
   `answer_permission`, expiry and the terminal fallback, plus the live
   Claude Code check that closes the issue.
4. **Codex parity.** Tool items in observations and approvals answered from
   cards.
5. **Linked bots over the peer link.**
6. **Beyond v1.** Decisions in the thread, attachments, slash commands,
   search, and moving gravity_os off Lens.

## Found while researching

These are not part of this work, but they surfaced while mapping the code:

- On Unix the Stop hook posts a fixed body, so `transcript_path` never
  reaches the daemon for Claude bots. Separately,
  `routine_run_id_from_transcript` expects the record to start with
  `[routine "`, but Claude Code prefixes delivered messages with "Another
  Claude session sent a message:". Routine completion via the transcript
  may therefore never match for Claude bots on Unix. Phase 1's parser
  replaces that scan.
- `transcript_dir` replaces `/` and `.` but not `_`, while bot directory
  names may contain `_`. If Claude Code replaces every non-alphanumeric
  character, bots with `_` in their path have no activity preview. Phase 1
  fixes the mangling.
- The desktop's `list_messages` type sends `before_id`, but the daemon reads
  `before_num`.
- Claude Code tells the bot that a delivered message "came from another
  Claude session — not typed by your user". The composer's messages arrive
  that way. The system prompt already overrides this for Gravity, but if
  bots treat composer messages as less authoritative in practice, the
  composer can type into the session instead when the bot is idle. That
  would be a phase 2 follow-up, decided from live use.
