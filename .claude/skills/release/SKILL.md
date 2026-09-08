---
name: release
description: Cut a new Gravity release — pick the version, land the version-bump PR, tag it, watch the release workflow, and verify the DMG, the gravityd tarball and any configured updater manifest were published. Use when someone asks to release, ship, cut, publish or tag a new version, bump the version, or when a `v*` release run needs monitoring or diagnosing.
---

# Releasing Gravity

A release is a signed `v*` tag on `main`. Pushing that tag runs
`.github/workflows/release.yml` on a GitHub-hosted macOS ARM64 runner, which
builds the desktop app, packages `gravityd`, and creates the GitHub release.
Signing, notarization, updater artifacts, and the R2 mirror are optional; see
`docs/public-builds.md` for the required variables and secrets. Verify that
configuration before releasing. For an existing distribution, preserve the
updater signing pair and download endpoint so installed clients still update.
Publishing `latest.json` makes configured running apps offer the update.

## The rule that governs everything here

**The version lives in six files and the workflow refuses to build unless they
all agree with the tag.** The `Resolve version` step compares the tag name
(minus `v`) against `Cargo.toml`, `apps/desktop/src-tauri/Cargo.toml`,
`apps/desktop/package.json` and `apps/desktop/src-tauri/tauri.conf.json`; the
two `Cargo.lock` files must follow or the build dirties them. The desktop crate
sits outside the root workspace, so nothing propagates a version for you.

## 1. Pick the version

Look at what landed since the last tag — that is the whole changelog:

```bash
git fetch origin --tags
git log $(git describe --tags --abbrev=0)..origin/main --oneline
```

Pre-1.0, so: **minor** for anything user-visible — a new feature, a removed
feature, a removed or renamed config key, a changed default. **Patch** only for
fixes and internals nobody can observe. State the reasoning in the bump
commit; that message is the release's rationale.

## 2. Land the bump PR

Never bump straight on `main` — it goes through a PR like any other change
with a What/Why/How tested description.

```bash
git checkout -b release/v0.12.0
# edit the four manifests, then sync both lockfiles:
cargo update --workspace --offline
(cd apps/desktop/src-tauri && cargo update --workspace --offline)
```

`apps/desktop/pnpm-lock.yaml` also contains a `0.11.0` — that is the
`@xterm/addon-web-links` dependency, not ours. Leave it.

Commit as `chore(release): bump version to X.Y.Z`, open the PR against `main`
with the usual What/Why/How tested body, and wait for `checks` and `tests`.
**Ask before merging** unless the user already said to — merging is what makes
the tag possible.

## 3. Tag the merge commit

Create an annotated tag; use your configured signing identity if tag signing
is enabled:

```bash
git fetch origin
git tag -a -m "Gravity 0.12.0" v0.12.0 <merge-commit-sha>
git push origin v0.12.0
```

Tag the actual squash-merge commit on `origin/main`, not local `HEAD`, and
re-check the manifests in that commit first:

```bash
git show <sha>:Cargo.toml | sed -n '6p'
```

A pushed tag publishes to GitHub releases and any configured mirror.

## 4. Watch the run

```bash
gh run list --workflow release.yml --limit 3
gh run watch <run-id> --exit-status --interval 30
```

Expect **~25 minutes**. `gh run watch` sometimes dies mid-stream on a transient
`HTTP 404` from the jobs API — that says nothing about the run. Fall back to
polling in the background rather than assuming failure:

```bash
for i in $(seq 1 60); do
  s=$(gh run view <run-id> --json status,conclusion -q '.status+" "+.conclusion')
  case "$s" in completed*) echo "$s"; break;; esac
  sleep 30
done
```

`Notarize DMG` is the long step — Apple's service, not the runner.

## 5. Verify the publish, don't trust the green check

```bash
gh release view v0.12.0 --json name,url,assets
# Fetch the updater endpoint configured for this distribution, if enabled.
```

Expect the DMG and `gravityd` tarball. With updater signing enabled, also expect
`.app.tar.gz`, its `.sig`, and `latest.json`. Fetch the configured manifest and
confirm its version and artifact URL. If R2 is enabled, check that mirror too.

Also check the release notes: the workflow appends a Gatekeeper/`xattr`
warning when notarization was unavailable. That is expected for builds without
Apple credentials; official notarized distributions should treat it as a failure.

## Dry runs

`workflow_dispatch` on `release.yml` builds from `tauri.conf.json`'s version
and only uploads artifacts to the run — no tag, no R2, no GitHub release. Use
it to prove a build change before tagging.

## When a release goes wrong

Versioned R2 objects are immutable and `latest.json` is `no-cache`, so the
recovery is always forward: fix, bump to the next patch, tag again. Do not
delete or move a published tag — apps may already have the manifest. If a bad
`latest.json` is live and the fix will take a while, re-put the previous
version's manifest to stop the rollout:

```bash
pnpm --dir apps/marketing exec wrangler r2 object put "$R2_BUCKET/desktop/gravity/latest.json" \
  --file latest.json --content-type application/json --cache-control no-cache --remote
```
