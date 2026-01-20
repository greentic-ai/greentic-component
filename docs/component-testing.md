# Component Testing

Short, example-driven notes for running component tests with `greentic-component test`. This is
meant to be copy-pasted into a component repo and extended as your matrix grows.

## Quick start

Run a single operation with JSON input:

```bash
greentic-component test \
  --wasm ./target/wasm32-wasip2/release/my_component.wasm \
  --op render \
  --input ./tests/fixtures/render.json \
  --pretty
```

Use inline JSON:

```bash
greentic-component test \
  --wasm ./target/wasm32-wasip2/release/my_component.wasm \
  --op render \
  --input-json '{"card":{"type":"AdaptiveCard","body":[],"version":"1.5"}}'
```

## In-memory state store

The test harness uses an in-memory state store scoped by tenant + flow/session prefix. Use
`--state-set` to seed values and `--state-dump` to inspect the store after the run.

```bash
greentic-component test \
  --wasm ./target/wasm32-wasip2/release/my_component.wasm \
  --op render \
  --input ./tests/fixtures/with_state.json \
  --state-set form_data=eyJmb28iOiJiYXIifQ== \
  --state-dump
```

## Interaction testing

Run multiple steps by repeating `--op` and `--input` with `--step` markers between them.
This lets you model a render + submit flow and verify state updates.

```bash
greentic-component test \
  --wasm ./target/wasm32-wasip2/release/my_component.wasm \
  --op render \
  --input ./tests/fixtures/render.json \
  --step \
  --op submit \
  --input ./tests/fixtures/submit.json
```

## Trace output and replay

Use `--trace-out` (or `GREENTIC_TRACE_OUT`) to save a runner-compatible `trace.json`.
On failure the CLI prints a replay hint:

```bash
greentic-component test \
  --wasm ./target/wasm32-wasip2/release/my_component.wasm \
  --op render \
  --input ./tests/fixtures/bad.json \
  --trace-out ./trace.json
```

Output:

```text
#TRY_SAVE_TRACE ./trace.json
```

## Suggested repo layout

```
tests/
  fixtures/
    render.json
    submit.json
  matrix_spec.yaml
  README/
    sample.gtest
```
