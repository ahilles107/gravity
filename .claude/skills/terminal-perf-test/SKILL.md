---
name: terminal-perf-test
description: Verify terminal rendering performance and correctness (WebGL renderer, bot-switch terminal cache, forced-repaint nudge, replay bottom-pin) by running the real desktop app in Chrome against a local gravityd with the double runtime, seeding heavy ANSI output, and stress-testing switching/scrolling/streaming. Use when changing TerminalPane, terminalCache, the daemon's attach/replay/resize path, or investigating terminal CPU, lag, black screens, or rendering artifacts.
---

# Terminal rendering performance test

Runs the real stack locally — `gravityd` with the deterministic `double` runtime, the
Vite-served desktop frontend in Chrome — then seeds terminals with more output
than the daemon's 1 MiB scrollback ring and stress-tests the paths that have
historically broken: bulk replay after eviction, rapid bot switching, live
streaming, scrolling under load.

Why the double runtime: it echoes every `input` byte back as terminal output
(so a driver script can pump arbitrary ANSI at any rate), and it prints a
`[resize CxR]` marker for every pty resize — making the forced-repaint nudge
(shrink to rows-1, restore ~50 ms later; see `REPAINT_NUDGE_DELAY` in
`crates/gravityd/src/supervisor.rs`) visible on screen.

## Setup

Use Node 22+ for the driver and the prerequisites in `CONTRIBUTING.md`.
Run from the repository root in a dedicated terminal. Verify test ports 49555
and 1420 are free first; choose unused ports consistently if needed.

```bash
umask 077
PERF_DIR=$(mktemp -d "${TMPDIR:-/tmp}/gravity-perf.XXXXXX")
PERF_DIR=$(cd "$PERF_DIR" && pwd -P)
export GRAVITY_HOME="$PERF_DIR"
export GRAVITY_WS=ws://127.0.0.1:49555/ws
cargo build -p gravityd
cat > "$GRAVITY_HOME/gravityd.toml" <<EOF
home = "$GRAVITY_HOME"
bind = ["127.0.0.1"]
port = 49555
negotiate_port = false
runtime = "double"
supervision_interval_ms = 500
EOF
./target/debug/gravityd --config "$GRAVITY_HOME/gravityd.toml" > "$PERF_DIR/gravityd.log" 2>&1 &
PERF_DAEMON_PID=$!
```

Wait for `$GRAVITY_HOME/gravityd.port`, confirm it is 49555 and the recorded
PID is still running, then check `curl --fail http://127.0.0.1:49555/health`.
Stop on startup failure. Do not connect the driver to an installed daemon or
use it with `pty`: its generated input belongs only in the echo runtime.

Start the frontend in this shell, with shell tracing off. The dev token stays
in the local process environment; do not print it or paste it into browser
commands or chat.

```bash
export VITE_GRAVITY_DEV_PORT=49555
export VITE_GRAVITY_DEV_TOKEN="$(cat "$GRAVITY_HOME/secrets/client.token")"
(cd apps/desktop && env -u CONDUCTOR_PORT pnpm dev)
```

Open `http://localhost:1420/` with available browser automation in a fresh test
profile. Stored connection settings override these defaults: confirm the footer
shows `127.0.0.1:49555 · connected` before interacting. Never expose or deploy
the dev frontend containing this token.

In another terminal, set `GRAVITY_HOME` to this run's exact directory and
`GRAVITY_WS` to the matching loopback URL, then seed:

```bash
node .claude/skills/terminal-perf-test/scripts/drive.mjs setup
```

This creates project "perf" and turbo1–turbo5. Turbo1 exceeds the 1 MiB ring;
the others receive smaller histories. Keep the printed bot IDs for stream mode.

## Verification checklist

Run each check; all must hold. Read browser console errors after every phase using the available tools;
zero exceptions are expected throughout.

