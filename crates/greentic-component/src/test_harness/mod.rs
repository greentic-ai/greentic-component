use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use blake3::Hasher;
use greentic_interfaces_host::component::v0_5::exports::greentic::component::node;
use greentic_interfaces_host::component::v0_5::exports::greentic::component::node::GuestIndices;
use greentic_interfaces_host::component::v0_6 as component_v0_6;
use greentic_types::TenantCtx;
use greentic_types::cbor::canonical;
use serde_json::Value;
use wasmtime::component::{Component, InstancePre, Linker};
use wasmtime::{Config, Engine, Store};

use crate::test_harness::linker::{HostState, HostStateConfig, build_linker};
use crate::test_harness::secrets::InMemorySecretsStore;
use crate::test_harness::state::{InMemoryStateStore, StateDumpEntry, StateScope};

mod linker;
mod secrets;
mod state;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComponentAbi {
    V0_5,
    V0_6,
}

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

#[derive(Debug)]
pub enum HarnessError {
    Timeout { timeout_ms: u64 },
    MemoryLimit { max_memory_bytes: usize },
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HarnessError::Timeout { timeout_ms } => {
                write!(f, "execution exceeded timeout of {timeout_ms}ms")
            }
            HarnessError::MemoryLimit { max_memory_bytes } => {
                write!(
                    f,
                    "execution exceeded memory limit of {max_memory_bytes} bytes"
                )
            }
        }
    }
}

impl std::error::Error for HarnessError {}

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
    pub config: Option<Value>,
    pub allow_http: bool,
    pub timeout_ms: u64,
    pub max_memory_bytes: usize,
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
    component: Component,
    linker: Linker<HostState>,
    instance_pre: InstancePre<HostState>,
    guest_indices: Option<GuestIndices>,
    abi: ComponentAbi,
    state_store: Arc<InMemoryStateStore>,
    secrets_store: Arc<InMemorySecretsStore>,
    state_scope: StateScope,
    allow_state_read: bool,
    allow_state_write: bool,
    allow_state_delete: bool,
    exec_ctx: node::ExecCtx,
    wasi_preopens: Vec<WasiPreopen>,
    config_json: Option<String>,
    allow_http: bool,
    timeout_ms: u64,
    max_memory_bytes: usize,
    wasm_bytes_metadata: String,
}

pub struct InvokeOutcome {
    pub output_json: String,
    pub instantiate_ms: u64,
    pub run_ms: u64,
}

impl TestHarness {
    pub fn new(config: HarnessConfig) -> Result<Self> {
        let mut wasmtime_config = Config::new();
        wasmtime_config.wasm_component_model(true);
        wasmtime_config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
        wasmtime_config.epoch_interruption(true);
        let engine = Engine::new(&wasmtime_config).context("create wasmtime engine")?;

        let component =
            Component::from_binary(&engine, &config.wasm_bytes).context("load component wasm")?;
        let wasm_bytes_metadata = describe_wasm_metadata(&config.wasm_bytes);
        let abi = detect_component_abi(&config.wasm_bytes);

        let linker = build_linker(&engine)?;
        let instance_pre = linker
            .instantiate_pre(&component)
            .map_err(|err| {
                eprintln!(
                    "Linker::instantiate_pre failed ({}): {err}",
                    wasm_bytes_metadata
                );
                for source in err.chain().skip(1) {
                    eprintln!("  cause: {source}");
                }
                err
            })
            .with_context(|| {
                format!(
                    "prepare component instance (wasm metadata: {})",
                    wasm_bytes_metadata
                )
            })?;
        let guest_indices = if abi == ComponentAbi::V0_5 {
            Some(
                GuestIndices::new(&instance_pre)
                    .map_err(|err| {
                        eprintln!("GuestIndices::new failed ({}): {err}", wasm_bytes_metadata);
                        for source in err.chain().skip(1) {
                            eprintln!("  cause: {source}");
                        }
                        err
                    })
                    .with_context(|| {
                        format!(
                            "load guest indices (wasm metadata: {})",
                            wasm_bytes_metadata
                        )
                    })?,
            )
        } else {
            None
        };

        let state_store = Arc::new(InMemoryStateStore::new());
        let secrets_store = InMemorySecretsStore::new(config.allow_secrets, config.allowed_secrets);
        let secrets_store = Arc::new(secrets_store.with_secrets(config.secrets));
        let scope = StateScope::from_tenant_ctx(&config.tenant_ctx, config.state_prefix);
        for (key, value) in config.state_seeds {
            state_store.write(&scope, &key, value);
        }

        let exec_ctx = node::ExecCtx {
            tenant: make_component_tenant_ctx(&config.tenant_ctx),
            i18n_id: config.tenant_ctx.i18n_id.clone(),
            flow_id: config.flow_id,
            node_id: config.node_id,
        };

        let config_json = match config.config {
            Some(value) => Some(serde_json::to_string(&value).context("serialize config json")?),
            None => None,
        };

        Ok(Self {
            engine,
            component,
            linker,
            instance_pre,
            guest_indices,
            abi,
            state_store,
            secrets_store,
            state_scope: scope,
            allow_state_read: config.allow_state_read,
            allow_state_write: config.allow_state_write,
            allow_state_delete: config.allow_state_delete,
            exec_ctx,
            wasi_preopens: config.wasi_preopens,
            config_json,
            allow_http: config.allow_http,
            timeout_ms: config.timeout_ms,
            max_memory_bytes: config.max_memory_bytes,
            wasm_bytes_metadata,
        })
    }

