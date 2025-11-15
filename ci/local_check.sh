#!/usr/bin/env bash
# Usage:
#   LOCAL_CHECK_ONLINE=1 LOCAL_CHECK_STRICT=1 LOCAL_CHECK_VERBOSE=1 ci/local_check.sh
# Defaults: online, non-strict, quiet.

set -euo pipefail
export RUSTFLAGS=""

ROOT_DIR=$(cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT_DIR/target}

LOCAL_CHECK_ONLINE=${LOCAL_CHECK_ONLINE:-1}
LOCAL_CHECK_STRICT=${LOCAL_CHECK_STRICT:-0}
LOCAL_CHECK_VERBOSE=${LOCAL_CHECK_VERBOSE:-0}
SMOKE_NAME=${SMOKE_NAME:-local-check}
TREE_DIR=${LOCAL_CHECK_TREE_DIR:-$TARGET_DIR/local-check}
SMOKE_TARGET_DIR=$TARGET_DIR/smoke

# Publish mode defaults:
#   - CI environment => publish
#   - local runs      => dry-run
if [ -z "${LOCAL_CHECK_PUBLISH_MODE:-}" ]; then
    if [ -n "${CI:-}" ]; then
        LOCAL_CHECK_PUBLISH_MODE="publish"
    else
        LOCAL_CHECK_PUBLISH_MODE="dry-run"
    fi
fi

# Ignore publish dry-run failures when bumping versions locally.
LOCAL_VERSION_UPGRADE=${LOCAL_VERSION_UPGRADE:-0}
mkdir -p "$TREE_DIR" "$SMOKE_TARGET_DIR"

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
FAILED_STEPS=()
LAST_RUN_FAILED=0

record_failure() {
    FAILED=1
    FAILED_STEPS+=("$1")
}

run_cmd() {
    local desc=$1
    shift
    step "$desc"
    LAST_RUN_FAILED=0
    if ! "$@"; then
        echo "[fail] $desc"
        record_failure "$desc"
        LAST_RUN_FAILED=1
    fi
}

run_bin_cmd() {
    local desc=$1
    local bin_path=$2
    shift 2
    step "$desc"
    if [ ! -x "$bin_path" ]; then
        echo "[fail] $desc ($bin_path missing)"
        record_failure "$desc ($bin_path missing)"
        return 1
    fi
    if ! "$bin_path" "$@"; then
        echo "[fail] $desc"
        record_failure "$desc"
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
        record_failure "$desc"
    else
        echo "[skip] $desc"
    fi
}

skip_step() {
    local desc=$1
    local reason=$2
    if [ "$LOCAL_CHECK_STRICT" = "1" ]; then
        echo "[fail] $desc ($reason)"
        record_failure "$desc ($reason)"
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

CRATES_IO_AVAILABLE=0
CRATES_IO_REASON="LOCAL_CHECK_ONLINE=0"
if [ "$LOCAL_CHECK_ONLINE" = "1" ]; then
    CRATES_IO_REASON="crates.io unreachable"
    if command -v curl >/dev/null 2>&1; then
        if curl -sSf --max-time 5 https://index.crates.io/config.json >/dev/null 2>&1; then
            CRATES_IO_AVAILABLE=1
            CRATES_IO_REASON=""
        else
            echo "[warn] crates.io unreachable; online-only steps will be skipped"
        fi
    else
        CRATES_IO_REASON="curl missing"
        echo "[warn] curl is missing; crates.io-dependent steps will be skipped"
    fi
fi

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
    local attempts=0
    local success=0
    while [ $attempts -lt 3 ]; do
        if curl -sSf --max-time 5 -o "$remote" https://greentic-ai.github.io/greentic-component/schemas/v1/component.manifest.schema.json; then
            success=1
            break
        fi
        attempts=$((attempts + 1))
        echo "[warn] schema download attempt $attempts failed"
        sleep 1
    done
    if [ $success -ne 1 ]; then
        if [ "$LOCAL_CHECK_STRICT" = "1" ]; then
            echo "[fail] schema drift check (remote unavailable)"
            record_failure "schema drift check (remote unavailable)"
        else
            echo "[skip] schema drift check (remote unavailable)"
        fi
        return 0
    fi
    local remote_id
    local local_id
    remote_id=$(jq -r '."$id"' "$remote")
    local_id=$(jq -r '."$id"' crates/greentic-component/schemas/v1/component.manifest.schema.json)
    if [ "$remote_id" != "$local_id" ]; then
        echo "Schema ID mismatch remote=$remote_id local=$local_id"
        if [ "$LOCAL_CHECK_STRICT" = "1" ]; then
            record_failure "schema drift check (schema ID mismatch remote=$remote_id local=$local_id)"
        fi
    else
        echo "Schema IDs match: $remote_id"
    fi
}

build_release_bin() {
    local bin=$1
    local features=$2
    local args=(cargo build --locked --release -p greentic-component --bin "$bin")
    if [ -n "$features" ]; then
        args+=(--features "$features")
    fi
    run_cmd "cargo build release -p $bin" "${args[@]}"
}

