# PR-02: Docs/examples for 0.6 component authority + capability gating

## Status
- Completed on 2026-02-16.

## Goals
- Documentation and developer guidance updates for the tightened 0.6 contract.

## Implementation Steps
1) Update docs:
   - Explain: schema/secret requirements/i18n/qa-spec are component-owned and exported.
   - Explain: required capabilities must be declared; host will deny missing ones.
   - Explain: update mode semantics.

2) Add examples:
   - minimal component that requests http + secrets
   - how to provide required_capabilities in scaffolding/wizard output.

## Acceptance Criteria
- Docs mention `update` not `upgrade`.
- Docs explicitly forbid manifest-sourced schema overrides for 0.6.

## Completion Notes
- Docs updated to describe 0.6 component-owned authority for schema/secret requirements/i18n/qa-spec and to explicitly reject manifest schema overrides in 0.6 flows:
  - `docs/component-developer-guide.md`
- CLI and wizard docs updated for `update` mode semantics and explicit non-support of `upgrade` as alias:
  - `docs/cli.md`
  - `docs/component_wizard.md`
- Capability-gating guidance/examples added, including how to embed required/provided capabilities through wizard scaffolding flags:
  - `--required-capability`, `--provided-capability`
  - `docs/component_wizard.md`
  - `docs/component-developer-guide.md`
- Example artifact naming aligned with `update` mode:
  - `examples/component-wizard/hello-component/examples/update.answers.json`

## Verification
- `cargo test --workspace` passed on 2026-02-16 after doc/example-aligned code updates.

