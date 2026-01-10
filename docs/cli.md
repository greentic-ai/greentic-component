# CLI quick guide

Practical notes for the main `greentic-component` subcommands: what they do, key flags, and why you might tweak them. Pair this with `--help` for the full list of options.

## new
- Purpose: scaffold a new component repo from a template (default: `rust-wasi-p2-min`).
- Usage: `greentic-component new --name hello-world --org ai.greentic [--template rust-wasi-p2-min] [--path ./hello-world] [--non-interactive] [--no-git] [--no-check] [--json]`.
- Tips: keep `--no-check` off in CI unless you already built the wasm; use `--template` to point at custom templates (listed via `templates`); `--no-git` skips the init/commit step.

## templates
- Purpose: list available scaffold templates (built-in + user-provided).
- Usage: `greentic-component templates [--json]`.
- Tips: use `--json` to drive tooling/selection in scripts; template paths are shown for local overrides.

## inspect
- Purpose: inspect a component wasm + manifest without enforcing runtime checks.
- Usage: `greentic-component inspect <wasm-or-dir> [--manifest path] [--json] [--strict]`.
- Output: id, wasm path, world match, hash, supports, profiles, lifecycle exports, capabilities, limits. `--strict` turns warnings into errors.
- Tips: point `--manifest` if the wasm and manifest are not co-located; use `--json` to feed CI checks.

## hash
- Purpose: recompute and write `hashes.component_wasm` in the manifest.
- Usage: `greentic-component hash [component.manifest.json] [--wasm path]`.
- Tips: run after rebuilding the wasm; `--wasm` overrides `artifacts.component_wasm`.

## build
- Purpose: one-stop: infer/validate config schema, regenerate dev_flows, build wasm, refresh artifacts/hashes.
- Usage: `greentic-component build [--manifest path] [--cargo path] [--no-flow] [--no-infer-config] [--no-write-schema] [--force-write-schema] [--no-validate] [--json]`.
- Behavior: unless `--no-flow`, calls the same regeneration as `flow update` (fails if required defaults are missing). Builds with cargo (override via `--cargo` or `CARGO`). Removes `config_schema` from the written manifest if it was only inferred and `--no-write-schema` is set.
- Tips: keep `--no-flow` off to avoid stale dev_flows; use `--json` for CI summaries; set `CARGO` to a wrapper if you need a custom toolchain.

## flow update
- Purpose: regenerate `dev_flows.default/custom` from manifest + input schema using YGTc v2 shape.
- Usage: `greentic-component flow update [--manifest path] [--no-infer-config] [--no-write-schema] [--force-write-schema] [--no-validate]`.
- Behavior: picks the operation via `default_operation` (or only op), uses node_id = manifest.name, operation-keyed node with `input` and routing to `NEXT_NODE_PLACEHOLDER`; fails if required fields lack defaults or if `mode/kind` is `tool`.
- Tips: run after editing schemas/operations; leave `--no-write-schema` off when you want inferred schemas persisted.

## store fetch
- Purpose: fetch a component artifact from FS or OCI into a local file.
- Usage: `greentic-component store fetch --fs <component-dir> --output out.wasm` or `--oci <ref> --output out.wasm [--cache-dir dir] [--source id]`.
- Tips: pick either `--fs` or `--oci` (not both); use `--cache-dir` for repeated fetches; `--source` labels the entry in the local store.

## doctor
- Purpose: validate a wasm + manifest pair and print a health report.
- Usage: `greentic-component doctor <wasm-or-dir> [--manifest path]`.
- Output highlights:
  - `manifest schema: ok` — manifest conforms to schema; fix missing/invalid fields otherwise.
  - `hash verification: ok` — manifest hash matches wasm bytes; run `greentic-component hash` or `build` after rebuilding wasm.
  - `world check: ok` — wasm metadata matches manifest `world`; rebuild with correct WIT world if it fails.
  - `lifecycle exports: init=<bool> health=<bool> shutdown=<bool>` — optional lifecycle hooks present in the wasm. Implement `on_start`/`on_stop`/health in your guest bindings if your host expects them; omit if not needed.
  - `describe payload versions` — number of describe payloads embedded (typically 1).
  - `redaction hints` — `x-redact` markers. Logs/inspectors can leak secrets/PII if fields aren’t redacted; add `x-redact` to sensitive fields so hosts/tooling can mask them. “none” means nothing will be redacted automatically.
  - `defaults applied` — config defaults auto-applied; set defaults in `schemas/io/input.schema.json` for required fields to enable dev flows.
  - `supports` — flow kinds declared; adjust `supports` in the manifest.
  - `capabilities declared` — wasi/host surfaces requested; keep minimal for least privilege.
  - `limits configured` — whether resource limits are present; set `limits` for guardrails.
- Tips: run after `build` to catch hash/world drift; point `--manifest` if wasm and manifest differ; errors on validation/hash/world/lifecycle issues.

### Lifecycle exports (how-to)
The doctor report surfaces lifecycle booleans based on your wasm. To expose them, implement the generated guest trait for your world (or use a macro) to provide `on_start`/`on_stop`/health handlers. If your host expects these hooks, add implementations; otherwise they can remain false.

Doctor output reference
-----------------------
- `manifest schema: ok` — Manifest JSON validated against the published schema; fix missing/invalid fields if not ok.
- `hash verification: ok (blake3:...)` — Manifest hash matches wasm; run `greentic-component hash`/`build` after rebuilding wasm to refresh.
- `world check: ok (...)` — Wasm exports/metadata match manifest `world`; rebuild with the correct WIT world if it fails.
- `lifecycle exports: init=<bool> health=<bool> shutdown=<bool>` — Optional lifecycle hooks detected; implement guest bindings if the host expects startup/health/shutdown.
- `describe payload versions: N` — Number of embedded describe payloads (typically 1).
- `redaction hints: ...` — `x-redact` paths; add to sensitive fields to prevent leaking secrets/PII in logs/inspectors.
- `defaults applied: ...` — Config defaults applied; set defaults in `schemas/io/input.schema.json` (required fields should usually have defaults).
- `supports: [...]` — Flow kinds declared; set in manifest.
- `capabilities declared: ...` — Requested wasi/host surfaces; keep minimal for least privilege.
- `limits configured: true/false` — Resource limits present; set `limits` to give hosts guardrails.
