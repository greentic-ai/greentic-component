#!/usr/bin/env bash
# Usage:
#   LOCAL_CHECK_ONLINE=1 LOCAL_CHECK_STRICT=1 LOCAL_CHECK_VERBOSE=1 ci/local_check.sh
# Defaults: online, non-strict, quiet.

set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

# Enable online checks by default unless explicitly disabled.
LOCAL_CHECK_ONLINE=${LOCAL_CHECK_ONLINE:-1}
LOCAL_CHECK_STRICT=${LOCAL_CHECK_STRICT:-0}
LOCAL_CHECK_VERBOSE=${LOCAL_CHECK_VERBOSE:-0}
SMOKE_NAME=${SMOKE_NAME:-local-check}

if [ "$LOCAL_CHECK_VERBOSE" = "1" ]; then
    set -x
fi

need() {
    if command -v "$1" >/dev/null 2>&1; then
        return 0
    fi
    echo "[miss] $1"
    return 1
}

step() {
    echo ""
    echo "▶ $*"
}

FAILED=0

run_cmd() {
    local desc=$1
    shift
    step "$desc"
    if ! "$@"; then
        echo "[fail] $desc"
        FAILED=1
    fi
}

run_or_skip() {
    local desc=$1
    shift
    if "$@"; then
        return 0
    fi
    if [ "$LOCAL_CHECK_STRICT" = "1" ]; then
        echo "[fail] $desc"
        FAILED=1
    else
        echo "[skip] $desc"
    fi
}

hard_need() {
    if ! need "$1"; then
        echo "Error: required tool '$1' is missing" >&2
        exit 1
    fi
}

hard_need rustc
hard_need cargo

step "Tool versions"
rustc --version
cargo --version
need jq && jq --version || true
need curl && curl --version || true

run_cmd "cargo fmt" cargo fmt --all -- --check
run_cmd "cargo clippy" cargo clippy --all-targets --all-features -- -D warnings
run_cmd "cargo build --workspace --locked" cargo build --workspace --locked
run_cmd "cargo test --workspace --all-features --locked" cargo test --workspace --all-features --locked -- --nocapture
run_cmd "cargo build" cargo build
run_cmd "cargo build --no-default-features" cargo build --no-default-features
run_cmd "cargo build --features serde" cargo build --features serde
run_cmd "cargo test greentic-component cli+prepare" cargo test -p greentic-component --features "prepare,cli"

schema_check() {
    if [ "$LOCAL_CHECK_ONLINE" != "1" ]; then
        echo "[skip] schema drift check (offline)"
        return 0
    fi
    if ! need curl; then
        echo "[skip] schema drift check (curl missing)"
        return 0
    fi
    if ! need jq; then
        echo "[skip] schema drift check (jq missing)"
        return 0
    fi
    step "Schema drift check"
    local remote=/tmp/local-check-schema.json
    if ! curl -sSfo "$remote" https://greentic-ai.github.io/greentic-component/schemas/v1/component.manifest.schema.json; then
        if [ "$LOCAL_CHECK_STRICT" = "1" ]; then
            echo "[fail] schema curl"
            FAILED=1
        else
            echo "[skip] schema curl (remote unavailable)"
        fi
        return 0
    fi
    local remote_id local_id
    remote_id=$(jq -r '."$id"' "$remote")
    local_id=$(jq -r '."$id"' crates/greentic-component/schemas/v1/component.manifest.schema.json)
    if [ "$remote_id" != "$local_id" ]; then
        echo "Schema ID mismatch remote=$remote_id local=$local_id"
        if [ "$LOCAL_CHECK_STRICT" = "1" ]; then
            FAILED=1
        fi
    else
        echo "Schema IDs match: $remote_id"
    fi
}
schema_check

run_cli_probe() {
    local bin=$1
    shift
    local args=("$@")
    if ! need cargo; then
        echo "[skip] $bin probe (cargo missing)"
        return 0
    fi
    step "Run $bin against fixtures"
    cargo run -p greentic-component --features "prepare,cli" --bin "$bin" -- "${args[@]}" >/dev/null
}

run_cli_probe "component-inspect" --json crates/greentic-component/tests/fixtures/manifests/valid.component.json
run_cli_probe "component-doctor" crates/greentic-component/tests/fixtures/manifests/valid.component.json

if [ -n "${SMOKE_DIR:-}" ]; then
    smoke_path="$SMOKE_DIR"
    cleanup_smoke=0
else
    smoke_parent=$(mktemp -d 2>/dev/null || mktemp -d -t greentic-smoke)
    smoke_path="$smoke_parent/$SMOKE_NAME"
    cleanup_smoke=1
fi
SMOKE_MANIFEST="$smoke_path/component.manifest.json"
rm -rf "$smoke_path"
run_cmd "Smoke: scaffold component" \
    cargo run -p greentic-component --features "cli" --bin greentic-component -- \
    new --name "$SMOKE_NAME" --org ai.greentic \
    --path "$smoke_path" --non-interactive --no-check --json
run_cmd "Smoke: component-doctor (generated)" \
    cargo run -p greentic-component --features "cli" --bin component-doctor -- "$smoke_path"
run_cmd "Smoke: component-inspect (generated)" \
    cargo run -p greentic-component --features "cli" --bin component-inspect -- \
    --json "$SMOKE_MANIFEST"
run_cmd "Smoke: cargo check (generated)" \
    bash -lc "cd \"$smoke_path\" && cargo check --target wasm32-wasip2"
if [ ${cleanup_smoke:-0} -eq 1 ]; then
    rm -rf "$smoke_parent"
fi

if [ $FAILED -ne 0 ]; then
    echo ""
    echo "local_check: FAILED"
    exit 1
fi

echo ""
echo "local_check: all checks passed"
