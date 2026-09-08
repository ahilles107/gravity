#!/usr/bin/env bash
#
# Enforces the project rule: no source file may exceed 400 lines.
#
# TypeScript and TSX are covered by oxlint's `max-lines` rule (see
# apps/desktop/.oxlintrc.json); this script covers the sources oxlint does not
# lint — Rust and CSS. Run from anywhere inside the repository.

set -euo pipefail

LIMIT="${MAX_FILE_LINES:-400}"
cd "$(git rev-parse --show-toplevel)"

status=0
while IFS= read -r file; do
  [ -f "$file" ] || continue
  lines=$(wc -l <"$file" | tr -d ' ')
  if [ "$lines" -gt "$LIMIT" ]; then
    printf '%s:%s: file has too many lines (%s). Maximum allowed is %s.\n' \
      "$file" "$lines" "$lines" "$LIMIT"
    status=1
  fi
done < <(git ls-files -- '*.rs' '*.css')

if [ "$status" -eq 0 ]; then
  echo "All Rust and CSS files are within $LIMIT lines."
fi
exit "$status"
