# Gravity

Always-on, multi-bot desktop app with Claude Code as the agent runtime. A Rust
daemon (`gravityd`) runs on a Mac mini; the Tauri desktop client attaches
from any machine. See the [architecture](docs/architecture.md) for the system
design and the [control-plane protocol](docs/protocol.md) for client APIs.

Builds and tests require no Linear, PostHog, or Cloudflare account. Running real
bots requires your own Claude Code installation and authentication; the test
runtime works without it. See [public-build configuration](docs/public-builds.md)
for telemetry, releases, and secret scanning.

## Layout

```
crates/bus/       shared types, SQLite schema, envelope format
crates/gravityd/      daemon: runtime adapters, durable delivery, scheduler,
                  MCP bridge, WebSocket control plane
apps/desktop/     Tauri v2 + React + xterm.js client
docs/             architecture, WS protocol v2, public-build configuration
ops/              launchd plist + example gravityd.toml
scripts/          repository checks (file-length rule)
```

## Build & test

After installing the desktop and marketing dependencies, run `pnpm run verify`
from the repository root for the full verification suite (macOS and Docker
required). See [CONTRIBUTING.md](CONTRIBUTING.md) for setup.

```sh
# Daemon
cargo build --release            # target/release/gravityd
cargo test --workspace           # unit + integration (uses the runtime double)
cargo clippy --workspace --all-targets
cargo fmt --all --check

# Desktop client
cd apps/desktop
pnpm install
pnpm typecheck && pnpm build     # frontend
../../scripts/prepare-sidecar.sh # stage gravityd as the bundled sidecar (once,
                                 # and after daemon changes)
pnpm tauri dev                   # run the app
```

## Code rules & tooling

**No source file may exceed 400 lines.** Split by concern instead of growing a
file: `crates/gravityd/src/db/`, `crates/gravityd/src/ws/` and `apps/desktop/src/app/`
are the reference examples.

```sh
./scripts/check-file-length.sh   # enforces the limit for Rust + CSS
```

TypeScript is covered by oxlint's `max-lines` rule instead, so the limit is
checked in two places for two toolchains.

The desktop client carries the JS/TS toolchain; `pnpm check` runs the whole
gate and is what CI (and a pre-push hook, if you add one) should call:

```sh
cd apps/desktop
pnpm check          # typecheck + format + lint + knip + tests-with-coverage + fallow
```

