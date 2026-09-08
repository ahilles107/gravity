---
name: release
description: Cut a new Gravity release — pick the version, land the version-bump PR, tag it, watch the release workflow, and verify the DMG, the gravityd tarball and any configured updater manifest were published. Use when someone asks to release, ship, cut, publish or tag a new version, bump the version, or when a `v*` release run needs monitoring or diagnosing.
---

# Releasing Gravity

A release is an annotated `v*` tag on `main`; use the configured signing
identity for official tags. Pushing that tag runs
`.github/workflows/release.yml` on a GitHub-hosted macOS ARM64 runner, which
builds the desktop app, packages `gravityd`, and creates the GitHub release.
Signing, notarization and updater artifacts are configurable; see
`docs/public-builds.md` for the required variables and secrets. Verify that
configuration before releasing. For an existing distribution, preserve the
updater signing pair. When changing download endpoints, retain a compatibility
bridge for every endpoint embedded in existing installations.
Publishing `latest.json` makes configured running apps offer the update.
Verify the intended repository and existing publication authorization before
pushing a tag;
do not infer access to official signing credentials in a contributor fork.

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
git describe --tags --abbrev=0 origin/main
# Use that tag as <previous-tag>; if no tag exists, review origin/main in full.
git log <previous-tag>..origin/main --oneline
```

Pre-1.0, so: **minor** for anything user-visible — a new feature, a removed
feature, a removed or renamed config key, a changed default. **Patch** only for
fixes and internals nobody can observe. State the reasoning in the bump
commit; that message is the release's rationale.

## 2. Land the bump PR

Never bump straight on `main` — it goes through a PR like any other change
with a What/Why/How tested description.

```bash
git switch -c release/vX.Y.Z origin/main
# edit the four manifests, then sync both lockfiles:
cargo update --workspace --offline
(cd apps/desktop/src-tauri && cargo update --workspace --offline)
```

`apps/desktop/pnpm-lock.yaml` also contains a `0.11.0` — that is the
`@xterm/addon-web-links` dependency, not ours. Leave it.

Commit as `chore(release): bump version to X.Y.Z`, open the PR against `main`
with the usual What/Why/How tested body and link the primary GitHub issue (`#123`). Before
pushing, run `pnpm run verify` (typecheck, tests, lint, builds and visual checks).
Regenerate dependency notices with `pnpm notices:generate` when versions or
lockfiles change; follow `third-party/README.md` for the pinned toolchain.
Wait for **all required PR checks**, including visual and secret scanning.
**Ask before merging** unless the user already said to — merging is what makes
the tag possible.

## 3. Tag the merge commit

Create an annotated tag; use your configured signing identity if tag signing
is enabled:

```bash
git fetch origin
git tag -a -m "Gravity X.Y.Z" vX.Y.Z <merge-commit-sha>
git push origin vX.Y.Z
```

Tag the actual merged commit on `origin/main`, not local `HEAD`, and
re-check the manifests in that commit first:

```bash
git show <sha>:Cargo.toml
git show <sha>:apps/desktop/src-tauri/Cargo.toml
git show <sha>:apps/desktop/package.json
git show <sha>:apps/desktop/src-tauri/tauri.conf.json
```

A pushed tag uploads all assets to a draft GitHub release, then publishes it as
latest. Existing R2 objects remain available, but new releases do not upload there.

## 4. Watch the run

```bash
gh run list --workflow release.yml --limit 3
gh run watch <run-id> --exit-status --interval 30
```

Build and notarization times vary. `gh run watch` may stop on a transient
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
gh release view vX.Y.Z --json name,url,assets
# Fetch the updater endpoint configured for this distribution, if enabled.
```

Expect the DMG and `gravityd` tarball. With updater signing enabled, also expect
`.app.tar.gz`, its `.sig`, and `latest.json`. Fetch the configured manifest and
confirm its version and GitHub artifact URL. Check the legacy manifest bridge
and website download redirect too, and verify updater signatures against the
existing public key. Keep the bridge for dormant installations.

Also check the release notes: the workflow appends a Gatekeeper/`xattr`
warning when notarization was unavailable. That is expected for builds without
Apple credentials; official notarized distributions should treat it as a failure.

## Dry runs

`workflow_dispatch` on `release.yml` builds from `tauri.conf.json`'s version
and only uploads artifacts to the run — no tag or GitHub release. Use
it to prove a build change before tagging.

## When a release goes wrong

Recovery is normally forward: fix, bump to the next patch, and tag again. Do not
delete or move a published tag or replace its signed archives — apps may already
have the manifest. If a bad release is latest, select the previous verified
GitHub release as latest to stop new clients discovering it. Point the legacy
bridge at that same version-specific manifest if needed. This stops future
update offers; it does not downgrade installed apps.

See `apps/marketing/README.md` for the bridge activation and rollback procedure.
Do not remove the old R2 custom domain or historical artifacts during recovery.
