#!/usr/bin/env bash
# Usage:
#   LOCAL_CHECK_ONLINE=1 LOCAL_CHECK_STRICT=1 LOCAL_CHECK_VERBOSE=1 ci/local_check.sh
# Defaults: offline, non-strict, quiet.

set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

LOCAL_CHECK_ONLINE=${LOCAL_CHECK_ONLINE:-0}
LOCAL_CHECK_STRICT=${LOCAL_CHECK_STRICT:-0}
LOCAL_CHECK_VERBOSE=${LOCAL_CHECK_VERBOSE:-0}

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
    need curl || { echo "[skip] schema drift check (curl missing)"; return 0; }
    need jq || { echo "[skip] schema drift check (jq missing)"; return 0; }
    step "Schema $id drift check"
    local remote=/tmp/local-check-schema.json
    if ! curl -sSfo "$remote" https://greentic-ai.github.io/greentic-component/schemas/v1/component.manifest.schema.json; then
        echo "[fail] schema curl"
        FAILED=1
        return 1
    fi
    local remote_id local_id
    remote_id=$(jq -r '."$id"' "$remote")
    local_id=$(jq -r '."$id"' crates/greentic-component/schemas/v1/component.manifest.schema.json)
    if [ "$remote_id" != "$local_id" ]; then
        echo "Schema ID mismatch remote=$remote_id local=$local_id"
        FAILED=1
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

if [ $FAILED -ne 0 ]; then
    echo ""
    echo "local_check: FAILED"
    exit 1
fi

echo ""
echo "local_check: all checks passed"
