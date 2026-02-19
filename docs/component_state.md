# Component state + flow payloads

This repo aligns components with the canonical state-store contract. Components talk to state via the `greentic:state/store@1.0.0` WIT interface (read/write/delete), and the runtime shim wires that interface to an in-memory store.

Canonical target: `component@0.6.0`. Legacy compatibility notes are maintained in `docs/vision/legacy.md`.

## State store access

- **Capability gate**: Components declare `capabilities.host.state` in their manifest with `read`, `write`, and/or `delete`. The manifest schema accepts all three flags.
- **Runtime shim**: The runtime host implements the state-store interface with an in-memory `HashMap`.
- **Legacy KV (deprecated compatibility path)**: `kv_get`/`kv_put` are legacy runner-host imports. In this runtime they are routed to the state store with a `"{namespace}:{key}"` composite key; they are not the canonical API. Prefer `greentic:state/store@1.0.0`.

Note: until the typed capability model in `greentic-types` grows a dedicated `delete` flag, this repo normalizes `delete: true` to `write: true` during manifest parsing.

## Read / write / delete from a component

The canonical interface lives in `greentic:state/store@1.0.0`:

- **Read**: `read(key, ctx)` returns `result<list<u8>, host-error>`.
- **Write**: `write(key, bytes, ctx)` returns `result<op-ack, host-error>`.
- **Delete**: `delete(key, ctx)` returns `result<op-ack, host-error>`.

The runtime shim enforces its policy flags (`allow_state_read/write/delete`) and returns a host error when an operation is denied.

## Accessing payloads from previous nodes in a flow

The runtime here does not inject prior-node payloads into component input or read them from output. A component invocation receives:

- `ExecCtx` (tenant + flow metadata)
- `operation` (string)
- `input` (raw JSON string)

Any prior-node data wiring happens in the external flow runner/orchestrator (typically via templating), not by reading state automatically.

## Source pointers

- State store host imports: `crates/greentic-component-runtime/src/host_imports.rs`
- Invocation path (no implicit state injection): `crates/greentic-component-runtime/src/invoker.rs`
- Manifest schema: `crates/greentic-component/schemas/v1/component.manifest.schema.json`
- Capability enforcement: `crates/greentic-component/src/security.rs`
