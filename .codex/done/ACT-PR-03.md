# ACT-PR-03 — greentic-component: Developer Docs + Templates for Component Testing

## Goal
Make it easy for any component repo (starting with adaptive-card) to add scenario + matrix tests.

## Scope
### Add docs
- `docs/component-testing.md` covering:
  - running `greentic-component test`
  - in-memory state store behavior
  - interaction testing
  - trace output and replay

### Add templates
- `templates/tests/README/` sample `.gtest`
- `templates/tests/matrix_spec.yaml` sample

## Acceptance criteria
- A new component repo can copy templates and run a basic suite in <10 minutes.

## Notes
Keep docs short and example-driven.
