# Repository Overview

## 1. High-Level Purpose
- Rust workspace providing the Greentic component tooling: authoring, validating, packaging, distributing, and running Greentic components that target WASI-Preview2 with Wasmtime.
- Ships a CLI (`greentic-component` plus doctor/hash/inspect tools) and supporting libraries for manifest validation, artifact fetching/verification, and runtime loading/binding/invocation.

## 2. Main Components and Functionality
- **Path:** `crates/greentic-component`  
  **Role:** Main public crate and CLI entrypoint. Exposes the component API (manifest parsing/validation, capability enforcement, telemetry, signing, store access) and binaries (`greentic-component`, `component-doctor`, `component-hash`, `component-inspect`).  
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
  **Role:** Runtime loader/invoker built on Wasmtime components.  
  **Key functionality:** Loads components with policy controls, describes manifests, binds tenant configuration/secrets, and invokes exported operations with JSON inputs/outputs.

- **Path:** `ci/local_check.sh`, `.github/workflows/*`  
  **Role:** CI/local verification scripts and workflows (lint, tests, publish, release assets, auto-tag).  
  **Key functionality:** Mirrors CI locally; any push to `master` (or manual dispatch) runs build/tests, cargo publish (already-exists errors tolerated), and binstall artifact builds. GitHub Release upload occurs only for tag refs; branch runs write summaries instead of tagging. Auto-tag still bumps versions.

## 3. Work In Progress, TODOs, and Stubs
- None noted.

## 4. Broken, Failing, or Conflicting Areas
- Workspace tests (`cargo test --workspace --locked`) pass locally; no failing areas observed.

## 5. Notes for Future Work
- None noted.