if [ "$LOCAL_CHECK_ONLINE" = "1" ] && [ "$CRATES_IO_AVAILABLE" = "1" ]; then
    run_cmd "cargo fetch (linux target)" \
        cargo fetch --locked --target x86_64-unknown-linux-gnu
else
    skip_step "cargo fetch (linux target)" "${CRATES_IO_REASON:-crates.io unreachable}"
fi

run_cmd "cargo fmt" cargo fmt --all -- --check
run_cmd "cargo clippy" cargo clippy --locked --workspace --all-targets -- -D warnings
run_cmd "cargo build --workspace --locked" cargo build --workspace --locked
run_cmd "cargo test --workspace --locked" cargo test --workspace --locked -- --nocapture
run_cmd "cargo build --workspace --all-features --locked" cargo build --workspace --all-features --locked
run_cmd "cargo test --workspace --all-features --locked" cargo test --workspace --all-features --locked -- --nocapture
schema_check

build_release_bin component-inspect "cli,prepare"
build_release_bin component-doctor "cli,prepare"
build_release_bin component-hash "cli"
build_release_bin greentic-component "cli"

readonly BIN_COMPONENT_INSPECT=$TARGET_DIR/release/component-inspect
readonly BIN_COMPONENT_DOCTOR=$TARGET_DIR/release/component-doctor
readonly BIN_COMPONENT_HASH=$TARGET_DIR/release/component-hash
readonly BIN_GREENTIC_COMPONENT=$TARGET_DIR/release/greentic-component

run_bin_cmd "component-inspect probe" "$BIN_COMPONENT_INSPECT" --json crates/greentic-component/tests/fixtures/manifests/valid.component.json
run_bin_cmd "component-doctor probe" "$BIN_COMPONENT_DOCTOR" crates/greentic-component/tests/fixtures/manifests/valid.component.json

run_smoke_mode() {
    local mode=$1
    step "Smoke mode: $mode"
    local smoke_parent
    local cleanup_smoke=0
    local cleanup_dir
    if [ -n "${SMOKE_DIR:-}" ]; then
        smoke_parent="$SMOKE_DIR"
    else
        smoke_parent=$(mktemp -d)
        cleanup_smoke=1
        cleanup_dir="$smoke_parent"
        trap "rm -rf '$cleanup_dir'" EXIT
    fi
    local smoke_path="$smoke_parent/$SMOKE_NAME-$mode"
    rm -rf "$smoke_path"
    export GREENTIC_DEP_MODE="$mode"
    local smoke_manifest="$smoke_path/component.manifest.json"
    local had_cargo_target=0
    local prev_cargo_target
    if [ "${CARGO_TARGET_DIR+x}" = "x" ]; then
        had_cargo_target=1
        prev_cargo_target="$CARGO_TARGET_DIR"
    fi
    export CARGO_TARGET_DIR="$SMOKE_TARGET_DIR"
    run_bin_cmd "Smoke ($mode): scaffold component" "$BIN_GREENTIC_COMPONENT" \
        new --name "$SMOKE_NAME" --org ai.greentic \
        --path "$smoke_path" --non-interactive --no-check --json
    run_bin_cmd "Smoke ($mode): component-doctor" "$BIN_COMPONENT_DOCTOR" "$smoke_path"
    local network_ok=0
    local network_reason="${CRATES_IO_REASON:-network unavailable}"
    if [ "$LOCAL_CHECK_ONLINE" = "1" ] && [ "$CRATES_IO_AVAILABLE" = "1" ]; then
        if curl -sSf --max-time 5 https://index.crates.io/config.json >/dev/null 2>&1; then
            local wasm_ready=1
            run_cmd "Smoke ($mode): cargo generate-lockfile" \
                bash -lc "cd '$smoke_path' && cargo generate-lockfile"
            if [ "$LAST_RUN_FAILED" -eq 1 ]; then
                wasm_ready=0
                network_reason="cargo generate-lockfile failed"
            fi
            local tree_file="$TREE_DIR/tree-$mode.txt"
            if [ $wasm_ready -eq 1 ]; then
                run_cmd "Smoke ($mode): cargo tree" \
                    bash -lc "cd '$smoke_path' && cargo tree -e no-dev --locked | tee '$tree_file' >/dev/null"
                if [ "$LAST_RUN_FAILED" -eq 1 ]; then
                    wasm_ready=0
                    network_reason="cargo tree failed"
                fi
            fi
            if [ $wasm_ready -eq 1 ]; then
                run_cmd "Smoke ($mode): cargo check" \
                    bash -lc "cd '$smoke_path' && cargo check --target wasm32-wasip2 --locked"
                if [ "$LAST_RUN_FAILED" -eq 1 ]; then
                    wasm_ready=0
                    network_reason="cargo check failed"
                fi
            fi
            if [ $wasm_ready -eq 1 ]; then
                run_cmd "Smoke ($mode): cargo build --release" \
                    bash -lc "cd '$smoke_path' && cargo build --target wasm32-wasip2 --release --locked"
                if [ "$LAST_RUN_FAILED" -eq 1 ]; then
                    wasm_ready=0
                    network_reason="cargo wasm build failed"
                fi
            fi
            if [ $wasm_ready -eq 1 ]; then
                local wasm_file="$smoke_path/bin/component.wasm"
                if [ ! -f "$wasm_file" ]; then
                    wasm_ready=0
                    network_reason="wasm artifact missing ($wasm_file)"
                fi
            fi
            if [ $wasm_ready -eq 1 ]; then
                network_ok=1
            fi
        else
            network_reason="crates.io unreachable"
        fi
    else
        if [ "$LOCAL_CHECK_ONLINE" != "1" ]; then
            network_reason="LOCAL_CHECK_ONLINE=0"
        fi
    fi
    if [ $network_ok -ne 1 ]; then
        skip_step "Smoke ($mode): cargo generate-lockfile" "$network_reason"
        skip_step "Smoke ($mode): cargo tree" "$network_reason"
        skip_step "Smoke ($mode): cargo check" "$network_reason"
        skip_step "Smoke ($mode): cargo build --release" "$network_reason"
    fi
    if [ $network_ok -eq 1 ]; then
        run_bin_cmd "Smoke ($mode): component-hash" "$BIN_COMPONENT_HASH" "$smoke_manifest"
        run_bin_cmd "Smoke ($mode): component-inspect" "$BIN_COMPONENT_INSPECT" --json "$smoke_manifest"
    else
        skip_step "Smoke ($mode): update manifest hash" "${network_reason:-wasm build unavailable}"
        skip_step "Smoke ($mode): component-inspect" "${network_reason:-wasm build unavailable}"
    fi
    if [ $cleanup_smoke -eq 1 ]; then
        rm -rf "$smoke_parent"
        trap - EXIT
    fi
    if [ $had_cargo_target -eq 1 ]; then
        export CARGO_TARGET_DIR="$prev_cargo_target"
    else
        unset CARGO_TARGET_DIR
    fi
}

