use greentic_types::TenantCtx;
use greentic_types::cbor::canonical;
use serde_json::Value;
use wasmtime::Store;

use crate::binder::binding_key;
use crate::error::CompError;
use crate::host_imports::{HostState, make_invocation_envelope};
use crate::loader::ComponentHandle;

pub fn invoke(
    handle: &ComponentHandle,
    operation: &str,
    input_json: &Value,
    tenant: &TenantCtx,
) -> Result<Value, CompError> {
    let inner = &handle.inner;

    if !inner
        .info
        .exports
        .iter()
        .any(|export| export.operation == operation)
    {
        return Err(CompError::OperationNotFound(operation.to_string()));
    }

    let key = binding_key(tenant);
    let binding = {
        let guard = inner.bindings.lock().expect("binding mutex poisoned");
        guard
            .get(&key)
            .cloned()
            .ok_or_else(|| CompError::BindingNotFound(key.clone()))?
    };

    let host_state = HostState::from_binding(
        tenant.clone(),
        binding.config.clone(),
        binding.secrets.clone(),
        inner.host_policy.clone(),
    );
    let mut store = Store::new(&inner.engine, host_state);
    let instance = inner.instance_pre.instantiate(&mut store)?;
    let exports = inner.guest_indices.load(&mut store, &instance)?;

    let payload_cbor = canonical::to_canonical_cbor_allow_floats(input_json)
        .map_err(|err| CompError::Runtime(format!("encode invoke payload failed: {err}")))?;
    let envelope = make_invocation_envelope(&inner.cref, tenant, operation, payload_cbor);
    let result = exports.call_invoke(&mut store, operation, &envelope)?;

    match result {
        Ok(output) => canonical::from_cbor(&output.output_cbor)
            .map_err(|err| CompError::Runtime(format!("decode invoke output failed: {err}"))),
        Err(err) => Err(CompError::Runtime(format!(
            "component error {}: {}",
            err.code, err.message
        ))),
    }
}
