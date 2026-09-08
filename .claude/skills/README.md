# Contributor agent skills

These optional workflows help coding agents work on Gravity. They are not
required to build or run the app. Start with `CONTRIBUTING.md` for dependencies
and verification; use whatever terminal, browser or native-window tools your
agent environment provides.

Follow `AGENTS.md` for issue tracking: internal and contributor work both use
this repository's GitHub Issues, not Linear.

| Skill | Purpose | Requirements beyond the contributor setup |
| --- | --- | --- |
| `bus-live-test` | Exercise bot messaging and guardrails | Node 22+; real Claude sessions require consent and an authenticated CLI |
| `terminal-perf-test` | Exercise terminal replay and rendering | Node 22+, browser automation; synthetic runtime only |
| `marketing-screenshot` | Capture a staged native app window | macOS, Node 22+, window capture access; consent for billed sessions |
| `visual-regression` | Review and adopt CI screenshots | GitHub CLI access to the relevant workflow artifacts |
| `release` | Prepare and verify a release | Maintainer authorization and access to the intended repository's release configuration |

Local drivers require an explicit disposable daemon home, a matching published
port and a loopback endpoint. They reject the standard installed daemon home
and port. These checks are guardrails, not a sandbox: use only a daemon started
for the current test. Keep credentials, real transcripts and local agent
settings out of commits and shared tool output. Leave cleanup of existing user
data to the user unless explicitly authorized.

Shared driver checks live in `lib/` and run in CI and `pnpm run verify`. To run
just those checks:

```bash
node --test .claude/skills/lib/*.test.mjs
```
