use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use component_manifest::{ComponentInfo, ManifestValidator};
use jsonschema::{validator_for, Validator};
use serde_json::Value;
use wasmtime::component::Component as WasmComponent;
use wasmtime::{Config, Engine};

use crate::error::CompError;
use crate::host_imports::{build_linker, HostState};
use crate::policy::LoadPolicy;

#[derive(Debug, Clone)]
pub struct ComponentRef {
    pub name: String,
    pub locator: String,
}

pub struct Loader;

impl Default for Loader {
    fn default() -> Self {
        Self
    }
}

impl Loader {
    pub fn load(
        &self,
        cref: &ComponentRef,
        policy: &LoadPolicy,
    ) -> Result<ComponentHandle, CompError> {
        let artifact = policy
            .store
            .fetch_from_str(&cref.locator, &policy.verification)?;

        let engine = create_engine()?;
        let component = WasmComponent::from_binary(&engine, &artifact.bytes)?;

        let linker = build_linker(&engine, &policy.host)?;
        let host_state = HostState::empty(policy.host.clone());
        let mut store = wasmtime::Store::new(&engine, host_state);

        let instance = greentic_interfaces::component_v0_4::Component::instantiate(
            &mut store, &component, &linker,
        )?;
        let manifest_json = instance
            .greentic_component_node()
            .call_get_manifest(&mut store)?;
        let manifest_value: Value = serde_json::from_str(&manifest_json)?;
        let validator = ManifestValidator::new();
        let info = validator.validate_value(manifest_value.clone())?;

        let config_schema_value = manifest_value
            .get("config_schema")
            .ok_or(CompError::InvalidManifest("config_schema"))?;
        let config_schema = validator_for(config_schema_value)
            .map_err(|err| CompError::SchemaValidation(err.to_string()))?;

        Ok(ComponentHandle {
            inner: Arc::new(ComponentInner {
                cref: cref.clone(),
                info,
                config_schema: Arc::new(config_schema),
                engine,
                component,
                host_policy: policy.host.clone(),
                bindings: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub fn describe(&self, handle: &ComponentHandle) -> Result<ComponentInfo, CompError> {
        Ok(handle.inner.info.clone())
    }
}

fn create_engine() -> Result<Engine, CompError> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.async_support(false);
    config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
    Engine::new(&config).map_err(|err| CompError::Runtime(err.to_string()))
}

pub struct ComponentHandle {
    pub(crate) inner: Arc<ComponentInner>,
}

pub(crate) struct ComponentInner {
    pub(crate) cref: ComponentRef,
    pub(crate) info: ComponentInfo,
    pub(crate) config_schema: Arc<Validator>,
    pub(crate) engine: Engine,
    pub(crate) component: WasmComponent,
    pub(crate) host_policy: crate::policy::HostPolicy,
    pub(crate) bindings: Mutex<HashMap<String, TenantBinding>>,
}

#[derive(Debug, Clone)]
pub(crate) struct TenantBinding {
    pub config: Value,
    pub secrets: HashMap<String, String>,
}

impl ComponentHandle {
    pub fn info(&self) -> &ComponentInfo {
        &self.inner.info
    }

    pub fn cref(&self) -> &ComponentRef {
        &self.inner.cref
    }
}

impl Clone for ComponentHandle {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