| Command | Tool | What it enforces |
| --- | --- | --- |
| `pnpm typecheck` | tsc | strict types, no emit |
| `pnpm format:check` / `pnpm format` | [oxfmt](https://oxc.rs) | formatting (`.oxfmtrc.json`) |
| `pnpm lint` / `pnpm lint:fix` | [oxlint](https://oxc.rs) | correctness, suspicious, perf, React/a11y, `max-lines: 400` (`.oxlintrc.json`) |
| `pnpm knip` | [knip](https://knip.dev) | unused files, exports and dependencies (`knip.json`) |
| `pnpm test` / `pnpm test:watch` | [vitest](https://vitest.dev) + jsdom + Testing Library | unit and component tests (`vitest.config.ts`) |
| `pnpm test:coverage` | vitest + v8 | writes `coverage/coverage-final.json` |
| `pnpm fallow` | [fallow](https://fallow.tools) | dead code, duplication, complexity and CRAP thresholds (`fallow.config.json`) |

### Tests and CRAP

`fallow health` scores untested complexity as CRAP (`CC² × (1 − coverage)³ + CC`),
so it only passes with a real coverage report: `fallow.config.json` points
`health.coverage` at `coverage/coverage-final.json`, and `pnpm check` runs
`test:coverage` before `fallow`. Running `pnpm fallow` on its own after changing
code will read a stale report — run `pnpm test:coverage` first.

Components are tested against `FakeDaemon` (`src/test/fakeDaemon.ts`), an
in-memory implementation of the `DaemonApi` interface in `src/protocol/api.ts`.
That interface exists so nothing in the UI depends on the concrete
`DaemonClient`, and so the double needs no type casts. `DaemonClient` itself is
tested against a scripted `WebSocket` (`src/test/fakeWebSocket.ts`).

### Pre-commit hook

`.githooks/pre-commit` runs oxfmt, oxlint and fallow's coverage-independent
checks (`dead-code`, `dupes`) whenever a commit touches
`apps/desktop/`. Enable it once per clone (it is a native git hook — no
dependency, and `core.hooksPath` is relative so it covers every worktree):

```sh
git config core.hooksPath .githooks
```

`fallow health` is deliberately left out of the hook: it needs a fresh coverage
report, and regenerating one on every commit is too slow. `pnpm check` covers it.

The hook inspects the working tree rather than the index, so a partial
staged-hunk commit is checked against the files as they are on disk. Resolve
validation failures before committing.

## Install (end users)

Open the release DMG and drag **Gravity** to Applications. The app bundles
the daemon as a signed sidecar; on first launch a setup wizard either installs
it as a launchd user agent on this Mac (one click, no sudo) or attaches to a
remote daemon with a device token. Headless machines skip the app:
`./gravityd service install` from the release tarball does the same
install, and `service status` / `service restart` / `service uninstall` manage
it.

A daemon already answering on `49777` stops the install: Gravity refuses to
start rather than run a second one against the same state. If something else —
not a daemon — holds `49777`, the app-managed service waits a few seconds, then
serves on a free port instead and the app follows it there. That moves the bot
bus off the allowlisted `http://127.0.0.1:49777/mcp` URL, which a policy-managed
Mac silently drops, so Settings → Connection flags it and the daemon restarts
onto `49777` as soon as it comes free. Set `negotiate_port = false` in
`gravityd.toml` to get a daemon that refuses to start instead.

## Run the whole stack (development)

`scripts/dev.sh` starts a workspace-private daemon and the client against it —
nothing touches `~/.gravity`, so it can run beside the installed production
daemon and beside other checkouts:

```sh
./scripts/dev.sh                     # gravityd + the real Tauri window
./scripts/dev.sh web                 # gravityd + Vite frontend in the browser
GRAVITY_RUNTIME=double ./scripts/dev.sh # deterministic echo runtime, no tokens spent
```

The daemon gets its own home at `.dev/gravityd/` (gitignored) with a generated
`gravityd.toml`. Ports derive from `CONDUCTOR_PORT` when Conductor sets it —
frontend on `CONDUCTOR_PORT`, daemon on `CONDUCTOR_PORT+1`, defaulting to
1420/1421 — so parallel workspaces do not collide. In browser mode the script
prints the `localStorage` snippet that points the page at that daemon; the
Tauri window picks the daemon up from `GRAVITY_HOME`.

Conductor drives the same script from `.conductor/settings.toml`: the **dev**,
**dev-double**, **app** and **test** run scripts, with `setup` installing the
frontend dependencies.

## Run the daemon (development)

```sh
mkdir -p ~/.gravity
cp ops/gravityd.example.toml ~/.gravity/gravityd.toml   # optional; defaults are sane
cargo run -p gravityd                                   # or: gravityd --config <path>
curl http://127.0.0.1:49777/health
```

On first start the daemon generates `~/.gravity/secrets/client.token`
(mode 0600), which the desktop client reads automatically on the same machine.

For a remote client (laptop over Tailscale), do **not** copy the owner token.
Create a device-scoped credential instead — Devices panel in the app, or
`create_device` over the protocol — choose `read` and/or `control` grants,
and enter the one-time token plus the Tailscale host/port in the remote app's
connection settings. Revoking the device immediately prevents reconnection.

### Backup & restore

```sh
gravityd backup                       # ~/.gravity/backups/backup-<timestamp>
gravityd backup --out /path/to/dir
gravityd restore --from /path/to/dir  # refuses if a db exists
gravityd restore --from /path/to/dir --overwrite   # moves current db aside first
```

Backups use SQLite's online backup API and include configuration manifests
(project/bot config files) but never secrets or bot workspaces. Retention
pruning (messages, deliveries, routine runs; FTS index kept in sync) runs
daily by default — see `[retention]` in `gravityd.toml`.

## Mac mini service

```sh
cargo build --release
./target/release/gravityd service install   # binary → ~/.gravity/bin, launchd user agent
sudo pmset -a sleep 0 disablesleep 1    # keep the mini awake
```

`service install` copies the invoked binary to `~/.gravity/bin/gravityd`,
writes `~/.gravity/gravityd.toml` if missing, and bootstraps the
`in.mikolajczuk.gravityd` launchd agent — rerun it after a rebuild to upgrade in
place. `service restart` bounces that agent without touching the install, which
kills every running bot session mid-turn; Settings → Connection offers the same
thing behind a confirmation. `ops/in.mikolajczuk.gravityd.plist` remains for
fully manual setups.

## How it works, briefly

- Each **bot** is a provisioned directory (`~/.gravity/projects/<p>/bots/<b>`)
  with `system.md`, `mcp.json`, and a workspace containing `CLAUDE.md` and
  cooperative `.claude/settings.json` (permission rules + lifecycle hooks that
  report state to the daemon). The runtime adapter spawns `claude` there in a
  PTY; the client renders it with xterm.js, preserving native prompts.
- **Messages** are durable rows before delivery. A worker leases due
  deliveries, posts rendered envelopes (`[msg #42 from BOB · task] …`)
  to the session's inbox socket, retries with backoff, and surfaces terminal
  failures. Bots consume and acknowledge via the `gravity-bus` MCP server
  (`send_message`, `complete_task`, `check_inbox`, …) with per-bot scoped
  tokens.
- **Bot-to-bot sends** create one task + one delivery per message, with hop
  counts and origin chains for loop prevention.
- **Routines** use cron, interval, or named-signal triggers with unique
  occurrences, leases, overlap policies, and catch-up on restart. Bots can
  create and manage their own routines, which are enabled on creation.
- The **WS control plane** (protocol v2) is authenticated, versioned, and
  reconnectable with terminal replay cursors. Multiple clients can watch a bot;
  clients with the `control` grant can type and resize its terminal.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, validation, and pull requests.

## License

Gravity is licensed under the [MIT License](LICENSE). Third-party dependencies
retain their respective licenses.
