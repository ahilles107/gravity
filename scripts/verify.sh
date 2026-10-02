#!/usr/bin/env bash
# Run the full repository verification from any working directory.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

exec node scripts/verify.mjs
