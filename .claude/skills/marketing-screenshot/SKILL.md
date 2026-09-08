---
name: marketing-screenshot
description: Capture the hero screenshot of the Gravity desktop app for getgravity.build — runs the real Tauri app against a throwaway daemon with real (billed) Claude Code sessions, stages a believable project and team, and captures the window at retina resolution. Use when the marketing page needs a new or refreshed app screenshot, or when someone asks for a screenshot of the app showing bots at work.
---

# Marketing screenshot

Produces `/tmp/gravity-shot/shots/gravity-hero.png` — the real desktop app, real
Claude Code sessions, a project that looks like someone's actual working day.

Two rules the whole setup exists to satisfy:

- **The installed Gravity must not be touched.** The user runs a real daemon on
  `~/.gravity` port 49777 with their own bots. Everything here is a throwaway
  daemon on port 49888.
- **The bots must be real.** Faking terminal output with the `double` runtime
  looks wrong the moment anyone reads it. Use `runtime = "pty"`, which spawns
  actual `claude` sessions **billed to the user's account** — get consent, and
  budget roughly one turn per bot plus a few for the prompts you send.

## Setup

The daemon needs the *real* `$HOME` or Claude Code is not logged in; the app
needs a *sandbox* `$HOME` or it connects to the installed daemon. So they get
different homes.

```bash
cargo build -p gravityd
./scripts/prepare-sidecar.sh                    # tauri dev needs the staged sidecar

S=/tmp/gravity-shot; rm -rf $S; mkdir -p $S/daemon $S/apphome/.gravity/secrets $S/shots
cat > $S/daemon/gravityd.toml <<EOF
home = "$S/daemon"
port = 49888
negotiate_port = false
runtime = "pty"
claude_bin = "$HOME/.local/bin/claude"          # absolute: the daemon's PATH is thin
supervision_interval_ms = 1000
EOF
./target/debug/gravityd --config $S/daemon/gravityd.toml > $S/gravityd.log 2>&1 &
sleep 3 && curl -s http://127.0.0.1:49888/health          # expect status ok
grep "runtime available" $S/gravityd.log                  # expect a Claude Code version

# What the app reads to find "the local daemon": a port and a token.
cp $S/daemon/secrets/client.token $S/apphome/.gravity/secrets/client.token
printf 'port = 49888\n' > $S/apphome/.gravity/gravityd.toml
echo 49888 > $S/apphome/.gravity/gravityd.port
```

Then run the app. `CONDUCTOR_PORT` must be unset — `vite.config.ts` honours it
with `strictPort`, while `tauri.conf.json` waits on 1420 and the two never meet.

```bash
cd apps/desktop && env -u CONDUCTOR_PORT \
  HOME=/tmp/gravity-shot/apphome \
  CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup PNPM_HOME=$HOME/Library/pnpm \
  pnpm tauri dev > /tmp/gravity-shot/tauri.log 2>&1 &
```

The footer must read `127.0.0.1:49888 · connected`. If it says 49777, the app
found the installed daemon and everything below would edit the user's real data —
stop and fix the sandbox home first.

## Staging the scene

`scripts/drive.mjs` speaks the WS control plane (`list`, `project`, `bot`, `say`,
`peek`, `update`, `delete`). Create the project and **one** lead bot, then have
that bot create the rest — a team the user watched a bot build reads as real,
and a preconfigured roster does not.

```bash
D=".claude/skills/marketing-screenshot/scripts/drive.mjs"
P=$(node $D project Work)
node $D bot $P Argus "Runs this project. Builds the team it needs, routes the day's work." \
  "You run this project. Create the bots you need, give each a clear charter, hand them work." icon:quartz
node $D say <argus-id> "Stand up the team. Create six bots with these names and charters, then tell me who they are and what each one owns. Forge — ships code … Sentry — reviews every PR … (one clause per bot)"
```

Give every bot a name that reads as a name (Argus, Forge, Sentry, Pulse, Echo,
Quill, Scout), not a job label. Watch it land:

