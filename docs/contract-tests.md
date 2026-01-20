# Contract Tests

These tests exercise the component harness against fixture inputs per WIT world.

## Running locally

```
cargo test -p greentic-component --features cli --test contract_tests
```

Optional fuzz (off by default):

```
GREENTIC_FUZZ=1 cargo test -p greentic-component --features "cli fuzz" --test contract_tests
```

## Fixtures

Each world has a fixture directory under `crates/greentic-component/tests/contract/fixtures/`:

```
component_v0_5_0/
  component.wasm
  component.manifest.json
  valid_inputs/
  invalid_inputs/
```

The test runner skips worlds if the fixture component is missing.

## Adding a world

1. Create a new fixture directory under `tests/contract/fixtures/<world-id>/`.
2. Add `component.wasm` and `component.manifest.json`.
3. Add `valid_inputs/*.json` and `invalid_inputs/*.json`.
4. Register the world in `crates/greentic-component/tests/contract/mod.rs`.

## Failure artifacts

When a contract case fails, the runner writes a bundle under:

```
target/contract-artifacts/<world>/<timestamp>/
  input.json
  output.json
```

Use the bundle to reproduce the exact case with `greentic-component test`.
