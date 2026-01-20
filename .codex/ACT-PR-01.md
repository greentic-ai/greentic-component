# ACT-PR-01 — greentic-component: `greentic-component test` CLI (in-memory state-store)

## Goal
Provide a fast, component-developer-friendly test runner to execute a single component wasm with:
- input JSON
- optional secrets
- **in-memory state store**
- stable JSON output

This enables running many adaptive-card matrix cases without building full packs/flows.

## Scope
### Add
- New CLI command:
  - `greentic-component test --wasm <path> --input <json file> [--secrets <file>] [--state inmem] [--pretty]`
- In-memory state-store implementation for the test harness
- Output:
  - success: component result JSON
  - failure: structured error JSON with stable `code`

## Implementation details
- Reuse existing component execution code paths (same interfaces as runner).
- Implement a minimal host that satisfies:
  - state-store read/write/delete
  - secrets provider (file/env)
  - (optional) asset resolver (host-only; wasm can’t read disk)

## Acceptance criteria
- Running the adaptive-card component with a simple inline card returns `rendered_card`.
- Interaction input updates state in the in-memory store.

## Test plan
- Add 2 integration tests:
  - render inline card
  - submit interaction updates `form_data`

## Notes
Trace integration is next.
