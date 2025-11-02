# Greentic Component Workspace

This workspace houses the core pieces needed to load, validate, and execute Greentic components without baking any component-specific knowledge into the runner. It is organised into three crates:

- `component-manifest` — strongly-typed parsing and validation for component self-descriptions. It validates capability lists, export declarations, config schemas, and WIT compatibility using JSON Schema tooling.
- `component-store` — fetches component artifacts from supported stores (filesystem, HTTP, OCI/Warg placeholders) with caching and digest/signature policy enforcement.
- `component-runtime` — uses Wasmtime’s component model to load components, bind tenant configuration/secrets, and invoke exported operations via the generic Greentic interfaces.

## Development

### Prerequisites

- Rust stable toolchain (1.78 or newer recommended)
- `wasmtime` dependencies (clang/LLVM on macOS & Linux) if you intend to run components locally

### Running Checks

```bash
# Format sources
cargo fmt

# Lint (clippy is run across all targets/features)
cargo clippy --all-targets --all-features

# Run tests for all crates
cargo test
```

## Releases & Publishing

- Versions are sourced directly from each crate's `Cargo.toml`.
- Pushing to `master` tags any crate whose version changed as `<crate-name>-v<semver>`.
- The publish workflow then attempts to release updated crates to crates.io.
- Publishing is idempotent: reruns succeed even when the crate version already exists.

## Component Store

The new `greentic-component` crate exposes a `ComponentStore` that can register filesystem paths and OCI references, materialise component bytes, and persist them in a content-addressed cache (`~/.greentic/components` by default).

```rust
use greentic_component::{CompatPolicy, ComponentStore};

let policy = CompatPolicy {
    required_abi_prefix: "greentic-abi-0".into(),
    required_capabilities: vec!["messaging".into()],
};

let mut store = ComponentStore::with_cache_dir(None, policy);
store.add_fs("local", "./build/my_component.wasm");
store.add_oci("remote", "ghcr.io/acme/greentic-tools:1.2.3");

let component = store.get("local").await?;
println!("id={} size={}", component.id.0, component.meta.size);
```

- Cache keys are `sha256:<digest>`; a locator index speeds up repeated fetches.
- OCI layers are selected when the media type advertises `application/wasm` or `application/octet-stream`.
- Capability and ABI compatibility checks are enforced before cache writes succeed.

## Testing Overview

Automated tests cover multiple layers:

- **Manifest validation** (`crates/component-manifest/tests/manifest_valid.rs`): ensures well-formed manifests pass and malformed manifests (duplicate capabilities, invalid secrets) fail.
- **Component store** (`crates/component-store/tests/*.rs`): verifies filesystem listings, caching behaviour, and HTTP fetching via a lightweight test server.
- **Runtime binding** (`crates/component-runtime/src/binder.rs` tests): validates schema enforcement and secret resolution logic.
- **Host imports** (`crates/component-runtime/src/host_imports.rs` tests): exercises telemetry gating plus the HTTP fetch host import, including policy denial and successful request/response handling.

Add new tests alongside the relevant crate to keep runtime guarantees tight.

## Host HTTP Fetch

The runtime now honours `HostPolicy::allow_http_fetch`. When enabled, host imports will perform outbound HTTP requests via `reqwest`, propagate headers, and base64-encode response bodies for safe transport back to components.

## Future Work

- Implement OCI/Warg store backends.
- Expand integration coverage with real Wasm components once fixtures are available.
- Support streaming invocations via the Greentic component interface.

Contributions welcome—please run `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test` before submitting changes.
