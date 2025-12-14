# Greentic Component Workspace

This workspace houses the core pieces needed to load, validate, and execute Greentic components without baking any component-specific knowledge into the runner. It is organised into three crates:

- `greentic-component-manifest` — strongly-typed parsing and validation for component self-descriptions. It validates capability lists, export declarations, config schemas, and WIT compatibility using JSON Schema tooling.
- `greentic-component-store` — fetches component artifacts from supported stores (filesystem, HTTP, OCI/Warg placeholders) with caching and digest/signature policy enforcement.
- `greentic-component-runtime` — uses Wasmtime’s component model to load components, bind tenant configuration/secrets, and invoke exported operations via the generic Greentic interfaces.

## Crates & publishing

Only the `greentic-component` crate is published on crates.io. Internal crates such as
`greentic-component-store` and `greentic-component-runtime` exist for code organization inside this
workspace and are marked `publish = false`. If you want to consume Greentic tooling from crates.io,
depend on `greentic-component` only:

```toml
[dependencies]
greentic-component = "0.4"
```

Developers working in this repository interact directly with the internal crates via their workspace
paths; downstream users do not need to reference them.

## Installation

```bash
rustup target add wasm32-wasip2
cargo install cargo-binstall
cargo binstall greentic-component           # prebuilt binaries from GitHub Releases
cargo install --path crates/greentic-component --features cli   # or build from source locally
# work locally via: make build
```

Tagged releases ship binstall-ready archives (Linux, macOS, Windows), so `cargo binstall` will fetch
prebuilt binaries when available and fall back to building from source otherwise.

The CLI lives inside this workspace; running `cargo run -p greentic-component --features cli --bin greentic-component -- <command>`
is convenient during development. For a workspace build that tracks your working copy, use
`cargo install --path crates/greentic-component --features cli`.

## Quickstart

```bash
# 1. Discover templates (built-in + ~/.greentic/templates/component/*)
greentic-component templates

# 2. Scaffold a component (runs cargo check --target wasm32-wasip2)
greentic-component new --name hello-world --org ai.greentic

# 3. Inspect / doctor the generated project
component-doctor ./hello-world
```

Need the full CLI reference? `greentic-component new --help` and `greentic-component templates --help`
describe every flag (JSON output, custom templates, reverse-DNS org names, etc.).

## Templates

- Built-in template: `rust-wasi-p2-min` (a Rust 2024 `cdylib` that targets WASI-P2 via `wit-bindgen`).
- User templates: `~/.greentic/templates/component/<template-id>/` with an optional `template.json`
  describing `{ "id", "description", "tags" }` (override via `GREENTIC_TEMPLATE_ROOT=...`).
- Metadata is surfaced by `greentic-component templates --json`, making it script-friendly.

## Structure of a scaffolded component

```
hello-world/
├── Cargo.toml
├── src/lib.rs
├── component.manifest.json
├── schemas/
│   ├── component.schema.json
│   └── io/{input,output}.schema.json
├── wit/world.wit
├── tests/conformance.rs
├── .github/workflows/ci.yml
└── README.md / LICENSE / Makefile
```

The generator wires `component.manifest.json`, schema stubs, a WIT world, CI workflow, and a local Makefile
so the project is immediately buildable (`cargo check --target wasm32-wasip2`) and testable.

## Config flows (convention)

- Config flows are normal flows (`id`, `kind`, `description`, `nodes`) whose last node emits a payload of the form `{ "node_id": "...", "node": { ... } }`. The engine treats `kind: component-config` as a hint only.
- `greentic-component flow update` reads `component.manifest.json` (`id`, `mode`/`kind`, `config_schema`) and writes FlowIR JSON into `dev_flows.default` (required fields with defaults only) plus `dev_flows.custom` (questions for every non-hidden field, emitting state-backed config). No `.ygtc` sidecars are produced by default.
- Defaults are only applied when the manifest supplies them; required fields without defaults are omitted from `dev_flows.default`. Fields marked `x_flow_hidden: true` are skipped in `dev_flows.custom` prompts.
- Mode detection is tolerant (`mode`, then `kind`, else `"tool"`); the scaffold uses a generic node id `COMPONENT_STEP` and leaves `NEXT_NODE_PLACEHOLDER` routing untouched for downstream tooling to rewire.
- `greentic-component build` is the one-stop entrypoint: it infers `config_schema` from WIT (fallback to manifest or stub), regenerates config flows into `dev_flows`, builds the wasm (`wasm32-wasip2`), and refreshes `artifacts`/`hashes` in `component.manifest.json`. Use `--no-flow`, `--no-infer-config`, or `--no-write-schema` to dial back parts of the pipeline. Override the cargo binary via `--cargo /path/to/cargo` (or `CARGO=/path/to/cargo`) if your PATH differs from the CLI’s environment.
- Templates default to the `greentic:component/component@0.5.0` world and expose a `@config` record in WIT so config_schema/flows can be inferred automatically. `supports` in the manifest accepts `messaging`, `event`, `component_config`, `job`, or `http` depending on your surface.

