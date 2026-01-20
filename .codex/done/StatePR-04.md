# StatePR-04 — greentic-component: Remove KV split-brain; align manifest capabilities to WIT state-store

## Repo
`greentic-component` (including `greentic-component-runtime`)

## Goal
Eliminate half-implemented alternatives and ensure this repo aligns with the canonical model:

- Components access state via `greentic:state/store@1.0.0` (read/write/delete)
- Manifest capabilities represent state-store permissions (read/write/delete)
- Runtime shim must not claim KV (`kv_get/kv_put`) is the canonical state interface

## Non-goals
- Do not implement a full runner here.
- In-memory dev state store is sufficient if runtime needs state; persistent backends are not required.

---

## Work Items

### 1) Inventory current state-related surfaces
Audit:
- `kv_get/kv_put` host imports
- stubs/no-ops for state
- documentation implying KV is canonical for components

### 2) Align runtime shim with canonical state-store
Choose one:
A) Implement canonical WIT `greentic:state/store@1.0.0` host imports for the runtime shim using in-memory storage
B) Explicitly fail with a clear error if state-store is invoked in this runtime

In both cases:
- Stop presenting KV as canonical.
- If KV is retained for legacy reasons, mark it deprecated and route it internally to state-store semantics where possible.

### 3) Manifest schema + enforcement
Update schema and enforcement so components can declare:
- state-store read/write/delete permissions
Normalize legacy fields (if any) to one internal representation so runner enforcement is consistent.

### 4) Docs cleanup
Update repo docs to match canonical rules:
- Payload is passed as component input JSON, produced as output JSON.
- Prior-node data is wired by the runner (templating), not by “reading previous payload from state” by default.
- Persistent state is via WIT state-store only.

### 5) Tests
- Capability enforcement: deny write => calling write fails
- If in-memory state store exists: read/write/delete roundtrip tests
- Ensure docs/examples do not reference deprecated KV APIs as canonical.

## Acceptance Criteria
- No docs claim kv_get/kv_put is the component state API.
- Manifest expresses state-store permissions including delete.
- Runtime shim is not misleading (either real in-memory store or explicit unsupported).
