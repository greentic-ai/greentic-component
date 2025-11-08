# Changelog

## NEW
- add the `greentic-component new` and `greentic-component templates` subcommands (PR-COMP-NEW-01) with initial scaffold plumbing, template discovery, docs, and tests.
- implement the Handlebars-driven template engine plus the `rust-wasi-p2-min` built-in template for WASI-P2 scaffolds (PR-COMP-NEW-02).
- wire `greentic-component new` to the scaffold engine end to end, add the default `cargo check --target wasm32-wasip2` verification (skippable via `--no-check`), enhance JSON/human outputs, and teach `component-doctor` how to recognize scaffolded directories (PR-COMP-NEW-03).
