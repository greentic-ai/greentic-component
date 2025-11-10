#!/usr/bin/env bash
# Usage:
#   LOCAL_CHECK_ONLINE=1 LOCAL_CHECK_STRICT=1 LOCAL_CHECK_VERBOSE=1 ci/local_check.sh
# Defaults: online, non-strict, quiet.

set -euo pipefail
export RUSTFLAGS=""

ROOT_DIR=$(cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

# Enable online checks by default unless explicitly disabled.
LOCAL_CHECK_ONLINE=${LOCAL_CHECK_ONLINE:-1}
LOCAL_CHECK_STRICT=${LOCAL_CHECK_STRICT:-0}
LOCAL_CHECK_VERBOSE=${LOCAL_CHECK_VERBOSE:-0}
SMOKE_NAME=${SMOKE_NAME:-local-check}
TREE_DIR=${LOCAL_CHECK_TREE_DIR:-$ROOT_DIR/target/local-check}
mkdir -p "$TREE_DIR"

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

skip_step() {
    local desc=$1
    local reason=$2
    if [ "$LOCAL_CHECK_STRICT" = "1" ]; then
        echo "[fail] $desc ($reason)"
        FAILED=1
    else
        echo "[skip] $desc ($reason)"
    fi
}

hard_need() {
    if ! need "$1"; then
        echo "Error: required tool '$1' is missing" >&2
        exit 1
    fi
}

ensure_rust_target() {
    local target=$1
    if rustup target list --installed | grep -Fxq "$target"; then
        return 0
    fi
    step "Installing Rust target $target"
    rustup target add "$target"
}

hard_need rustc
hard_need cargo
hard_need rustup

ensure_rust_target wasm32-wasip2
ensure_rust_target x86_64-unknown-linux-gnu
ensure_rust_component() {
    local component=$1
    if rustup component list --installed | cut -d' ' -f1 | grep -Fxq "$component"; then
        return 0
    fi
    step "Installing Rust component $component"
    rustup component add "$component"
}

ensure_rust_component rustfmt
ensure_rust_component clippy

step "Tool versions"
rustc --version
cargo --version
need jq && jq --version || true
need curl && curl --version || true

if [ "$LOCAL_CHECK_ONLINE" = "1" ]; then
    run_cmd "cargo fetch (linux target)" \
        cargo fetch --locked --target x86_64-unknown-linux-gnu
else
    skip_step "cargo fetch (linux target)" "offline mode"
fi

run_cmd "cargo fmt" cargo fmt --all -- --check
run_cmd "cargo clippy" cargo clippy --locked --workspace --all-targets -- -D warnings
run_cmd "cargo build --workspace --locked" cargo build --workspace --locked
if [ "$LOCAL_CHECK_STRICT" = "1" ]; then
    run_cmd "cargo build --workspace --all-features --locked" cargo build --workspace --all-features --locked
else
    run_cmd "cargo build (greentic-component all features)" \
        cargo build -p greentic-component --all-features --locked
fi
if [ "$LOCAL_CHECK_STRICT" = "1" ]; then
    run_cmd "cargo test --workspace --all-features --locked" \
        cargo test --workspace --all-features --locked -- --nocapture
else
    run_cmd "cargo test --workspace --locked" \
        cargo test --workspace --locked -- --nocapture
fi
run_cmd "cargo build" cargo build --locked
run_cmd "cargo build --no-default-features" cargo build --no-default-features --locked
run_cmd "cargo build --features serde" cargo build --features serde --locked
run_cmd "cargo test greentic-component cli+prepare" \
    cargo test -p greentic-component --features "prepare,cli" --locked

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
    if ! curl -sSf --max-time 5 -o "$remote" https://greentic-ai.github.io/greentic-component/schemas/v1/component.manifest.schema.json; then
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
    cargo run --locked -p greentic-component --features "prepare,cli" --bin "$bin" -- "${args[@]}" >/dev/null
}

