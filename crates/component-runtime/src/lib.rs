mod binder;
mod error;
mod host_imports;
mod invoker;
mod loader;
mod policy;

use greentic_types::TenantCtx;
use serde_json::Value;

pub use binder::{Binder, Bindings};
pub use error::CompError;
pub use loader::{ComponentHandle, ComponentRef, Loader};
pub use policy::{HostPolicy, LoadPolicy};

pub fn load(
    cref: &ComponentRef,
    policy: &LoadPolicy,
) -> Result<ComponentHandle, CompError> {
    Loader::default().load(cref, policy)
}

pub fn describe(handle: &ComponentHandle) -> Result<ComponentManifestInfo, CompError> {
    Loader::default().describe(handle)
}

pub fn bind(
    handle: &ComponentHandle,
    tenant: &TenantCtx,
    bindings: &Bindings,
    secret_resolver: &mut dyn FnMut(&str, &TenantCtx) -> Result<String, CompError>,
) -> Result<(), CompError> {
    Binder::default().bind(handle, tenant, bindings, secret_resolver)
}

pub fn invoke(
    handle: &ComponentHandle,
    operation: &str,
    input_json: &Value,
    tenant: &TenantCtx,
) -> Result<Value, CompError> {
    invoker::invoke(handle, operation, input_json, tenant)
}

pub type ComponentManifestInfo = component_manifest::ComponentInfo;
