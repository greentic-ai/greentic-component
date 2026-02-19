# Repository Overview

## 1. High-Level Purpose
- Rust workspace providing Greentic component authoring and packaging tooling: manifest/schema validation, capability enforcement, hashing/signing, and local inspection for WASI-Preview2 components.
- Ships a CLI (`greentic-component` plus doctor/hash/inspect tools) and supporting libraries for manifest validation, artifact fetching/verification, and a lightweight invocation library used for tests/dev (production host bindings live in runner/greentic-secrets).

## 2. Main Components and Functionality
- **Path:** `crates/greentic-component`  
  **Role:** Main public crate and CLI entrypoint. Exposes the component API (manifest parsing/validation, capability enforcement, telemetry, signing) and binaries (`greentic-component`, `component-doctor`, `component-hash`, `component-inspect`).  
  **Key functionality:** Manifest parsing/validation and schema handling; capability/limit management; provenance and security checks; signing and hash verification; prepare/loader helpers; CLI scaffolding and inspection tools (with `cli` feature).  
  **Notes:** Feature-gated modules for ABI inspection, describe payloads, loader/prepare, and CLI.

- **Path:** `crates/component-manifest`  
  **Role:** Schema and types for component manifests.  
  **Key functionality:** Validates component config schemas and exposes strongly typed manifest structures (`ComponentManifest`, exports, capabilities, compatibility metadata).

- **Path:** `crates/greentic-component-store`  
  **Role:** Artifact fetcher with caching and verification.  
  **Key functionality:** Fetches components from filesystem, HTTP (feature-gated), OCI, and Warg; computes cache keys; verifies digests/signatures; persists validated artifacts; extracts ABI/provider/capability metadata from WIT/producers metadata to enforce compatibility policies.  
  **Notes:** Provides verification policy/digest utilities reused by the main crate.

- **Path:** `crates/greentic-component-runtime`  
  **Role:** Runtime loader/invoker library built on Wasmtime components for local/test usage.  
  **Key functionality:** Loads components with policy controls, describes manifests, binds tenant configuration/secrets provided by the caller, and invokes exported operations with JSON inputs/outputs. Runtime invocation now targets `component@0.6.0` contract shapes (`InvocationEnvelope` / CBOR output decoding) and avoids `component_v0_4` tenant/impersonation paths. Secrets-store and other production host bindings belong in greentic-runner/greentic-secrets.

- **Path:** `ci/local_check.sh`, `.github/workflows/*`  
  **Role:** CI/local verification scripts and workflows (lint, tests, publish, release assets, auto-tag).  
  **Key functionality:** Mirrors CI locally; includes canonical WIT duplication guard and canonical bindings import guard (fails on `greentic_interfaces::bindings::*` and `bindings::greentic::*` usage), then build/tests, cargo publish (already-exists errors tolerated), binstall artifact builds, and creates/updates GitHub Releases using the plain version tag (e.g., `v0.4.10`). Auto-tag still bumps versions.

## 3. Work In Progress, TODOs, and Stubs
- Component manifests now allow optional `secret_requirements` (validated via `greentic-types` rules: SecretKey pattern, env/tenant scope, schema must be object). Keep downstream consumers/schema docs aligned if fields evolve.
- Runtime does not provide secrets-store; secret resolution/storage belongs to greentic-runner + greentic-secrets. HostState can carry injected secrets for tests/binder but no host bindings are exposed here.
- Templates and docs target `greentic:component/component@0.5.0` and accept expanded `supports` (`messaging`, `event`, `component_config`, `job`, `http`); keep downstream references in sync if interfaces bump again.
- Config inference + flow regeneration is integrated into `greentic-component build`; flows are embedded into `dev_flows` (FlowIR JSON) and manifests are updated with inferred `config_schema` when missing.
- Downstream consumers (packc/runner/deployer) must read `secret_requirements` from component manifests/metadata; this repo only validates and emits it.

## 4. Broken, Failing, or Conflicting Areas
- `ci/local_check.sh` currently reports failures unrelated to this PR surface:
  - lockfile drift with `--locked` steps (local branch has dependency/version churn),
  - `cargo fmt --check` diffs in pre-existing files (including `crates/greentic-component/src/test_harness/mod.rs`),
  - upstream `greentic-interfaces-guest` resolution error in local/crates.io contexts (`greentic_component_0_6_0_component` missing in generated `bindings`).

## 5. Notes for Future Work
- If crates.io remains unreachable, publishing/packaging steps will continue to skip/fail; rerun when network is available.
- `.codex/PR-01-interfaces.md` defines a downstream policy: consumers should import WIT types from `greentic_interfaces::canonical` and avoid `greentic_interfaces::bindings::*` in app/library/tests/docs code.
