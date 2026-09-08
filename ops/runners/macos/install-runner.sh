#!/usr/bin/env bash
# Installs an optional macOS ARM64 runner as a launchd user agent.
# Supply REPO_URL, RUNNER_NAME and a short-lived registration TOKEN from
# your repository's Actions runner settings. See ../README.md.
set -euo pipefail

REPO_URL="${REPO_URL:?set REPO_URL}"
RUNNER_NAME="${RUNNER_NAME:?set RUNNER_NAME}"
TOKEN="${TOKEN:?set TOKEN}"
# Override DIR if this host already has a Gravity runner.
DIR="${DIR:-$HOME/actions-runner-gravity}"
LABELS="${LABELS:-self-hosted,macOS,ARM64}"

version="$(curl -fsSL https://api.github.com/repos/actions/runner/releases/latest | jq -r .tag_name)"
version="${version#v}"
tarball="actions-runner-osx-arm64-${version}.tar.gz"

mkdir -p "$DIR"
cd "$DIR"
if [ ! -x ./config.sh ]; then
  curl -fsSLO "https://github.com/actions/runner/releases/download/v${version}/${tarball}"
  tar xzf "$tarball"
  rm -f "$tarball"
fi

./config.sh --unattended --replace \
  --url "$REPO_URL" \
  --token "$TOKEN" \
  --name "$RUNNER_NAME" \
  --labels "$LABELS" \
  --work _work

./svc.sh install
./svc.sh start
./svc.sh status
