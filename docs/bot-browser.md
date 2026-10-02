# Each bot's own browser

Status: implemented in the daemon (`crates/gravityd/src/browser/`) and the
desktop app (the bot's Browser tab). Tests: `crates/gravityd/tests/agent_browser.rs`.

## Why

Bots used to browse through the owner's own Chrome. Claude Code's Claude in
Chrome integration follows the user's global setting
(`claudeInChromeDefaultEnabled` in `~/.claude.json`), and bot sessions run as
the user, so every bot drove the one Chrome the owner was using. Tabs opened
there with nothing saying which bot opened them, or why.

## What a bot gets

- **A browser of its own.** Each bot session runs the
  [Playwright MCP](https://github.com/microsoft/playwright-mcp) server as
  `playwright`, against a Chrome whose profile lives in the bot's directory
  (`<bot>/browser/profile`). Logins persist between sessions and are the bot's
  alone. It runs headless, so no windows appear on the desktop.
- **The same reach as before.** The tools cover navigating, reading the page,
  clicking, typing, forms, tabs (`browser_tabs`: list, open, select, close),
  screenshots, the console and the network. The `vision` capability adds mouse
  tools that act at coordinates on a screenshot, the equivalent of Claude in
  Chrome's `computer` tool. Nothing is taken away: the server is added
  alongside the bot's other MCP servers (no `--strict-mcp-config`).
- **The owner's Chrome only when allowed.** Bots start with `--no-chrome`. A
  per-bot switch, "Can use your Chrome" in the bot's Info panel
  (`set_bot_user_chrome`), starts it with `--chrome` instead, for a task that
  needs the owner's logged-in sessions. Changing it restarts the session. The
  system prompt tells the bot which browsers it has, and to prefer its own.

Claude Code and Codex bots both get the browser: Claude Code through the
session's `mcp.json`, Codex through its `mcp_servers` config.

## Watching it

The bot's **Browser** tab shows:

- **Its tabs.** The view follows whichever tab the bot used last. Clicking
  another tab shows that one instead, and "Follow bot" goes back to following.
- **The page, live.** The daemon streams screencast frames of the tab on show.
- **Activity.** Every browser action the bot took, newest first, grouped under
  the request that started its turn ("Task from lead: compare the pricing
  tiers"). Actions in the owner's Chrome are marked "your Chrome".

The app starts watching as soon as a bot is selected, whatever tab is open, so
the Browser tab is live when opened; its label shows a red dot while the bot's
browser is open.

**Several viewers, one stream.** Every client watching the same bot (the
desktop app and a phone, say) reads one shared stream: one tab list poll per
browser and one screencast per tab, however many watch. A viewer that joins
gets the current tabs and screen at once, and a slow one skips to the newest
frame rather than falling behind. The stream stops when the last viewer
leaves. A phone uses the same requests as the desktop: `watch_browser` when a
bot is selected, `unwatch_browser` when it leaves.

How the daemon finds the browser: Chrome is launched with
`--remote-debugging-port=0` and writes the port it chose to `DevToolsActivePort`
in the profile. The daemon reads that file, lists the tabs over the DevTools
HTTP endpoint, and attaches to the tab on show for `Page.startScreencast`.
Streaming happens only while some client watches that bot.

## Configuration

`[browser]` in `gravityd.toml`:

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Give every bot a browser of its own. |
| `node_dir` | found | The directory holding `node` and `npx`. The daemon looks on PATH, then in mise, nvm, asdf, fnm, Volta, Homebrew and, on Windows, Program Files, scoop and nvm-windows. Set it if Node lives elsewhere. |
| `channel` | found | `chrome`, `msedge` or `chromium`: Chrome when installed, else Edge, else Playwright's own Chromium. |
| `headless` | `true` | Set `false` to see the bots' browser windows on the desktop. |
| `package` | `@playwright/mcp@0.0.83` | The npm package serving the tools, pinned. |

Without Node the bot starts without a browser of its own, and the daemon logs
a warning. `npx` downloads the pinned package on first use.

## A linked bot's browser

A linked bot's browser runs on its own machine, and the daemon a client is
connected to relays it (`peer_browser`): its Browser tab works as for a local
bot, from the desktop or a phone. The bot's machine watches the browser for
the peer through its shared stream (`browser_watch`) and forwards the pushes as
`browser_feed` events: tab lists at once, frames newest only and at most a
few a second, so a slow link never queues them. The other daemon keeps one
feed per stand-in and tab choice, shared by its viewers, and renames the bot
to the stand-in. When the link drops, viewers are told the machine is
offline, and the feeds resume when it is back. The activity log is read from
the bot's machine (`list_browser_activity` on the stand-in).
