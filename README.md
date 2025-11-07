# Greentic Component Workspace

This workspace houses the core pieces needed to load, validate, and execute Greentic components without baking any component-specific knowledge into the runner. It is organised into three crates:

- `component-manifest` — strongly-typed parsing and validation for component self-descriptions. It validates capability lists, export declarations, config schemas, and WIT compatibility using JSON Schema tooling.
- `component-store` — fetches component artifacts from supported stores (filesystem, HTTP, OCI/Warg placeholders) with caching and digest/signature policy enforcement.
- `component-runtime` — uses Wasmtime’s component model to load components, bind tenant configuration/secrets, and invoke exported operations via the generic Greentic interfaces.

## Development

### Prerequisites & MSRV

- Rust stable toolchain (MSRV: 1.85)
- `wasmtime` dependencies (clang/LLVM on macOS & Linux) if you intend to run components locally

### Cargo Features

| Feature    | Default | Purpose |
|------------|---------|---------|
| `oci`      | ✅       | Enable OCI fetching for the component store. |
| `schema`   | ⛔️       | Generate JSON Schemas via `schemars`. |
| `abi`      | ⛔️       | Pull in the WIT/wasm tooling required for `abi::check_world` and lifecycle inspection. |
| `describe` | ⛔️       | Enable describe payload helpers (builds on `abi`). |
| `loader`   | ⛔️       | Component discovery APIs (`loader::discover`). |
| `prepare`  | ⛔️       | One-stop loader (`prepare_component`) plus caching. |
| `cli`      | ⛔️       | Build the `component-inspect` and `component-doctor` binaries (implies `prepare`). |

Enable only the features you need to avoid pulling in heavy wasm tooling when you are just parsing manifests.

### Integrating with greentic-dev / runner

```rust
use greentic_component::prepare_component;

let prepared = prepare_component("./component.manifest.json")?;
pack_builder.with_component(prepared.to_pack_entry()?);
runner.add_component(prepared.to_runner_config());
```

`PreparedComponent` exposes both `to_pack_entry()` (hashes, manifest JSON, first schema) and `to_runner_config()` (wasm path, world, capabilities/limits/telemetry, redactions/defaults, describe payload), which lets higher-level tooling plug in with almost no extra glue.

### Running Checks

```bash
# Format sources
cargo fmt

# Lint (clippy is run across all targets/features)
cargo clippy --all-targets --all-features

# Run tests for all crates
cargo test
```

### Local Checks

Run `ci/local_check.sh` to mirror CI locally:

```bash
# Default: offline, non-strict
ci/local_check.sh

# Enable online-only checks & strict mode
LOCAL_CHECK_ONLINE=1 LOCAL_CHECK_STRICT=1 ci/local_check.sh

# Show every command
LOCAL_CHECK_VERBOSE=1 ci/local_check.sh
```

The script gracefully skips network-dependent steps unless `LOCAL_CHECK_ONLINE=1` and will fail fast when `LOCAL_CHECK_STRICT=1` is set.

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

## Component Manifest v1

`crates/greentic-component` now owns the canonical manifest schema (`schemas/v1/component.manifest.schema.json`) and typed parser. Manifests describe a reverse-DNS `id`, human name, semantic `version`, the exported WIT `world`, and the function to call for describing configuration. Artifact metadata captures the relative wasm path plus a required `blake3` digest. Optional sections describe enforced `limits`, `telemetry` attributes, and build `provenance` (builder, commit, toolchain, timestamp).

- **Capabilities** — structured declarations for HTTP domains, secrets scopes, KV buckets, filesystem mounts, net access, and tool invocations. The `security::enforce_capabilities` helper compares a manifest against a runtime `Profile` and produces precise denials (e.g. `capabilities.http.domains[foo.example]`).
- **Describe loading order** — `describe::load` first tries to decode the embedded WIT world from the wasm, falls back to a JSON blob emitted by an exported symbol (e.g. `describe`), and finally searches `schemas/v1/*.json` for provider-supplied payloads. The resulting `DescribePayload` snapshots all known schema versions.
- **Redaction hints** — schema utilities walk arbitrary JSON Schema documents and surface paths tagged with `x-redact`, `x-default-applied`, and `x-capability`. These hints are used by greentic-dev/runner to scrub transcripts or explain defaulted fields.

See `greentic_component::manifest` and `greentic_component::describe` for the Rust APIs, and consult the workspace tests for concrete usage.

The schema is published at <https://greentic-ai.github.io/greentic-component/schemas/v1/component.manifest.schema.json>. A minimal manifest looks like:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://greentic-ai.github.io/greentic-component/schemas/v1/component.manifest.schema.json",
  "id": "com.greentic.examples.echo",
  "name": "Echo",
  "version": "0.1.0",
  "world": "greentic:component/node@0.1.0",
  "describe_export": "describe",
  "capabilities": {},
  "artifacts": {"component_wasm": "component.wasm"},
  "hashes": {"component_wasm": "blake3:..."}
}
```

### Command-line tools (optional `cli` feature)

```
cargo run --features cli --bin component-inspect ./component.manifest.json --json
cargo run --features cli --bin component-doctor ./component.manifest.json
```

`component-inspect` emits a structured JSON report with manifest metadata, BLAKE3 hashes, lifecycle detection, describe payloads, and redaction hints sourced from `x-redact` annotations. `component-doctor` executes the full validation pipeline (schema validation, hash verification, world/ABI probe, lifecycle detection, describe resolution, and redaction summary) and exits non-zero on any failure—perfect for CI gates.

## Host HTTP Fetch

The runtime now honours `HostPolicy::allow_http_fetch`. When enabled, host imports will perform outbound HTTP requests via `reqwest`, propagate headers, and base64-encode response bodies for safe transport back to components.

## Future Work

- Implement OCI/Warg store backends.
- Expand integration coverage with real Wasm components once fixtures are available.
- Support streaming invocations via the Greentic component interface.

Contributions welcome—please run `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test` before submitting changes.

## Security

See [SECURITY.md](SECURITY.md) for guidance on `x-redact`, capability declarations, and protecting operator logs.
