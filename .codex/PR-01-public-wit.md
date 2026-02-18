0) Global rule for all repos (tell Codex this every time)

Use this paragraph at the top of every prompt:

Global policy: greentic:component@0.6.0 WIT must have a single source of truth in greentic-interfaces. No other repo should define or vendor package greentic:component@0.6.0 or world component-v0-v6-v0 in its own wit/ directory. Repos may keep tiny repo-specific worlds (e.g. messaging-provider-teams) but must depend on the canonical greentic component WIT via deps/ pointing at greentic-interfaces or via a published crate path, never by copying the WIT file contents.

B) greentic-component repo prompt (stop local WIT copies; consume canonical; fix logging)
You are working in the greentic-component repository.

Goal
- Ensure `greentic-component` scaffolding/build/doctor uses canonical WIT from greentic-interfaces, not local copied WIT files.
- Replace any local `package greentic:component@0.6.0` WIT definitions with deps that point to greentic-interfaces WIT.
- Fix misleading log output where build prints component@0.5.0 when manifest world is 0.6.0.

Work
1) Inventory:
- Search for `package greentic:component@0.6.0;` and any `world component-v0-v6-v0` definitions in this repo.
- If any exist outside tests/fixtures, remove them and replace with a dependency reference to greentic-interfaces WIT package.

2) Wiring:
- Update build.rs / wit-bindgen config / scaffold templates so that when generating bindings, the v0.6 WIT comes from greentic-interfaces.
  - Prefer using the WIT directory shipped in the greentic-interfaces(-guest) crate (path in workspace).
  - Avoid copying WIT into component templates.

3) Logging fix:
- Add integration test capturing `greentic-component build --manifest <fixture>` output.
- Ensure it prints the manifest-resolved world (0.6.0) not 0.5.0.
- Fix log statement to print resolved world.

4) Use new guest wrapper macro:
- Where fixture components are needed, replace raw wit-bindgen duplicated shape with `greentic_interfaces_guest::export_component_v060!` when possible.

Constraints
- Keep tests deterministic/offline
- Don’t delete files under `target/` (ignore them)
- If a local WIT file is repo-specific (not greentic:component), keep it.

Deliverables
- No local WIT definitions of `greentic:component@0.6.0` in this repo (except fixtures if truly needed)
- Updated scaffold to depend on canonical WIT
- Log test goes green and prints correct world

Now implement it.