#!/usr/bin/env bash
# Deprecated shim kept for compatibility; prefer running ci/local_check.sh directly.
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname "$0")" && pwd)
echo "[deprecated] ci/local_check.sh -> ci/local_check.sh" >&2
exec "$SCRIPT_DIR/local_check.sh" "$@"
