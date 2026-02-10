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
- `src/lib.rs` stubs for `describe`, `qa-spec`, `apply-answers`, and `run`.
- `src/qa.rs` with QA mode stubs for default/setup/upgrade/remove.
- `src/schemas.rs` with placeholder CBOR schema entry points.
- `src/i18n.rs` key registry.
- Example answers and schema files under `examples/`.
- A `Makefile` with `build`, `test`, `fmt`, `clippy`, `wasm`, and `doctor` targets.

**ABI Versioning + WASM Naming**
The wizard stores ABI version in `Cargo.toml` under `[package.metadata.greentic]` and uses it to name the wasm artifact:
- Output: `dist/<name>__<abi_with_underscores>.wasm`
- Example: `dist/hello-component__0_6_0.wasm`

**QA Modes**
The template includes four QA modes: default, setup, upgrade, remove. Use `--mode` with `--answers` to prefill the selected mode in the generated `src/qa.rs`.

**Doctor Validation**
`greentic-component doctor` checks the wizard scaffold for:
- required WIT exports
- QA modes
- ABI metadata
- Makefile targets

**Flow Integration**
After implementing your component, use Greentic Flow tooling to connect the component to a distribution client and flow registry. This keeps the wizard focused on scaffolding while flow integration is handled in the flow repo.