## Next steps

1. Implement your domain logic in `src/lib.rs` (notably the `handle` export).
2. Extend `schemas/` and `component.manifest.json` to reflect real inputs, outputs, and capabilities.
3. Use `component-doctor` and `component-inspect` (or `make smoke`) to validate manifests and wasm artifacts.
4. Run `make build`, `make test`, and `make lint` to mirror CI locally.
5. When ready, `greentic-component new --json ...` integrates nicely with automation/CI pipelines.

> **Validation guardrails**
>
> The `new` subcommand validates component names (kebab/snake case), orgs
> (reverse-DNS like `ai.greentic`), semantic versions, and target directories up
> front. Validation failures emit actionable human output or structured JSON
> (when `--json` is set) so CI/CD pipelines can separate invalid input from
> later build failures.

> **Post-render hooks**
>
> Each `greentic-component new ...` run bootstraps a git repository (unless the
> target lives inside an existing worktree), creates an initial commit
> `chore(init): scaffold component from <template id>`, and prints a short list
> of “next step” commands (cd into the directory, run `component-doctor`, etc.)
> so freshly scaffolded projects start in a clean, versioned state. Set
> `--no-git` (or `GREENTIC_SKIP_GIT=1`) to opt out when an external tool is
> responsible for version control; structured `post_init.events[]` entries in
> the `--json` output capture each git step’s status for CI logs.

`greentic-component new --json ...` now surfaces the template description/tags
(`scaffold.template_description`, `scaffold.template_tags`) so automation can
record which template produced a component without shell parsing.

`GREENTIC_DEP_MODE` controls how dependencies are written when scaffolding:
`local` (default) injects workspace `path =` overrides so CI catches template
regressions against unpublished crates, while `cratesio` emits pure semver
constraints and fails fast if any `path =` slips into the generated
`Cargo.toml`. The dual-mode smoke tests exercise both flavors.

## Continuous Integration

- `.github/workflows/ci.yml` runs on every push/PR using the stable toolchain on `ubuntu-latest`.
- The `checks` job runs `cargo fmt`, `cargo clippy`, full workspace tests (locked + all features), targeted CLI feature tests, and verifies the published schema `$id` on pushes to `master`.
- The `smoke` job scaffolds a temporary component via `greentic-component new`,
  runs `component-doctor`, performs both `cargo check --target wasm32-wasip2`
  and `cargo build --target wasm32-wasip2 --release`, and finishes with
  `component-inspect --json`, mirroring
  `make smoke`/`ci/local_check.sh`.
- Run `ci/local_check.sh` before pushing to mirror the GitHub Actions pipeline (fmt, clippy, builds/tests, schema drift, CLI probes, and the smoke scaffold).

## Development

### Prerequisites & MSRV

- Rust stable toolchain (MSRV: 1.89)
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

Developers only need one entrypoint to mirror CI:

```bash
# Fast checks (quiet, online, non-strict)
bash ci/local_check.sh

# CI-equivalent (strict, verbose)
LOCAL_CHECK_ONLINE=1 LOCAL_CHECK_STRICT=1 LOCAL_CHECK_VERBOSE=1 bash ci/local_check.sh
```

Toggles remain available when you need a targeted run:

```bash
# Default: online, non-strict
bash ci/local_check.sh

# Force offline mode (skip schema drift curl)
LOCAL_CHECK_ONLINE=0 bash ci/local_check.sh

# Enable strict mode (enforces online schema + full feature builds/tests)
LOCAL_CHECK_ONLINE=1 LOCAL_CHECK_STRICT=1 bash ci/local_check.sh

# Temporarily skip the smoke scaffold (not recommended)
LOCAL_CHECK_SKIP_SMOKE=1 bash ci/local_check.sh

# Show every command
LOCAL_CHECK_VERBOSE=1 bash ci/local_check.sh
```

The script runs in online mode by default, gracefully skips network-dependent
steps when `LOCAL_CHECK_ONLINE=0`, scaffolds a fresh component (doctor +
`cargo check --target wasm32-wasip2`, `cargo build --target wasm32-wasip2 --release`,
then inspect) whenever registry access is available, and fails fast when
`LOCAL_CHECK_STRICT=1` is set (even if smoke scaffolding is skipped due to an
offline environment). Strict mode also forces workspace-wide
`cargo build/test --all-features`; otherwise those heavyweight steps are scoped
to the `greentic-component` crate for a faster inner loop.

The smoke phase runs twice with complementary dependency modes:

- `local` injects workspace `path =` overrides so regressions surface before publish.
- `cratesio` uses only published crates; lockfile/tree/build steps emit `[skip]` when
  `LOCAL_CHECK_ONLINE=0` (or the crates.io probe fails) unless strict mode is enabled,
  in which case the same conditions are treated as hard failures.

