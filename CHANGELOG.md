# Changelog

## NEW
- add the `greentic-component new` and `greentic-component templates` subcommands (PR-COMP-NEW-01) with initial scaffold plumbing, template discovery, docs, and tests.
- implement the Handlebars-driven template engine plus the `rust-wasi-p2-min` built-in template for WASI-P2 scaffolds (PR-COMP-NEW-02).
- wire `greentic-component new` to the scaffold engine end to end, add the default `cargo check --target wasm32-wasip2` verification (skippable via `--no-check`), enhance JSON/human outputs, and teach `component-doctor` how to recognize scaffolded directories (PR-COMP-NEW-03).
- surface template metadata (id/description/tags) for both built-in and user templates in `greentic-component templates --json`, document the user template lookup semantics, and add snapshot coverage (PR-COMP-NEW-04).
- harden CLI validation (names, orgs, versions, paths) with actionable diagnostics/miette reports, JSON error payloads, and additional unit tests so `greentic-component new` fails gracefully before scaffolding (PR-COMP-NEW-05).
- add a full GitHub Actions pipeline (fmt, clippy, locked workspace tests, schema drift verification) plus a smoke job that scaffolds and `cargo check`s a WASI-P2 component to guarantee the CLI stays end-to-end healthy (PR-COMP-NEW-07).
- add post-render hooks so `greentic-component new` initializes a git repo (when appropriate), commits the scaffold as `chore(init): ...`, prints a curated “next steps” checklist, emits structured post-hook events/metadata in `--json`, supports opting out via `--no-git`/`GREENTIC_SKIP_GIT=1`, and extends smoke checks (CI/local) to run doctor/inspect before `cargo check` (PR-COMP-NEW-08).
