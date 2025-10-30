use greentic_interfaces::component_v0_4;
use greentic_types::TenantCtx;
use serde_json::Value;
use wasmtime::Store;

use crate::binder::binding_key;
use crate::error::CompError;
use crate::host_imports::{build_linker, make_exec_ctx, HostState};
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

    let mut linker = build_linker(&inner.engine, &inner.host_policy)?;
    let host_state = HostState::from_binding(
        tenant.clone(),
        binding.config.clone(),
        binding.secrets.clone(),
        inner.host_policy.clone(),
    );
    let mut store = Store::new(&inner.engine, host_state);
    let instance = component_v0_4::Component::instantiate(&mut store, &inner.component, &mut linker)?;
    let exports = instance.greentic_component_node();

    let exec_ctx = make_exec_ctx(&inner.cref, tenant);
    let input = serde_json::to_string(input_json)?;
    let result = exports.call_invoke(&mut store, exec_ctx, operation.to_string(), input)?;

    use greentic_interfaces::component_v0_4::exports::greentic::component::node::InvokeResult;

    match result {
        InvokeResult::Ok(output_json) => {
            Ok(serde_json::from_str(&output_json)?)
        }
        InvokeResult::Err(err) => {
            Err(CompError::Runtime(format!(
                "component error {}: {}",
                err.code,
                err.message
            )))
        }
    }
}
