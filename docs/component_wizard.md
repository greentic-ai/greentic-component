# Component Wizard

The component wizard generates a ready-to-edit component@0.6.0 scaffold with Greentic conventions baked in. It focuses on deterministic templates and leaves runtime integration to follow-up work.

**Quickstart**
1. `greentic-component wizard new hello-component`
2. `cd hello-component`
3. `make wasm`
4. `greentic-component doctor ./dist/hello-component__0_6_0.wasm`

**What You Get**
- `Cargo.toml` with ABI metadata.
- `wit/package.wit` containing the component@0.6.0 world exports.
- `src/lib.rs` with WIT bindings and export wiring.
- `src/descriptor.rs` for `get-component-info` and `describe`.
- `src/schema.rs` for SchemaIR and canonical CBOR helpers.
- `src/runtime.rs` for CBOR run handling.
- `src/qa.rs` with QA specs and `apply-answers`.
- `src/i18n.rs` key registry.
- `assets/i18n/en.json` default bundle for i18n keys.
- A `Makefile` with `build`, `test`, `fmt`, `clippy`, `wasm`, and `doctor` targets.

**ABI Versioning + WASM Naming**
The wizard stores ABI version in `Cargo.toml` under `[package.metadata.greentic]` and uses it to name the wasm artifact:
- Output: `dist/<name>__<abi_with_underscores>.wasm`
- Example: `dist/hello-component__0_6_0.wasm`

**QA Modes**
The template includes four QA modes: default, setup, upgrade, remove. Use `--mode` (default/setup/upgrade/remove) with `--answers` to write `examples/<mode>.answers.json` and `examples/<mode>.answers.cbor`. If `--answers` is not provided, no example answers are created.

**Doctor Validation**
`greentic-component doctor` validates the built wasm artifact for:
- required WIT exports
- QA modes and i18n coverage
- strict SchemaIR + schema hash

**Flow Integration**
After implementing your component, use Greentic Flow tooling to connect the component to a distribution client and flow registry. This keeps the wizard focused on scaffolding while flow integration is handled in the flow repo.
