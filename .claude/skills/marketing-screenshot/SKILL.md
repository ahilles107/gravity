---
name: marketing-screenshot
description: Capture the real Gravity desktop app for the marketing site using a disposable daemon and staged example data. Real Claude Code sessions are billed and require explicit consent.
---

# Marketing screenshot

Use the native macOS app with fictional project data. Confirm consent and a
small turn budget before starting `runtime = "pty"`: creating each bot starts
a real, billed Claude Code session. Use `double` for a free layout rehearsal;
it cannot demonstrate genuine agent behavior.

Keep the installed app and daemon untouched. Never assume their port or data
location; use a fresh daemon home and an unused test port. Requirements are in
`CONTRIBUTING.md`; the driver additionally requires Node 22+. A real run needs
an authenticated `claude` executable on PATH. Do not print credentials, raw
process command lines, or real session transcripts into shared reports.

## Setup

Run from the repository root, in a dedicated terminal. These examples use
49888 for the daemon and 1420 for the frontend. Check both are free first;
choose other ports consistently if occupied, without stopping their owners.
Keep shell tracing off when loading credentials.

```bash
umask 077
SHOT_DIR=$(mktemp -d "${TMPDIR:-/tmp}/gravity-shot.XXXXXX")
SHOT_DIR=$(cd "$SHOT_DIR" && pwd -P)  # canonical paths also avoid transcript aliases
export GRAVITY_HOME="$SHOT_DIR/daemon"
export GRAVITY_WS=ws://127.0.0.1:49888/ws
mkdir -p "$GRAVITY_HOME" "$SHOT_DIR/shots"
CLAUDE_BIN=$(command -v claude)
cargo build -p gravityd
./scripts/prepare-sidecar.sh
cat > "$GRAVITY_HOME/gravityd.toml" <<EOF
home = "$GRAVITY_HOME"
bind = ["127.0.0.1"]
port = 49888
negotiate_port = false
runtime = "pty"
claude_bin = "$CLAUDE_BIN"
supervision_interval_ms = 1000
EOF
./target/debug/gravityd --config "$GRAVITY_HOME/gravityd.toml" > "$SHOT_DIR/gravityd.log" 2>&1 &
SHOT_DAEMON_PID=$!
```

Wait for `$GRAVITY_HOME/gravityd.port` and a successful
`curl --fail http://127.0.0.1:49888/health`. Confirm the recorded daemon PID
is still running and the published port is 49888. Do not continue on a startup
failure. Leave `$HOME` unchanged so Claude Code can use its existing login.

Start the app in the same terminal; the dev token stays in the local process
environment, not a command argument or a pasted browser-tool call:

```bash
export VITE_GRAVITY_DEV_PORT=49888
export VITE_GRAVITY_DEV_TOKEN="$(cat "$GRAVITY_HOME/secrets/client.token")"
(cd apps/desktop && env -u CONDUCTOR_PORT pnpm tauri dev \
  --config '{"identifier":"build.getgravity.screenshot","build":{"devUrl":"http://localhost:1420"}}')
```

The separate app identifier separates screenshot app storage from the official
app. A previous screenshot profile can still contain connection overrides:
the footer **must** read `127.0.0.1:49888 · connected` before any interaction.
If it differs, stop and reset only that disposable profile's connection settings.
Never install, restart, or update the managed production daemon from this app.
Do not expose the dev frontend to the network or deploy a build containing its
local test token.

## Staging the scene

In another terminal, set `GRAVITY_HOME` to the exact disposable daemon home
from setup and `GRAVITY_WS` to its loopback URL. The bundled driver validates
that the endpoint matches the home's published port before reading credentials.

```bash
DRIVER=.claude/skills/marketing-screenshot/scripts/drive.mjs
PROJECT_ID=$(node "$DRIVER" project Example)
node "$DRIVER" bot "$PROJECT_ID" Argus "Coordinates the example project." \
  "Use fictional example data only. Create no additional bots until asked." icon:quartz
```

The driver prints the bot ID. Use `say <bot-id> <prompt>` to ask for a small,
budgeted task, or additional bots within the approved budget. `list` shows IDs
and states; `peek <bot-id>` prints terminal content, so review it locally for
sensitive information before sharing it. `update` changes a bot's charter.
`delete` archives a bot: identify the exact bot and obtain deletion approval
before using it. Every command must target this run's disposable home.

Let the work settle naturally. Do not backdate database messages or add symlinks
inside the user's Claude transcript directory to make the scene look busy.
Pin the lead bot, select it, and collapse the info panel if that suits the shot.

## Capturing

Use available native accessibility/window tools. Identify the dev app's exact
PID and executable path from the session you launched; do not target a process
by name alone. The installed app may have the same executable name. Inspect
only the chosen window, raise it, and read its current bounds. For example,
with the verified PID in `SHOT_APP_PID`:

```bash
osascript -e "tell application \"System Events\" to tell (first process whose unix id is $SHOT_APP_PID) to perform action \"AXRaise\" of window 1"
osascript -e "tell application \"System Events\" to tell (first process whose unix id is $SHOT_APP_PID) to get {position, size} of window 1"
# Set SHOT_BOUNDS to those four numbers as x,y,width,height, then:
screencapture -o -x -R "$SHOT_BOUNDS" "$SHOT_DIR/shots/gravity-hero.png"
sips -g pixelWidth -g pixelHeight "$SHOT_DIR/shots/gravity-hero.png"
```

Resolution depends on the display scale and actual window size. Inspect the
saved PNG for overlapping windows, private paths, account information and real
project data before copying it into public marketing assets. Do not claim
synthetic output is a real agent session.

## Cleanup

Stop the foreground `pnpm tauri dev` with Ctrl-C. Stop only the daemon PID
recorded in this live shell (`kill "$SHOT_DAEMON_PID"; wait "$SHOT_DAEMON_PID"`).
If the shell/session was lost, verify process ownership again before signalling;
never use broad `pkill` patterns. Unset the temporary Vite token environment.

Retain the screenshot and run directory for review. Ask before deleting the
exact directory. Real Claude sessions may also create transcript directories
under `~/.claude/projects` and trust entries in `~/.claude.json`. Report this
residue; do not delete globbed paths or rewrite the user's configuration.