run_cli_probe "component-inspect" --json crates/greentic-component/tests/fixtures/manifests/valid.component.json
run_cli_probe "component-doctor" crates/greentic-component/tests/fixtures/manifests/valid.component.json

run_smoke_mode() {
    local mode=$1
    step "Smoke mode: $mode"
    if [ -n "${SMOKE_DIR:-}" ]; then
        smoke_path="$SMOKE_DIR/$mode"
        cleanup_smoke=0
    else
        smoke_parent=$(mktemp -d 2>/dev/null || mktemp -d -t "greentic-smoke-$mode")
        smoke_path="$smoke_parent/$SMOKE_NAME-$mode"
        cleanup_smoke=1
    fi
    GREENTIC_DEP_MODE="$mode"
    export GREENTIC_DEP_MODE
    SMOKE_MANIFEST="$smoke_path/component.manifest.json"
    rm -rf "$smoke_path"
    run_cmd "Smoke ($mode): scaffold component" \
        cargo run --locked -p greentic-component --features "cli" --bin greentic-component -- \
        new --name "$SMOKE_NAME" --org ai.greentic \
        --path "$smoke_path" --non-interactive --no-check --json
    run_cmd "Smoke ($mode): component-doctor" \
        cargo run --locked -p greentic-component --features "cli" --bin component-doctor -- "$smoke_path"
    local network_ok=0
    if [ "$LOCAL_CHECK_ONLINE" = "1" ] && \
        curl -sSf --max-time 5 https://index.crates.io/config.json >/dev/null 2>&1; then
        network_ok=1
        run_cmd "Smoke ($mode): cargo generate-lockfile" \
            bash -lc "cd \"$smoke_path\" && cargo generate-lockfile"
        local tree_file="$TREE_DIR/tree-$mode.txt"
        run_cmd "Smoke ($mode): cargo tree" \
            bash -lc "cd \"$smoke_path\" && cargo tree -e no-dev --locked | tee \"$tree_file\" >/dev/null"
        run_cmd "Smoke ($mode): cargo check" \
            bash -lc "cd \"$smoke_path\" && cargo check --target wasm32-wasip2 --locked"
        run_cmd "Smoke ($mode): cargo build --release" \
            bash -lc "cd \"$smoke_path\" && cargo build --target wasm32-wasip2 --release --locked"
    else
        local reason="network unavailable"
        if [ "$LOCAL_CHECK_ONLINE" != "1" ]; then
            reason="LOCAL_CHECK_ONLINE=0"
        fi
        skip_step "Smoke ($mode): cargo generate-lockfile" "$reason"
        skip_step "Smoke ($mode): cargo tree" "$reason"
        skip_step "Smoke ($mode): cargo check" "$reason"
        skip_step "Smoke ($mode): cargo build --release" "$reason"
    fi
    if [ "$network_ok" -eq 1 ]; then
        run_cmd "Smoke ($mode): update manifest hash" \
            cargo run --locked -p greentic-component --features "cli" --bin component-hash -- \
            "$SMOKE_MANIFEST"
        run_cmd "Smoke ($mode): component-inspect" \
            cargo run --locked -p greentic-component --features "cli" --bin component-inspect -- \
            --json "$SMOKE_MANIFEST"
    else
        skip_step "Smoke ($mode): update manifest hash" "wasm build unavailable"
        skip_step "Smoke ($mode): component-inspect" "wasm build unavailable"
    fi
    if [ ${cleanup_smoke:-0} -eq 1 ]; then
        rm -rf "$smoke_parent"
    fi
}

if [ "${LOCAL_CHECK_SKIP_SMOKE:-0}" = "1" ]; then
    echo "[skip] smoke scaffold (LOCAL_CHECK_SKIP_SMOKE=1)"
else
    for mode in local cratesio; do
        run_smoke_mode "$mode"
    done
fi

if [ $FAILED -ne 0 ]; then
    echo ""
    echo "local_check: FAILED"
    exit 1
fi

echo ""
echo "local_check: all checks passed"