1. **WebGL renderer active** — in the page:
   ```js
   const host = document.querySelector('.terminal-host');
   ({ canvases: host.querySelectorAll('canvas').length,      // > 0
      domRows: host.querySelector('.xterm-rows') === null }) // true: no DOM renderer
   ```

2. **Bulk replay paints and lands pinned** — select turbo1 (first visit =
   full replay of the ring). The transcript must be visible without scrolling
   and end at the newest line:
   ```js
   const vp = document.querySelector('.xterm-viewport');
   vp.scrollHeight - vp.clientHeight - vp.scrollTop   // 0 once replay settles
   ```
   A screenshot mid-replay legitimately shows the tail still streaming;
   re-check after a few seconds before calling it a failure.

3. **Cache reuse on switch-back** — tag the terminal's element, switch to
   another bot and back; the same node must return and no `term.reset` path
   should run (screen identical, no repaint flash, no new `[resize ...]`
   shrink marker):
   ```js
   document.querySelector('.terminal-host').firstElementChild.dataset.perfTag = 'x';
   // switch away and back, then:
   document.querySelector('.terminal-host').firstElementChild.dataset.perfTag  // 'x'
   ```

4. **Eviction survives** — visit 4+ other bots (cache holds 3), then return
   to turbo1. Expect: full replay again (fresh node, tag gone), content
   pinned at the bottom, and a nudge marker pair like
   `[resize 163x45][resize 163x46]` appended — shrink and restore observed by
   the runtime as two separate sizes. No console errors: eviction disposes a
   parked WebGL terminal, which is the path that once crashed the whole app.

5. **Live-stream stress with instrumentation** — install the meter, then
   `node .../drive.mjs stream <turbo1-id> 20`, and during it scroll up
   (view must hold position while streaming), switch away and back:
   ```js
   window.__perf = { frames: 0, longTasks: 0, worstFrame: 0, start: performance.now(), last: performance.now() };
   const tick = () => { const now = performance.now(); const d = now - window.__perf.last;
     if (d > window.__perf.worstFrame) window.__perf.worstFrame = d;
     window.__perf.last = now; window.__perf.frames++; requestAnimationFrame(tick); };
   requestAnimationFrame(tick);
   new PerformanceObserver(l => { for (const e of l.getEntries()) window.__perf.longTasks++; })
     .observe({ entryTypes: ['longtask'] });
   ```
   Read afterwards: avg fps ≈ display refresh rate, `longTasks` 0, worst
   frame well under 100 ms. Record the display refresh rate, hardware and elapsed time; compare with
   an unchanged build on the same machine. Reload afterwards to stop the meter.

6. **Rapid-switch storm** — 25 programmatic clicks across all bots at 120 ms
   intervals (dispatch mousedown/mouseup/click on the sidebar rows from the
   javascript tool). Expect a fully painted, pinned terminal and zero
   console errors at the end.

## Pitfalls learned the hard way

- Vite/esbuild strips comments — never verify which code the page runs by
  fetching source and searching for a comment; search for a code token.
- After a React tree crash, HMR cannot revive it and a stale module graph may
  linger: always `location.reload()` before re-testing a fix.
- Chrome/Blink hides the DOM-renderer cost at this terminal size; the CPU win
  matters on WKWebView (prod). For renderer A/B evidence, comment out the
  `loadWebgl(term)` call in `TerminalPane.tsx` temporarily — and put it back.
- The final prod check is still a release build on macOS: WKWebView is a
  different engine than the Chrome used here.

## Cleanup

Stop the foreground Vite session with Ctrl-C. Stop only the daemon started in
this live shell (`kill "$PERF_DAEMON_PID"; wait "$PERF_DAEMON_PID"`). If the
session was lost, verify process ownership again before signalling. Do not use
broad process-name matching. Unset the temporary Vite token environment.

Close the test browser profile. Retain the run directory for review and ask
before deleting its exact path. Restore only your temporary A/B edits, preserving
any pre-existing changes.
