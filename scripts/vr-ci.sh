#!/usr/bin/env bash
#
# Runs the Ladle visual-regression suite inside the pinned Playwright image.
#
# Baselines are compared byte for byte, so they are only reproducible in one
# exact environment. This script is that environment: the browser, the font
# set and the renderer all come from the image tag below, never from the
# runner host. Bump PLAYWRIGHT_IMAGE and @playwright/test together, and expect
# to re-accept every baseline when you do.
#
# Usage: scripts/vr-ci.sh [--update]
#   --update  regenerate every baseline from scratch instead of comparing.

set -euo pipefail

PLAYWRIGHT_IMAGE="mcr.microsoft.com/playwright:v1.62.1-noble"

cd "$(git rev-parse --show-toplevel)"
workspace="${GITHUB_WORKSPACE:-$PWD}"

npm_script="vr"
if [ "${1:-}" = "--update" ]; then
  npm_script="vr:update"
fi

# The self-hosted runner is itself a container, so a sibling container cannot
# bind-mount the workspace by path — that path exists only inside the runner.
# /proc/self/mountinfo maps it back to the host: field 4 is the source on the
# host, field 5 is where it is mounted here. Re-mounting the source at the same
# destination makes every path identical on both sides. On a bare-metal runner
# nothing matches except "/", and we bind the workspace directly.
# macOS has no mountinfo, and there the workspace path is already the host
# path, so the direct bind below is correct.
best_dest="/"
best_src=""
if [ -r /proc/self/mountinfo ]; then
  while read -r _ _ _ src dest _; do
    case "$workspace/" in
      "$dest"/*)
        if [ "${#dest}" -gt "${#best_dest}" ] || [ "$best_dest" = "/" ]; then
          best_dest="$dest"
          best_src="$src"
        fi
        ;;
    esac
  done </proc/self/mountinfo
fi

if [ "$best_dest" = "/" ] || [ -z "$best_src" ]; then
  mount_args=(-v "$workspace:$workspace")
else
  mount_args=(-v "$best_src:$best_dest")
fi

echo "Mounting ${mount_args[1]} for $PLAYWRIGHT_IMAGE"

# Dependencies are installed inside the container rather than on the runner:
# rollup and friends ship per-platform native binaries, so a node_modules
# built anywhere else fails to load here.
#
# Both the install target and the store are named volumes. node_modules in
# particular must not be the host's — otherwise running this on a developer
# machine overwrites their darwin binaries with linux ones and every local
# tool breaks until they reinstall. Keeping it in a volume also means only the
# first run pays for the download.
#
# --ipc=host keeps Chromium from exhausting the default 64 MB /dev/shm and
# dying mid-suite. CI=true reaches the config's guard against local writes.
exec docker run --rm \
  --ipc=host \
  -e CI=true \
  "${mount_args[@]}" \
  -v gravity-vr-pnpm-store:/pnpm-store \
  -v gravity-vr-node-modules:"$workspace/apps/desktop/node_modules" \
  -w "$workspace/apps/desktop" \
  "$PLAYWRIGHT_IMAGE" \
  sh -ec '
    npm install --global --silent pnpm@11 >/dev/null
    pnpm install --frozen-lockfile --store-dir /pnpm-store
    pnpm run "$1"
  ' _ "$npm_script"
