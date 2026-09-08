# Contributing to Gravity

Report bugs and propose changes through [GitHub issues](https://github.com/ahilles107/gravity/issues).
Include the app and daemon versions, operating system, steps to reproduce, and
expected and actual behavior. Remove tokens, personal paths, and private project
content from logs and screenshots. See [SECURITY.md](SECURITY.md) for security
reports.

## Issue tracking

GitHub Issues is the source of truth for all Gravity work, including internal
maintainer tasks and follow-ups. Use this repository's issues instead of Linear
or `USE-XXX` identifiers. Search for an existing issue before opening one, and
reference it as `#123` in discussions and pull requests. Use `Fixes #123` when a
PR fully resolves the issue.

The same workflow applies to coding agents; see [AGENTS.md](AGENTS.md). Review
private backlog items for sensitive information before proposing any migration
to public issues. Security reports belong in the private channel described in
[SECURITY.md](SECURITY.md).

## Development setup

Use a current stable Rust toolchain, Node.js 22, and pnpm 11. Building the native
macOS app requires Xcode Command Line Tools. Official binaries target Apple
Silicon macOS; other platforms are not currently distributed. Docker is required
for the visual regression suite.

```sh
git clone https://github.com/ahilles107/gravity.git
cd gravity
pnpm --dir apps/desktop install --frozen-lockfile
pnpm --dir apps/marketing install --frozen-lockfile
cargo build --workspace
./scripts/dev.sh
```

Real bots require your own Claude Code installation and authentication. Use
`GRAVITY_RUNTIME=double ./scripts/dev.sh` to run the deterministic test runtime
without Claude credentials. Development state stays under `.dev/`.

Start with the [README](README.md), [architecture](docs/architecture.md), and
[protocol](docs/protocol.md). Optional telemetry, signing, and hosting configuration
is described in [public builds](docs/public-builds.md); no maintainer service
credentials are required to build or test.

## Validation

After installing the dependencies above, run the full checks from the repository
root before opening a pull request:

```sh
pnpm run verify
```

This runs Rust and native-shell checks, desktop and marketing typechecking,
linting, tests and builds, the file-length check, and visual regression tests.
It stops on the first failure. Full verification requires macOS and running Docker.

Visual baselines must be generated in the pinned CI container; use the
repository's visual regression workflow when accepting
intentional changes. Do not bypass hooks or checks to make a change pass.

## Pull requests

Keep changes focused and preserve strict TypeScript checks. Add tests for changed
behavior and update documentation where needed. Never commit credentials, local
configuration, daemon state, or `.context/` files.

Use conventional commit titles, such as `fix(desktop): reconnect after daemon restart`.
Target `main` and structure the description with **What**, **Why**, and
**How tested** sections. Link the relevant GitHub issue, list the exact checks
performed, and call out migrations, compatibility changes, and any checks you
could not run. Include screenshots for visible UI changes.
