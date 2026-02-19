# Legacy Surfaces (Deprecated Guidance)

This page collects compatibility surfaces that still exist but are not the canonical path for new components.

Canonical target remains `greentic:component/component@0.6.0`.

## Legacy map and replacements

1. `greentic-component test --raw-output` (legacy output envelope)
Replacement: default JSON envelope from `greentic-component test` output.

2. Legacy runner-host KV imports: `kv_get` / `kv_put`
Replacement: `greentic:state/store@1.0.0` read/write/delete API.

3. Guest feature flag naming `component-v0-6` in `greentic-interfaces-guest`
Replacement: treat as compatibility feature naming only; document external world as `component@0.6.0`.

4. Internal world symbol naming `component-v0-v6-v0`
Replacement: user-facing docs and manifests should reference `component@0.6.0`.

5. Contract fixture world `component_v0_5_0`
Replacement: add and prioritize `component@0.6.0` fixtures for ongoing contract coverage.

6. Legacy manifest schema fallback paths in runtime loader tests
Replacement: embed/resolve describe payloads from canonical `component@0.6.0` exports.

7. Legacy output wording in test docs ("raw output only")
Replacement: standardize on structured status/diagnostic JSON envelope.

8. Legacy compatibility notes mixed into primary guides
Replacement: keep primary guides v0.6-first and link here for compatibility details.

9. Legacy world naming variants in internal scaffolding strings
Replacement: maintain compatibility internally while documenting canonical world string publicly.

10. Legacy mode aliases in older scaffolds/workflows
Replacement: use wizard-supported 0.6 modes (`default`, `setup`, `update`, `remove`).

## WIT/world deprecation banner

The following identifiers are compatibility-oriented and should be treated as legacy labels in docs:

- `component-v0-v6-v0` (internal symbol form)
- `component-v0-6` (guest feature flag form)

Use `greentic:component/component@0.6.0` in user-facing docs and examples.
