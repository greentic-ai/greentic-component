#!/usr/bin/env bash
set -euo pipefail

ROOT="${1:-.}"
PATTERN='^package[[:space:]]+greentic:component@'

MATCHES="$(rg -n --glob '*.wit' \
  --glob '!**/target/**' \
  --glob '!**/tests/fixtures/**' \
  "$PATTERN" "$ROOT" || true)"

if [[ -n "$MATCHES" ]]; then
  echo "ERROR: Canonical greentic:component WIT is duplicated in this repository:"
  echo
  echo "$MATCHES"
  echo
  echo "Use canonical WIT from greentic-interfaces instead of committing package greentic:component@*.wit here."
  exit 1
fi

echo "OK: no duplicated canonical greentic:component WIT packages found."
