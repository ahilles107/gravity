---
name: visual-regression
description: Work with the Ladle visual-regression check — read a failing `visual` CI run, decide whether a pixel diff is a regression or an intended redesign, and adopt CI-produced baselines into the PR. Use when the `visual` workflow fails, when a PR needs updated component snapshots, when adding or changing a `*.stories.tsx` file, or when someone asks to update/refresh/accept UI snapshots.
---

# Visual regression for Ladle stories

The `visual` workflow screenshots every Ladle story and compares it byte for
byte against a committed baseline in
`apps/desktop/tests/visual/__screenshots__/`. One PNG per story id.

## The rule that governs everything here

**Never generate a baseline locally.** They are only reproducible inside the
pinned Playwright container that CI uses (`mcr.microsoft.com/playwright` — the
tag lives in `scripts/vr-ci.sh`). A macOS or bare-Linux checkout renders
different glyphs and antialiasing, so a locally produced PNG is guaranteed to
fail on the next push while looking correct in the diff.

`playwright.config.ts` enforces this: `--update-snapshots` throws unless
`CI=true`, and `updateSnapshots: "none"` makes a missing baseline a hard
failure rather than a silent write. Do not work around either guard.

## Adopting new baselines

When `visual` fails, the job regenerates the complete baseline set inside the
container and uploads it as the `visual-snapshots` artifact. Adopt it with:

```bash
scripts/vr-accept.sh            # newest `visual` run for the current branch
scripts/vr-accept.sh 1234567890 # or a specific run id
```

It downloads the artifact, replaces `__screenshots__` wholesale (so baselines
for deleted stories disappear too), and stages the result. Then commit:

```
test(desktop): update story baselines for <what changed>
```

The script warns when the run's `headSha` differs from local `HEAD`. If it
does, push first and re-run — otherwise you commit baselines describing a
different tree.

## Deciding first, adopting second

`vr-accept.sh` is mechanical; the judgement is yours. Before running it, look
at what actually changed:

```bash
gh run download <run-id> --name visual-report --dir /tmp/vr-report
```

The Playwright HTML report has expected/actual/diff for every failure. Read
the diffs and classify:

- **Intended** — the PR changes that component, spacing, or token. Adopt.
- **Unintended blast radius** — a shared token or a rule in `src/styles/`
  moved something the PR never meant to touch. Fix the CSS, don't adopt.
- **Non-determinism** — the same story flips between runs. Adopting hides it.
  Find the source (a live `Date`, an unpinned locale, a loading font, an
  animation) and make the story deterministic instead.

Say which category each diff falls into when you report back. "Snapshots
updated" without that judgement is not a review.

## Adding a story

New stories have no baseline, so the first CI run after adding one fails with
`snapshot doesn't exist`. That is the expected bootstrap: push, let it fail,
run `scripts/vr-accept.sh`, commit the new PNGs.

## Determinism requirements for stories

The suite pins `timezoneId: "UTC"`, `locale: "en-US"`, a 1024x720 viewport and
`deviceScaleFactor: 1`. Stories must supply the rest:

- Use the fixed fixtures in `src/test/fixtures.ts`. Their timestamps are frozen
  at 2024-05-01, which keeps `fmtTimestamp`/`fmtShortTime` off their
  `isToday()` branch. A story that builds a date from `new Date()` will pass
  once and fail forever after.
- No randomness, no network, no `setTimeout`-driven reveal.
- Keep the rendered component inside the viewport; content below the fold is
  not captured.

## Running the comparison yourself

You can run the real check locally if Docker is available — this uses the same
container as CI, so it is safe:

```bash
scripts/vr-ci.sh
```

What you must not do is `pnpm vr:update` on your machine. `pnpm vr` alone
(outside the container) will also report false diffs; it exists for the
container to call.