if [ "${LOCAL_CHECK_SKIP_SMOKE:-0}" = "1" ]; then
    echo "[skip] smoke scaffold (LOCAL_CHECK_SKIP_SMOKE=1)"
else
    for mode in local cratesio; do
        run_smoke_mode "$mode"
    done
fi

publish_crates=(
    greentic-component
)
for crate in "${publish_crates[@]}"; do
    run_cmd "cargo package (locked) -p $crate" \
        cargo package --allow-dirty -p "$crate" --locked
done
case "$LOCAL_CHECK_PUBLISH_MODE" in
    dry-run)
        if [ "$LOCAL_CHECK_ONLINE" = "1" ] && [ "$CRATES_IO_AVAILABLE" = "1" ]; then
            for crate in "${publish_crates[@]}"; do
                step "cargo publish --dry-run (locked) -p $crate"
                echo ""
                echo "▶ cargo publish --dry-run (locked) -p $crate"
                if ! cargo publish --allow-dirty -p "$crate" --dry-run --locked; then
                    if [ "$LOCAL_VERSION_UPGRADE" = "1" ]; then
                        echo "[warn] cargo publish --dry-run (locked) -p $crate failed, ignoring due to LOCAL_VERSION_UPGRADE=1"
                    else
                        echo "[fail] cargo publish --dry-run (locked) -p $crate"
                        record_failure "cargo publish --dry-run (locked) -p $crate"
                    fi
                fi
            done
        else
            skip_step "cargo publish --dry-run (locked)" "${CRATES_IO_REASON:-network unavailable}"
        fi
        ;;
    publish)
        if [ "$LOCAL_CHECK_ONLINE" = "1" ] && [ "$CRATES_IO_AVAILABLE" = "1" ]; then
            for crate in "${publish_crates[@]}"; do
                step "cargo publish (locked) -p $crate"
                echo ""
                echo "▶ cargo publish (locked) -p $crate"
                if ! cargo publish --allow-dirty -p "$crate" --locked; then
                    echo "[fail] cargo publish (locked) -p $crate"
                    record_failure "cargo publish (locked) -p $crate"
                fi
            done
        else
            skip_step "cargo publish (locked)" "${CRATES_IO_REASON:-network unavailable}"
        fi
        ;;
    *)
        skip_step "cargo publish / dry-run" "LOCAL_CHECK_PUBLISH_MODE=$LOCAL_CHECK_PUBLISH_MODE not recognized"
        ;;
esac

if [ "$FAILED" -ne 0 ]; then
    echo ""
    if [ "${#FAILED_STEPS[@]}" -ne 0 ]; then
        echo "Failed checks:"
        for step in "${FAILED_STEPS[@]}"; do
            echo "- $step"
        done
        echo ""
    fi
    echo "❌ LOCAL CHECK FAILED"
    exit 1
fi

echo ""
echo "✅ LOCAL CHECK PASSED"