```bash
sqlite3 /tmp/gravity-shot/daemon/bus.sqlite "select name from bot where deleted_at is null;"
grep "bot state" /tmp/gravity-shot/gravityd.log | tail -5   # all "ready — turn complete"
node $D peek <bot-id>                                       # what a terminal shows right now
```

Two fixes the sidebar needs before it looks right:

```bash
# 1. Previews read "system: You have just been cre…" because each bot's creation
#    note is newer than its first turn. Backdate the notes so the turn wins.
sqlite3 /tmp/gravity-shot/daemon/bus.sqlite \
  "UPDATE message SET created_at='2026-09-01T08:12:00+00:00' WHERE sender_name='system';"

# 2. The daemon looks for transcripts under the workspace path it stored (/tmp/…)
#    while Claude Code writes them under the resolved one (/private/tmp/…).
cd ~/.claude/projects && for d in -private-tmp-gravity-shot-daemon-*-workspace; do
  ln -sfn "$PWD/$d" "$PWD/${d#-private}"
done
```

Both only take effect after the client refetches, which means restarting the app
(or `touch apps/desktop/src/main.tsx` for an HMR reload). A reload wipes anything
typed in the terminal input, so do this before the final composition.

Finish by pinning the lead bot (right-click its row → Pin), selecting it, and
collapsing the right-hand info panel with the toggle in the bot header — the
screenshot wants the sidebar and the terminal, nothing else.

## Capturing

**Two processes are named `gravity-desktop`**: the installed app and the dev
build. Targeting by name grabs whichever AppleScript finds first, which has
already meant moving and capturing the user's real window. Always target the pid.

```bash
PID=$(pgrep -f "target/debug/gravity-desktop")
osascript -e "tell application \"System Events\" to tell (first process whose unix id is $PID) to perform action \"AXRaise\" of window 1"
osascript -e "tell application \"System Events\" to tell (first process whose unix id is $PID) to get {position, size} of window 1"
# -> e.g. 62, 55, 1580, 925 — feed those four numbers straight to -R
sleep 2 && screencapture -o -x -R 62,55,1580,925 /tmp/gravity-shot/shots/gravity-hero.png
sips -g pixelWidth -g pixelHeight /tmp/gravity-shot/shots/gravity-hero.png
```

`-R` takes points and writes retina pixels, so a 1580×925 window yields
3160×1850. That is the ceiling for this display — a bigger file means a bigger
window, not a flag. Read the PNG back before calling it done: `AXRaise` sometimes
loses the race with `screencapture` and you get Conductor instead.

## Pitfalls learned the hard way

- **A sandboxed `$HOME` breaks Claude Code auth** — every bot answers
  `Not logged in · Please run /login`. Symlinking `.claude` does not help. The
  daemon keeps the real home; only the app gets a fake one.
- **`say` needs the newline as a separate write.** One combined write reads as a
  paste and the Enter just adds a line; the driver already splits it.
- **`term.data` is plain UTF-8**, despite looking like it should be base64.
- Deleted bots keep their workspace directory, so a recreated `Forge` gets
  `forge-2` — re-run the transcript symlink loop after any reset.
- Long `say` prompts are worth a second look in `peek`: a bot mid-turn shows a
  spinner, which is how you tell "still thinking" from "never submitted".
- The bots answer honestly, including about the staging. One that had its charter
  rewritten mid-session said so in its summary. Either accept it or ask for a
  clean restatement before capturing.

## Cleanup

Tell the user what is being removed; the transcripts are in their real home.

```bash
kill $(pgrep -f "target/debug/gravity-desktop"); pkill -f "tauri dev"
pkill -f "gravityd --config /tmp/gravity-shot"
rm -rf /tmp/gravity-shot
rm -f ~/.claude/projects/-tmp-gravity-shot-*                 # the symlinks
rm -rf ~/.claude/projects/-private-tmp-gravity-shot-*        # the real transcripts
```

`~/.claude.json` also gains a trust entry per bot workspace. Harmless, but say so.
