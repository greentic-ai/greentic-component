use std::collections::HashMap;
use std::convert::TryFrom;

use greentic_interfaces::component_v0_4::{self, exports::greentic::component::node, ControlHost};
use greentic_interfaces::host_import_v0_4::{
    self,
    greentic::host_import::{http, secrets, telemetry},
    greentic::types_core::types as core_types,
};
use greentic_types::TenantCtx;
use serde_json::Value;
use tracing::{debug, warn};
use wasmtime::component::Linker;
use wasmtime::Engine;

use crate::error::CompError;
use crate::loader::ComponentRef;
use crate::policy::HostPolicy;

#[derive(Debug, Clone)]
pub struct HostState {
    tenant: Option<TenantCtx>,
    config: Value,
    secrets: HashMap<String, String>,
    policy: HostPolicy,
}

impl HostState {
    pub fn empty(policy: HostPolicy) -> Self {
        Self {
            tenant: None,
            config: Value::Null,
            secrets: HashMap::new(),
            policy,
        }
    }

    pub fn from_binding(
        tenant: TenantCtx,
        config: Value,
        secrets: HashMap<String, String>,
        policy: HostPolicy,
    ) -> Self {
        Self {
            tenant: Some(tenant),
            config,
            secrets,
            policy,
        }
    }

    pub fn tenant(&self) -> Option<&TenantCtx> {
        self.tenant.as_ref()
    }

    pub fn config(&self) -> &Value {
        &self.config
    }
}

pub fn build_linker(engine: &Engine, _policy: &HostPolicy) -> Result<Linker<HostState>, CompError> {
    let mut linker = Linker::<HostState>::new(engine);
    host_import_v0_4::add_to_linker(&mut linker)?;
    component_v0_4::add_control_to_linker(&mut linker, |state: &mut HostState| state)?;
    Ok(linker)
}

impl ControlHost for HostState {
    fn should_cancel(&mut self) -> bool {
        false
    }

    fn yield_now(&mut self) {}
}

impl core_types::Host for HostState {}

impl secrets::Host for HostState {
    fn get(
        &mut self,
        key: String,
        _ctx: Option<core_types::TenantCtx>,
    ) -> Result<String, core_types::IfaceError> {
        match self.secrets.get(&key) {
            Some(value) => Ok(value.clone()),
            None => Err(core_types::IfaceError::NotFound),
        }
    }
}

impl telemetry::Host for HostState {
    fn emit(
        &mut self,
        span_json: String,
        _ctx: Option<core_types::TenantCtx>,
    ) {
        if !self.policy.allow_telemetry {
            debug!("dropping telemetry event because policy denies telemetry: {}", span_json);
            return;
        }
        debug!("component telemetry: {}", span_json);
    }
}

impl http::Host for HostState {
    fn fetch(
        &mut self,
        _req: http::HttpRequest,
        _ctx: Option<core_types::TenantCtx>,
    ) -> Result<http::HttpResponse, core_types::IfaceError> {
        if !self.policy.allow_http_fetch {
            return Err(core_types::IfaceError::Denied);
        }
        warn!("http.fetch host import is not implemented; returning unavailable");
        Err(core_types::IfaceError::Unavailable)
    }
}

pub fn make_exec_ctx(
    cref: &ComponentRef,
    tenant: &TenantCtx,
) -> node::ExecCtx {
    node::ExecCtx {
        tenant: make_component_tenant_ctx(tenant),
        flow_id: cref.name.clone(),
        node_id: None,
    }
}

pub fn make_component_tenant_ctx(
    tenant: &TenantCtx,
) -> node::TenantCtx {
    node::TenantCtx {
        tenant: tenant.tenant.as_str().to_string(),
        team: tenant.team.as_ref().map(|t| t.as_str().to_string()),
        user: tenant.user.as_ref().map(|u| u.as_str().to_string()),
        trace_id: tenant.trace_id.clone(),
        correlation_id: tenant.correlation_id.clone(),
        deadline_unix_ms: tenant.deadline.and_then(|deadline| {
            let millis = deadline.unix_millis();
            if millis >= 0 {
                u64::try_from(millis).ok()
            } else {
                None
            }
        }),
        attempt: tenant.attempt,
        idempotency_key: tenant.idempotency_key.clone(),
    }
}

pub fn make_host_tenant_ctx(
    tenant: &TenantCtx,
) -> core_types::TenantCtx {
    core_types::TenantCtx {
        tenant: tenant.tenant.as_str().to_string(),
        team: tenant.team.as_ref().map(|t| t.as_str().to_string()),
        user: tenant.user.as_ref().map(|u| u.as_str().to_string()),
        deployment: core_types::DeploymentCtx {
            cloud: core_types::Cloud::Other,
            region: None,
            platform: core_types::Platform::Other,
            runtime: None,
        },
        trace_id: tenant.trace_id.clone(),
    }
}
