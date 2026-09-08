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

1. Build and start the daemon on a throwaway home:

   ```bash
   cargo build -p gravityd
   mkdir -p /tmp/gravityd-uidev
   cat > /tmp/gravityd-uidev/gravityd.toml <<'EOF'
   home = "/tmp/gravityd-uidev"
   port = 49555
   runtime = "double"
   supervision_interval_ms = 500
   EOF
   ./target/debug/gravityd --config /tmp/gravityd-uidev/gravityd.toml > /tmp/gravityd-uidev/gravityd.log 2>&1 &
   ```

   The client token appears at `/tmp/gravityd-uidev/secrets/client.token`.

2. Start the frontend: `cd apps/desktop && pnpm dev` (port 1420, strict).

3. Open `http://localhost:1420/` in Chrome using the claude-in-chrome tools
   (the `chrome-devtools-axi` bridge has proven flaky for eval/snapshot — use
   the extension tools). Configure the app via the javascript tool, then
   reload:

   ```js
   localStorage.setItem('gravity.connection', JSON.stringify({host:'127.0.0.1', port:49555}));
   localStorage.setItem('gravity.device-token', '<contents of client.token>');
   localStorage.setItem('gravity.setup-complete', 'true');
   location.reload();
   ```

4. Seed bots and data: `node .claude/skills/terminal-perf-test/scripts/drive.mjs setup`
   Creates project "perf" with bots turbo1–turbo5; turbo1 gets ~2.3 MiB
   (forces the trimmed non-resumable replay), the others ~300 KiB. The script
   prints the bot ids — keep them for stream mode.

## Verification checklist

Run each check; all must hold. Read console errors after every phase
(`read_console_messages` with `onlyErrors`) — zero exceptions expected
throughout.

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
   frame well under 100 ms. Baseline on a 120 Hz display (2026-08): 120 fps
   avg, worst 28 ms, 0 long tasks over 52 s.

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

```bash
pkill -f "target/debug/gravityd"; pkill -f "node.*vite"
rm -rf /tmp/gravityd-uidev
```

Close the Chrome tab, and revert any temporary A/B edits.
