# ACT-PR-02 — greentic-component: Trace Output Compatibility with greentic-runner

## Goal
Allow component-level testing to produce the same `trace.json` structure so failures are replayable.

## Scope
### Add
- `--trace-out <path>` and `GREENTIC_TRACE_OUT` support
- Include in trace:
  - component id/op
  - input/output hashes
  - duration
  - error block

### Compatibility
- Emit `trace_version: 1` aligned with runner.

## Acceptance criteria
- When a component test fails (e.g., invalid schema), trace.json is produced.
- Integration tester can `#TRY_SAVE_TRACE` and print replay hint.

## Test plan
- Add an integration test that fails validation and asserts trace file exists.
