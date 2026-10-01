# Bot runtimes

Gravity can run Claude Code and Codex CLI bots in the same project. Install and
sign in to the selected CLI on the daemon's computer. In **Bot info → Bot runtime**,
choose the CLI and click **Change runtime**. This interrupts the active turn and
restarts the bot. Its workspace, instructions, saved facts, bus messages and routines
remain available. The two providers keep separate conversations; switching back
resumes that provider's saved conversation.

![Runtime picker verified in the native Windows app](images/bot-runtime-picker.png)

Existing bots and new bots use Claude Code by default. For a Codex-only installation,
set this in `~/.gravity/gravityd.toml` and restart the daemon:

```toml
default_bot_runtime = "codex_cli"
codex_bin = "codex"
codex_args = []
```

Use an absolute `codex_bin` or `claude_bin` path when the daemon service cannot find
the CLI on its PATH. Bot-authored children inherit their creator's runtime when
the MCP `create_bot` call omits `runtime`. Both `create_bot` and `update_bot` accept
`runtime: "claude_code"` or `runtime: "codex_cli"`; `update_bot` can only change a
bot the caller created and leaves the runtime unchanged when omitted. These tools
return the saved runtime, also available through `list_bots` and `get_self`.
For example, `create_bot({"name":"Reviewer","runtime":"claude_code"})` creates a
Claude bot even when the caller uses Codex. A runtime change interrupts the child's
current turn and restarts it, just like the client picker. The selected CLI must
be available before creation or switching succeeds. Control clients may also pass
`runtime` when creating a bot. The `runtime = "double"` daemon
setting continues to select the deterministic test adapter for every bot.

## Codex terminal

Codex runs through [Codex App Server](https://learn.chatgpt.com/docs/app-server)
over a private authenticated loopback WebSocket, with the native Codex CLI terminal
attached to the same thread. Tested against Codex CLI 0.159.3; the CLI must support
`--remote` and App Server capability-token authentication. Codex's prompt editor,
status, slash commands, keyboard shortcuts, approval dialogs and questions appear
directly in Gravity's terminal. Terminal resize reaches the CLI; inline mode keeps
scrollback available. Use the native CLI controls to interrupt or leave a session.
Gravity supervision restarts a bot after its terminal exits.

Bus messages arrive through structured turn requests and never consume a partially
typed terminal prompt. Gravity observes both native and bus turns for activity and
routine completion. The native CLI handles approval decisions; Gravity does not
automatically answer them. Each bot has a separate App Server with a fresh random
capability token. The raw token is passed through the terminal environment and RPC
authorization header; only its SHA-256 verifier appears in server arguments.

The upstream WebSocket transport and remote terminal are experimental. Keep the
runtime contribution in draft until live provider validation is complete.

Each bot receives its own Gravity bus token through an environment variable; the
App Server config enables the local Gravity MCP endpoint. Codex uses workspace-write
sandboxing with the project's shared artifacts directory included. It reads the
shared system instructions, `CLAUDE.md` and `FACTS.md`. Thread IDs and activity
observations are saved beside the workspace for restarts and routine correlation.
Interrupted or failed turns do not report successful routine completion.

## Upgrade

The additive SQLite migration sets existing bots to `claude_code`. Update the
daemon and client together to expose the picker; the client hides it for older
daemons. Keep a database backup before upgrading, as older daemons do not understand
Codex bots. CLI authentication and a live model response require the user's account.
The default tests use a local App Server fixture. Opt-in native CLI tests use a
local mock Responses provider without paid model requests.

For a development installer with the same version number as the installed build,
refresh the managed daemon explicitly after installing:

```powershell
.\gravityd.exe service install
```

Use the daemon built with that installer. Running its `.exe` without `service
install` starts a foreground daemon and does not replace the managed service. The
refresh restarts bot sessions while preserving projects and configuration.