Both variants execute the exact commands the CI job uses:

```bash
GREENTIC_DEP_MODE=<mode> cargo run --locked -p greentic-component --features cli -- \
  new --name local-check --org ai.greentic --path "$TMPDIR/<mode>" \
  --non-interactive --no-check --json
(cd "$TMPDIR/<mode>" && cargo generate-lockfile)
(cd "$TMPDIR/<mode>" && cargo tree -e no-dev --locked \
    | tee target/local-check/tree-<mode>.txt >/dev/null)
cargo run --locked -p greentic-component --features cli --bin component-doctor -- "$TMPDIR/<mode>"
(cd "$TMPDIR/<mode>" && cargo check --target wasm32-wasip2 --locked)
(cd "$TMPDIR/<mode>" && cargo build --target wasm32-wasip2 --release --locked)
cargo run --locked -p greentic-component --features cli --bin component-hash -- \
  "$TMPDIR/<mode>/component.manifest.json"
cargo run --locked -p greentic-component --features cli --bin component-inspect -- \
  --json "$TMPDIR/<mode>/component.manifest.json"
```

Per-mode cargo trees are stored under `target/local-check/tree-<mode>.txt`
(override via `LOCAL_CHECK_TREE_DIR=...`) so failures always include a snapshot
of the resolved dependencies.

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

- **Manifest validation** (`crates/component-manifest/tests/manifest_valid.rs`): ensures well-formed manifests pass and malformed manifests (duplicate capabilities, invalid secret requirements) fail.
- **Component store** (`crates/greentic-component-store/tests/*.rs`): verifies filesystem listings, caching behaviour, and HTTP fetching via a lightweight test server.
- **Runtime binding** (`crates/greentic-component-runtime/src/binder.rs` tests): validates schema enforcement and secret resolution logic.
- **Host imports** (`crates/greentic-component-runtime/src/host_imports.rs` tests): exercises telemetry gating plus the HTTP fetch host import, including policy denial and successful request/response handling.

Add new tests alongside the relevant crate to keep runtime guarantees tight.

## Component Manifest v1

`crates/greentic-component` now owns the canonical manifest schema (`schemas/v1/component.manifest.schema.json`) and typed parser. Manifests describe an opaque `id`, human name, semantic `version`, the exported WIT `world`, and the function to call for describing configuration. Artifact metadata captures the relative wasm path plus a required `blake3` digest. Optional sections describe enforced `limits`, `telemetry` attributes, and build `provenance` (builder, commit, toolchain, timestamp).

- **Capabilities** — structured WASI + host declarations (filesystem/env/random/clocks plus secrets/state/messaging/events/http/telemetry/IaC). The `security::enforce_capabilities` helper compares a manifest against a runtime `Profile` and produces precise denials (e.g. `host.secrets.required[OPENAI_API_KEY]`). Component manifests optionally declare structured `secret_requirements` for pack tooling while keeping backwards compatibility when no secrets are needed.
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
  "supports": ["messaging", "event"],
  "profiles": {
    "default": "stateless",
    "supported": ["stateless"]
  },
  "capabilities": {
    "wasi": {
      "filesystem": {
        "mode": "none",
        "mounts": []
      },
      "random": true,
      "clocks": true
    },
    "host": {
      "messaging": {
        "inbound": true,
        "outbound": true
      }
    }
  },
  "artifacts": {"component_wasm": "component.wasm"},
  "hashes": {"component_wasm": "blake3:..."}
}
```

### Command-line tools (optional `cli` feature)

```
cargo run --features cli --bin component-inspect ./component.manifest.json --json
cargo run --features cli --bin component-doctor ./component.manifest.json
```

`component-inspect` emits a structured JSON report with manifest metadata, BLAKE3 hashes, lifecycle detection, describe payloads, and redaction hints sourced from `x-redact` annotations. Add `--strict` when warnings should become hard failures (default mode only exits non-zero on actual errors so smoke jobs can keep running while still surfacing warnings on stderr). `component-doctor` executes the full validation pipeline (schema validation, hash verification, world/ABI probe, lifecycle detection, describe resolution, and redaction summary) and exits non-zero on any failure—perfect for CI gates.

## Host HTTP Fetch

The runtime now honours `HostPolicy::allow_http_fetch`. When enabled, host imports will perform outbound HTTP requests via `reqwest`, propagate headers, and base64-encode response bodies for safe transport back to components.

## Future Work

- Implement OCI/Warg store backends.
- Expand integration coverage with real Wasm components once fixtures are available.
- Support streaming invocations via the Greentic component interface.

Contributions welcome—please run `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test` before submitting changes.

## Security

See [SECURITY.md](SECURITY.md) for guidance on `x-redact`, capability declarations, and protecting operator logs.
