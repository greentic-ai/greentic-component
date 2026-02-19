#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=${1:-.}

if rg -n --hidden \
    --glob '!target/**' \
    --glob '!.git/**' \
    --glob '!.codex/**' \
    --glob '!ci/check_no_bindings_imports.sh' \
    "greentic_interfaces::bindings::|\\bbindings::greentic::" \
    "$ROOT_DIR"; then
    echo "ERROR: use greentic_interfaces::canonical instead of bindings::*"
    exit 1
fi

echo "OK: no downstream bindings::* imports found."
