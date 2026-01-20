use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use greentic_interfaces_host::component::v0_5::exports::greentic::component::node;
use greentic_interfaces_host::component::v0_5::exports::greentic::component::node::GuestIndices;
use greentic_types::TenantCtx;
use serde_json::Value;
use wasmtime::component::{Component, InstancePre};
use wasmtime::{Config, Engine, Store};

use crate::test_harness::linker::{HostState, build_linker};
use crate::test_harness::secrets::InMemorySecretsStore;
use crate::test_harness::state::{InMemoryStateStore, StateDumpEntry, StateScope};

mod linker;
mod secrets;
mod state;

#[derive(Debug)]
pub struct ComponentInvokeError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub backoff_ms: Option<u64>,
    pub details: Option<String>,
}

impl std::fmt::Display for ComponentInvokeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "component error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for ComponentInvokeError {}

pub struct HarnessConfig {
    pub wasm_bytes: Vec<u8>,
    pub tenant_ctx: TenantCtx,
    pub flow_id: String,
    pub node_id: Option<String>,
    pub state_prefix: String,
    pub state_seeds: Vec<(String, Vec<u8>)>,
    pub allow_state_read: bool,
    pub allow_state_write: bool,
    pub allow_state_delete: bool,
    pub allow_secrets: bool,
    pub allowed_secrets: HashSet<String>,
    pub secrets: HashMap<String, String>,
    pub wasi_preopens: Vec<WasiPreopen>,
}

#[derive(Clone, Debug)]
pub struct WasiPreopen {
    pub host_path: PathBuf,
    pub guest_path: String,
    pub read_only: bool,
}

impl WasiPreopen {
    pub fn new(host_path: impl Into<PathBuf>, guest_path: impl Into<String>) -> Self {
        Self {
            host_path: host_path.into(),
            guest_path: guest_path.into(),
            read_only: false,
        }
    }

    pub fn read_only(mut self, value: bool) -> Self {
        self.read_only = value;
        self
    }
}

pub struct TestHarness {
    engine: Engine,
    instance_pre: InstancePre<HostState>,
    guest_indices: GuestIndices,
    state_store: Arc<InMemoryStateStore>,
    secrets_store: Arc<InMemorySecretsStore>,
    state_scope: StateScope,
    allow_state_read: bool,
    allow_state_write: bool,
    allow_state_delete: bool,
    exec_ctx: node::ExecCtx,
    wasi_preopens: Vec<WasiPreopen>,
}

impl TestHarness {
    pub fn new(config: HarnessConfig) -> Result<Self> {
        let mut wasmtime_config = Config::new();
        wasmtime_config.wasm_component_model(true);
        wasmtime_config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
        let engine = Engine::new(&wasmtime_config).context("create wasmtime engine")?;

        let component =
            Component::from_binary(&engine, &config.wasm_bytes).context("load component wasm")?;

        let linker = build_linker(&engine)?;
        let instance_pre = linker
            .instantiate_pre(&component)
            .context("prepare component instance")?;
        let guest_indices = GuestIndices::new(&instance_pre).context("load guest indices")?;

        let state_store = Arc::new(InMemoryStateStore::new());
        let secrets_store = InMemorySecretsStore::new(config.allow_secrets, config.allowed_secrets);
        let secrets_store = Arc::new(secrets_store.with_secrets(config.secrets));
        let scope = StateScope::from_tenant_ctx(&config.tenant_ctx, config.state_prefix);
        for (key, value) in config.state_seeds {
            state_store.write(&scope, &key, value);
        }

        let exec_ctx = node::ExecCtx {
            tenant: make_component_tenant_ctx(&config.tenant_ctx),
            flow_id: config.flow_id,
            node_id: config.node_id,
        };

        Ok(Self {
            engine,
            instance_pre,
            guest_indices,
            state_store,
            secrets_store,
            state_scope: scope,
            allow_state_read: config.allow_state_read,
            allow_state_write: config.allow_state_write,
            allow_state_delete: config.allow_state_delete,
            exec_ctx,
            wasi_preopens: config.wasi_preopens,
        })
    }

    pub fn invoke(&self, operation: &str, input_json: &Value) -> Result<String> {
        let host_state = HostState::new(
            self.state_scope.clone(),
            self.state_store.clone(),
            self.secrets_store.clone(),
            self.allow_state_read,
            self.allow_state_write,
            self.allow_state_delete,
            &self.wasi_preopens,
        )
        .context("build WASI context")?;
        let mut store = Store::new(&self.engine, host_state);
        let instance = self
            .instance_pre
            .instantiate(&mut store)
            .context("instantiate component")?;
        let exports = self
            .guest_indices
            .load(&mut store, &instance)
            .context("load component exports")?;

        let input = serde_json::to_string(input_json).context("serialize input json")?;
        let result = exports
            .call_invoke(&mut store, &self.exec_ctx, operation, &input)
            .context("invoke component")?;

        use greentic_interfaces_host::component::v0_5::exports::greentic::component::node::InvokeResult;

        match result {
            InvokeResult::Ok(output_json) => Ok(output_json),
            InvokeResult::Err(err) => Err(anyhow::Error::new(ComponentInvokeError {
                code: err.code,
                message: err.message,
                retryable: err.retryable,
                backoff_ms: err.backoff_ms,
                details: err.details,
            })),
        }
    }

    pub fn state_dump(&self) -> Vec<StateDumpEntry> {
        self.state_store.dump()
    }
}

fn make_component_tenant_ctx(tenant: &TenantCtx) -> node::TenantCtx {
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
