#!/usr/bin/env bash
# Run the full repository verification from any working directory.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

pnpm notices:check
python3 -m unittest discover -s scripts -p 'test_notices.py'
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
bash scripts/check-file-length.sh

pnpm --dir apps/desktop check
pnpm --dir apps/desktop build
pnpm --dir apps/marketing check

bash scripts/prepare-sidecar.sh
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml

bash scripts/vr-ci.sh