    pub fn invoke(&self, operation: &str, input_json: &Value) -> Result<InvokeOutcome> {
        let host_state = HostState::new(HostStateConfig {
            base_scope: self.state_scope.clone(),
            state_store: self.state_store.clone(),
            secrets: self.secrets_store.clone(),
            allow_state_read: self.allow_state_read,
            allow_state_write: self.allow_state_write,
            allow_state_delete: self.allow_state_delete,
            wasi_preopens: self.wasi_preopens.clone(),
            allow_http: self.allow_http,
            config_json: self.config_json.clone(),
            max_memory_bytes: self.max_memory_bytes,
        })
        .context("build WASI context")?;
        let mut store = Store::new(&self.engine, host_state);
        store.limiter(|state| state.limits_mut());
        store.set_epoch_deadline(1);

        let done = Arc::new(AtomicBool::new(false));
        let _timeout_guard = TimeoutGuard::new(done.clone());
        let engine = self.engine.clone();
        let timeout_ms = self.timeout_ms;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(timeout_ms));
            if !done.load(Ordering::Relaxed) {
                engine.increment_epoch();
            }
        });

        let instantiate_start = Instant::now();
        match self.abi {
            ComponentAbi::V0_5 => {
                let guest_indices = self
                    .guest_indices
                    .as_ref()
                    .context("missing v0.5 guest indices")?;
                let instance = self
                    .instance_pre
                    .instantiate(&mut store)
                    .context("instantiate component")
                    .and_then(|instance| {
                        guest_indices
                            .load(&mut store, &instance)
                            .context("load component exports")
                            .map(|exports| (instance, exports))
                    })
                    .with_context(|| {
                        format!(
                            "failed to prepare component instance (wasm metadata: {})",
                            self.wasm_bytes_metadata
                        )
                    });

                let (_instance, exports) = match instance {
                    Ok(value) => value,
                    Err(err) => {
                        return map_invoke_error(
                            err,
                            &store,
                            self.timeout_ms,
                            self.max_memory_bytes,
                        );
                    }
                };
                let instantiate_ms = duration_ms(instantiate_start.elapsed());

                let input = serde_json::to_string(input_json).context("serialize input json")?;
                let run_start = Instant::now();
                let result = exports
                    .call_invoke(&mut store, &self.exec_ctx, operation, &input)
                    .context("invoke component");

                use greentic_interfaces_host::component::v0_5::exports::greentic::component::node::InvokeResult;

                let result = match result {
                    Ok(result) => result,
                    Err(err) => {
                        return map_invoke_error(
                            err,
                            &store,
                            self.timeout_ms,
                            self.max_memory_bytes,
                        );
                    }
                };
                let run_ms = duration_ms(run_start.elapsed());

                match result {
                    InvokeResult::Ok(output_json) => Ok(InvokeOutcome {
                        output_json,
                        instantiate_ms,
                        run_ms,
                    }),
                    InvokeResult::Err(err) => Err(anyhow::Error::new(ComponentInvokeError {
                        code: err.code,
                        message: err.message,
                        retryable: err.retryable,
                        backoff_ms: err.backoff_ms,
                        details: err.details,
                    })),
                }
            }
            ComponentAbi::V0_6 => {
                let exports = component_v0_6::ComponentV0V6V0::instantiate(
                    &mut store,
                    &self.component,
                    &self.linker,
                )
                .context("instantiate component")
                .with_context(|| {
                    format!(
                        "failed to prepare component instance (wasm metadata: {})",
                        self.wasm_bytes_metadata
                    )
                });
                let exports = match exports {
                    Ok(value) => value,
                    Err(err) => {
                        return map_invoke_error(
                            err,
                            &store,
                            self.timeout_ms,
                            self.max_memory_bytes,
                        );
                    }
                };
                let instantiate_ms = duration_ms(instantiate_start.elapsed());

                let mut payload = input_json.clone();
                if !payload.is_object() {
                    payload = serde_json::json!({ "input": payload });
                }
                if let Some(object) = payload.as_object_mut()
                    && !object.contains_key("operation")
                {
                    object.insert(
                        "operation".to_string(),
                        Value::String(operation.to_string()),
                    );
                }

                let input = canonical::to_canonical_cbor_allow_floats(&payload)
                    .context("encode invoke payload to cbor")?;
                let state = canonical::to_canonical_cbor_allow_floats(&serde_json::json!({}))
                    .context("encode state payload to cbor")?;

                let run_start = Instant::now();
                let result = exports
                    .greentic_component_component_runtime()
                    .call_run(&mut store, &input, &state)
                    .context("invoke component");
                let result = match result {
                    Ok(value) => value,
                    Err(err) => {
                        return map_invoke_error(
                            err,
                            &store,
                            self.timeout_ms,
                            self.max_memory_bytes,
                        );
                    }
                };
                let run_ms = duration_ms(run_start.elapsed());
                let output_value: Value =
                    canonical::from_cbor(&result.output).context("decode run output cbor")?;
                let output_json =
                    serde_json::to_string(&output_value).context("serialize run output json")?;
                Ok(InvokeOutcome {
                    output_json,
                    instantiate_ms,
                    run_ms,
                })
            }
        }
    }

    pub fn state_dump(&self) -> Vec<StateDumpEntry> {
        self.state_store.dump()
    }
}

