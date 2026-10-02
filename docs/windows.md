# Windows build

The Windows contribution (#15) builds a native x64 desktop installer and daemon.
Install the stable MSVC Rust toolchain, Visual Studio C++ build tools with the
Windows SDK, Node.js 22, pnpm 11, and Python 3.11 or newer. The desktop uses
WebView2; the installer downloads its bootstrapper when needed.

From PowerShell at the repository root:

```powershell
pnpm --dir apps/desktop install --frozen-lockfile
pnpm --dir apps/marketing install --frozen-lockfile
./scripts/prepare-sidecar.ps1
pnpm --dir apps/desktop tauri build --bundles nsis
```

The installer is written under
`apps/desktop/src-tauri/target/release/bundle/nsis/`. Tauri automatically merges
`tauri.windows.conf.json`, which selects a per-user NSIS installer and native
window decorations. The Windows CI workflow lints and tests the daemon and native
shell on every pull request; it does not build the installer. The release
workflow builds the installer and runs the installer lifecycle test before
publishing. That test checks fresh install, same-version refresh, uninstall,
and uninstall after manual daemon removal, using an isolated test home with no
real bots.

The Windows installer installs and starts the bundled daemon under
`%USERPROFILE%\.gravity\bin\gravityd.exe` and registers a Task Scheduler task
for the current user. No separate daemon download or command is needed. Running
the installer again refreshes the managed daemon even when the app version has
not changed, restarting bot sessions while preserving projects and configuration.
It runs immediately and at user sign-in, without elevation,
and leaves bots running when the desktop closes. The task requires an interactive
user session; it does not run while that user is signed out. Logs, configuration,
and state live under `%USERPROFILE%\.gravity`; `GRAVITY_HOME` overrides this root.
The configured port defaults to 49777. If Windows reserves it (for example for
Hyper-V), the local managed daemon chooses an available port and the desktop
follows it. To use a fixed port, choose an available `port` in `gravityd.toml`
and restart the daemon. Remote clients and MCP URL allowlists must use the active
port. Direct
launches and remotely reachable daemons require an available configured port.

```powershell
# After installation:
& "$env:USERPROFILE/.gravity/bin/gravityd.exe" service status
& "$env:USERPROFILE/.gravity/bin/gravityd.exe" service restart
& "$env:USERPROFILE/.gravity/bin/gravityd.exe" service uninstall
```

The desktop uninstaller also stops and removes its managed daemon task.
Uninstalling keeps the database, configuration and workspaces; deleting
`%USERPROFILE%\.gravity` after uninstall gives a completely fresh setup.
The first-run wizard can still install a missing bundled daemon as a recovery step.
The desktop can also connect to a remote daemon using a device token.

Local Claude Code bots require an installed and authenticated native Claude Code
CLI. Cross-session messaging requires version 2.1.234 or later on native Windows
(2.1.248 or later for all providers). Gravity connects to the authenticated named
pipe reported by its SessionStart hook. PowerShell hooks serialize the pipe path
as JSON and report lifecycle events to the local daemon. A native Windows daemon
cannot use a Claude inbox inside WSL; install the native CLI for native bots.

For Codex bots, install and sign in to Codex CLI on the daemon host, then choose
**Bot info → Bot runtime → Codex CLI**. Gravity searches PATH, the usual npm/pnpm
and native Windows CLI locations, then the Codex desktop bundle. An explicit
`codex_bin` path in `gravityd.toml` overrides discovery. The Windows task can
have an older PATH than a newly opened terminal, so custom installation paths
should be configured explicitly. Missing CLIs are rejected before the current
bot session is stopped; later startup failures appear in the bot state and
terminal. Switching providers clears the previous provider's terminal screen.

The chat composer's microphone dictates with Windows' built-in SAPI desktop
recognizer, which runs entirely on the PC; audio is not sent to Microsoft or
Gravity. It uses the recognizer selected in Control Panel › Speech Recognition,
and the button stays hidden when none is installed. Desktop apps must be
allowed to use the microphone in Settings › Privacy & security › Microphone.
Windows' newer online dictation is not used, so the setting for Online speech
recognition does not matter.

For development without model credentials, set `runtime = "double"` in a private
`gravityd.toml` and run `cargo run -p gravityd -- --config <path>`. The double uses
an authenticated named pipe and exercises the same message-delivery path.

Run `pnpm run verify` before proposing a change. The visual suite uses the pinned
Linux Docker image; Docker Desktop must be running with Linux containers. Git for
Windows supplies Bash for the visual script. Set up the checkout with LF
line endings, as specified in `.gitattributes`, to match the formatting and notice
checks. The shared dependency notice inventory covers macOS ARM64 and Windows
x64; Windows release signing and publication require separate maintainer setup.

![Windows first-run setup](images/windows-setup.png)
