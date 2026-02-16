# PR-01: 0.6 contract tightening (mode rename + schema authority + capabilities + canonical writes)

## Status
- Completed on 2026-02-16.

## Goals
- Align greentic-component 0.6 runtime/scaffold with the architecture:
  - Mode name becomes `update` (not upgrade).
  - **Strict post-apply validation** uses the **component-provided schema** (not manifest-sourced).
  - Capability declaration in describe payload is explicit (not empty defaults).
  - Canonical CBOR is enforced at persistence boundaries.

## Implementation Steps
1) Rename mode (mechanical):
   - Update templates and mappings:
     - `world.wit.hbs` / related WIT templates
     - any `WizardMode::Upgrade` mapping strings
     - docs: CLI strings / wizard docs
   - Keep a temporary alias for decoding if any string parsing exists here.

2) Fix schema authority (P0):
   - Identify all code paths validating config:
     - `doctor.rs` (currently only checks map/object)
     - `loader.rs` (currently reads `config_schema` from manifest_value)
     - `binder.rs` (validate_config(schema, ...))
   - Change schema source of truth to **component exports/self-description**:
     - Prefer calling exported funcs: `config-schema()` / descriptor payload field.
     - Ensure `loader`/`binder` cannot override schema via manifest for 0.6.
     - If legacy path needs manifest schema, gate behind legacy flag/version check.

3) Enforce strict validation post-apply:
   - After `apply-answers`, decode CBOR -> value.
   - Validate against schema bytes from component.
   - Provide actionable errors: which field failed, schema location if available.
   - Add tests: a deliberately invalid config produced by mocked answers must fail validation.

4) Capabilities contract (P1):
   - Stop emitting `required_capabilities: Vec::new()` and `provided_capabilities: Vec::new()` in generated code.
   - Decide where required capabilities come from:
     - either wizard config input
     - or component-declared static list
   - Ensure describe payload includes them and runtime enforcement uses the describe/manifest consistently.

5) Canonical CBOR at write boundaries (P2, but do it now if cheap):
   - Identify host/runtime writes (state/config store insert/write).
   - Ensure any CBOR bytes persisted are canonicalized via existing helper.
   - Add a test that persisted bytes are canonical (golden compare or canonicalize then equality).

6) Run & verify:
   - `cargo fmt`
   - `cargo clippy -D warnings`
   - `cargo test`

## Acceptance Criteria
- 0.6 paths never source config schema from manifest overrides; schema comes from component exports.
- `doctor` fails invalid configs with schema-based diagnostics (not only type checks).
- Describe payload contains non-empty required capabilities when configured.
- Canonical CBOR is enforced for persisted artifacts.
- Tests pass.

## Completion Notes
- Mode rename to `update` implemented across wizard/template/doctor paths:
  - `crates/greentic-component/src/cmd/wizard.rs`
  - `crates/greentic-component/src/cmd/doctor.rs`
  - `crates/greentic-component/assets/templates/component/rust-wasi-p2-min/wit/world.wit.hbs`
- Schema authority for 0.6 moved to component describe payload in runtime loader (manifest fallback only for legacy/non-0.6):
  - `crates/greentic-component-runtime/src/loader.rs`
- Strict post-apply validation in doctor now validates decoded `apply-answers` output against `describe.config_schema` with actionable field-path diagnostics:
  - `crates/greentic-component/src/cmd/doctor.rs`
- Capability declaration gap closed in wizard scaffolding via explicit flags and generated describe constants:
  - `--required-capability`, `--provided-capability`
  - `crates/greentic-component/src/cmd/wizard.rs`
  - `crates/greentic-component/tests/wizard_tests.rs`
- Canonical CBOR now enforced at runtime persistence write boundaries (state store + legacy kv writes):
  - `crates/greentic-component-runtime/src/host_imports.rs`
  - coverage: `state_store_write_canonicalizes_cbor_payload`

## Verification
- `cargo test --workspace` passed on 2026-02-16.
- Targeted suites executed successfully during closure:
  - `cargo test -p greentic-component doctor -- --nocapture`
  - `cargo test -p greentic-component wizard -- --nocapture`
  - `cargo test -p greentic-component-runtime loader -- --nocapture`
  - `cargo test -p greentic-component-runtime host_imports -- --nocapture`

