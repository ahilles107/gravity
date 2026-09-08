#!/usr/bin/env bash
#
# Adopts the visual baselines produced by a failed `visual` CI run.
#
# Snapshots are never generated locally — a developer machine renders
# different glyphs than the pinned CI container, so a locally produced PNG
# fails CI on the very next push. When the `visual` job fails it regenerates
# the full baseline set inside that container and uploads it as the
# `visual-snapshots` artifact; this script downloads that set and stages it.
#
# Usage: scripts/vr-accept.sh [run-id]
#   run-id  a specific `visual` run to adopt. Defaults to the newest run for
#           the current branch.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

SNAPSHOT_DIR="apps/desktop/tests/visual/__screenshots__"
ARTIFACT="visual-snapshots"

command -v gh >/dev/null || {
  echo "gh CLI is required: https://cli.github.com" >&2
  exit 1
}

run_id="${1:-}"
if [ -z "$run_id" ]; then
  branch="$(git rev-parse --abbrev-ref HEAD)"
  run_id="$(gh run list --workflow visual.yml --branch "$branch" --limit 1 \
    --json databaseId --jq '.[0].databaseId')"
  if [ -z "$run_id" ] || [ "$run_id" = "null" ]; then
    echo "No 'visual' workflow run found for branch $branch. Push first." >&2
    exit 1
  fi
fi

run_sha="$(gh run view "$run_id" --json headSha --jq .headSha)"
head_sha="$(git rev-parse HEAD)"
if [ "$run_sha" != "$head_sha" ]; then
  echo "Warning: run $run_id built $run_sha but HEAD is $head_sha." >&2
  echo "Baselines will describe the pushed commit, not your working tree." >&2
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $ARTIFACT from run $run_id..."
gh run download "$run_id" --name "$ARTIFACT" --dir "$tmp"

if ! find "$tmp" -name '*.png' -print -quit | grep -q .; then
  echo "Artifact $ARTIFACT contained no PNGs — did the job fail before the suite ran?" >&2
  exit 1
fi

# Replace wholesale rather than merge: CI regenerates from an empty directory,
# so the artifact is authoritative and this also drops baselines for stories
# that no longer exist.
rm -rf "$SNAPSHOT_DIR"
mkdir -p "$(dirname "$SNAPSHOT_DIR")"
mv "$tmp" "$SNAPSHOT_DIR"
trap - EXIT

git add -A "$SNAPSHOT_DIR"
echo
echo "Staged baseline changes:"
git status --short -- "$SNAPSHOT_DIR"
echo
echo "Review the diff, then commit. Every changed PNG is a deliberate UI change."