fn make_component_tenant_ctx(tenant: &TenantCtx) -> node::TenantCtx {
    node::TenantCtx {
        env: tenant.env.as_str().to_string(),
        tenant: tenant.tenant.as_str().to_string(),
        tenant_id: tenant.tenant_id.as_str().to_string(),
        team: tenant.team.as_ref().map(|t| t.as_str().to_string()),
        team_id: tenant.team_id.as_ref().map(|t| t.as_str().to_string()),
        user: tenant.user.as_ref().map(|u| u.as_str().to_string()),
        user_id: tenant.user_id.as_ref().map(|u| u.as_str().to_string()),
        session_id: tenant.session_id.clone(),
        flow_id: tenant.flow_id.clone(),
        node_id: tenant.node_id.clone(),
        provider_id: tenant.provider_id.clone(),
        trace_id: tenant.trace_id.clone(),
        i18n_id: tenant.i18n_id.clone(),
        correlation_id: tenant.correlation_id.clone(),
        attributes: tenant
            .attributes
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        deadline_ms: tenant
            .deadline
            .and_then(|deadline| i64::try_from(deadline.unix_millis()).ok()),
        attempt: tenant.attempt,
        idempotency_key: tenant.idempotency_key.clone(),
        impersonation: tenant.impersonation.as_ref().map(|impersonation| {
            greentic_interfaces_host::component::v0_5::greentic::interfaces_types::types::Impersonation {
                actor_id: impersonation.actor_id.as_str().to_string(),
                reason: impersonation.reason.clone(),
            }
        }),
    }
}

struct TimeoutGuard {
    done: Arc<AtomicBool>,
}

impl TimeoutGuard {
    fn new(done: Arc<AtomicBool>) -> Self {
        Self { done }
    }
}

impl Drop for TimeoutGuard {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
    }
}

fn is_timeout_error(err: &anyhow::Error) -> bool {
    err.chain()
        .find_map(|source| source.downcast_ref::<wasmtime::Trap>())
        .is_some_and(|trap| matches!(trap, wasmtime::Trap::Interrupt))
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

fn map_invoke_error(
    err: anyhow::Error,
    store: &Store<HostState>,
    timeout_ms: u64,
    max_memory_bytes: usize,
) -> Result<InvokeOutcome> {
    if is_timeout_error(&err) {
        return Err(anyhow::Error::new(HarnessError::Timeout { timeout_ms }));
    }
    if store.data().memory_limit_hit() {
        return Err(anyhow::Error::new(HarnessError::MemoryLimit {
            max_memory_bytes,
        }));
    }
    Err(err)
}

fn detect_component_abi(bytes: &[u8]) -> ComponentAbi {
    if let Ok(decoded) = crate::wasm::decode_world(bytes) {
        let world = &decoded.resolve.worlds[decoded.world];
        if world.name == "component-v0-v6-v0" {
            return ComponentAbi::V0_6;
        }
    }
    ComponentAbi::V0_5
}

fn describe_wasm_metadata(bytes: &[u8]) -> String {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    format!("len={}, blake3:{}", bytes.len(), hasher.finalize().to_hex())
}
