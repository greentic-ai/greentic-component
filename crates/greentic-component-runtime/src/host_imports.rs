use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::{Arc, Mutex};

use greentic_interfaces::runner_host_v1::{self, RunnerHost};
use greentic_interfaces_host::component::v0_4::{
    self, ControlHost, exports::greentic::component::node,
};
use greentic_interfaces_wasmtime::host_helpers::v1::state_store::{
    OpAck, StateStoreError, StateStoreHost, TenantCtx as WitTenantCtx, add_state_store_to_linker,
};
use greentic_types::TenantCtx;
use reqwest::blocking::Client as HttpClient;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use wasmtime::component::{Linker, ResourceTable};
use wasmtime::{Engine, Result as WasmtimeResult};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView, p2};

use crate::error::CompError;
use crate::loader::ComponentRef;
use crate::policy::HostPolicy;

pub struct HostState {
    _tenant: Option<TenantCtx>,
    _config: Value,
    _secrets: HashMap<String, Vec<u8>>,
    wasi_ctx: WasiCtx,
    wasi_table: ResourceTable,
    policy: HostPolicy,
    state_store: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    runner: RunnerHostImpl,
    control: ControlHostImpl,
}

impl HostState {
    pub fn empty(policy: HostPolicy) -> Self {
        let (wasi_ctx, wasi_table) = build_wasi_state();
        let runner_policy = policy.clone();
        let state_store = policy.state_store.clone();
        Self {
            _tenant: None,
            _config: Value::Null,
            _secrets: HashMap::new(),
            wasi_ctx,
            wasi_table,
            state_store,
            policy,
            runner: RunnerHostImpl::new(runner_policy),
            control: ControlHostImpl,
        }
    }

    pub fn from_binding(
        tenant: TenantCtx,
        config: Value,
        secrets: HashMap<String, Vec<u8>>,
        policy: HostPolicy,
    ) -> Self {
        let (wasi_ctx, wasi_table) = build_wasi_state();
        let runner_policy = policy.clone();
        let state_store = policy.state_store.clone();
        Self {
            _tenant: Some(tenant),
            _config: config,
            _secrets: secrets,
            wasi_ctx,
            wasi_table,
            state_store,
            policy,
            runner: RunnerHostImpl::new(runner_policy),
            control: ControlHostImpl,
        }
    }
}

fn build_wasi_state() -> (WasiCtx, ResourceTable) {
    let mut wasi_builder = WasiCtxBuilder::new();
    (wasi_builder.build(), ResourceTable::new())
}

struct RunnerHostImpl {
    policy: HostPolicy,
    http_client: HttpClient,
}

impl RunnerHostImpl {
    fn new(policy: HostPolicy) -> Self {
        Self {
            policy,
            http_client: HttpClient::new(),
        }
    }
}

impl RunnerHost for RunnerHostImpl {
    fn http_request(
        &mut self,
        method: String,
        url: String,
        headers: Vec<String>,
        body: Option<Vec<u8>>,
    ) -> WasmtimeResult<Result<Vec<u8>, String>> {
        if !self.policy.allow_http_fetch {
            return Ok(Err("http fetch denied by policy".into()));
        }

        let method = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|err| CompError::Runtime(err.to_string()))?;
        let url = url
            .parse::<reqwest::Url>()
            .map_err(|err| CompError::Runtime(err.to_string()))?;

        let mut builder = self.http_client.request(method, url);

        if !headers.is_empty() {
            let mut header_map = HeaderMap::new();
            for entry in headers {
                if let Some((name, value)) = entry.split_once(':') {
                    let header_name = HeaderName::from_bytes(name.trim().as_bytes())
                        .map_err(|err| CompError::Runtime(err.to_string()))?;
                    let header_value = HeaderValue::from_str(value.trim())
                        .map_err(|err| CompError::Runtime(err.to_string()))?;
                    header_map.append(header_name, header_value);
                }
            }
            builder = builder.headers(header_map);
        }

        if let Some(body) = body {
            builder = builder.body(body);
        }

        let response = builder
            .send()
            .map_err(|err| CompError::Runtime(err.to_string()))?;
        let bytes = response
            .bytes()
            .map_err(|err| CompError::Runtime(err.to_string()))?;

        Ok(Ok(bytes.to_vec()))
    }

    fn kv_get(&mut self, _ns: String, _key: String) -> WasmtimeResult<Option<String>> {
        // Legacy runner-host surface; routed to the state store for compatibility.
        let key = format!("{_ns}:{_key}");
        if !self.policy.allow_state_read {
            return Ok(None);
        }
        let guard = self
            .policy
            .state_store
            .lock()
            .expect("state store mutex poisoned");
        match guard.get(&key) {
            Some(bytes) => Ok(Some(
                String::from_utf8(bytes.clone())
                    .map_err(|err| CompError::Runtime(err.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    fn kv_put(&mut self, _ns: String, _key: String, _val: String) -> WasmtimeResult<()> {
        // Legacy runner-host surface; routed to the state store for compatibility.
        if !self.policy.allow_state_write {
            return Ok(());
        }
        let key = format!("{_ns}:{_key}");
        let mut guard = self
            .policy
            .state_store
            .lock()
            .expect("state store mutex poisoned");
        guard.insert(key, _val.into_bytes());
        Ok(())
    }
}

struct ControlHostImpl;

impl ControlHost for ControlHostImpl {
    fn should_cancel(&mut self) -> bool {
        false
    }

    fn yield_now(&mut self) {}
}

pub fn build_linker(engine: &Engine, _policy: &HostPolicy) -> Result<Linker<HostState>, CompError> {
    let mut linker = Linker::<HostState>::new(engine);
    runner_host_v1::add_to_linker(&mut linker, |state: &mut HostState| &mut state.runner)?;
    v0_4::add_control_to_linker(&mut linker, |state: &mut HostState| &mut state.control)?;
    add_state_store_to_linker(&mut linker, |state: &mut HostState| state)?;
    p2::add_to_linker_sync(&mut linker)?;
    Ok(linker)
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.wasi_table,
        }
    }
}

impl StateStoreHost for HostState {
    fn read(
        &mut self,
        key: String,
        _ctx: Option<WitTenantCtx>,
    ) -> Result<Vec<u8>, StateStoreError> {
        if !self.policy.allow_state_read {
            return Err(StateStoreError {
                code: "state.read.denied".into(),
                message: "state store reads are disabled by policy".into(),
            });
        }
        let guard = self.state_store.lock().expect("state store mutex poisoned");
        match guard.get(&key) {
            Some(bytes) => Ok(bytes.clone()),
            None => Err(StateStoreError {
                code: "state.read.miss".into(),
                message: format!("state key `{key}` not found"),
            }),
        }
    }

    fn write(
        &mut self,
        key: String,
        bytes: Vec<u8>,
        _ctx: Option<WitTenantCtx>,
    ) -> Result<OpAck, StateStoreError> {
        if !self.policy.allow_state_write {
            return Err(StateStoreError {
                code: "state.write.denied".into(),
                message: "state store writes are disabled by policy".into(),
            });
        }
        let mut guard = self.state_store.lock().expect("state store mutex poisoned");
        guard.insert(key, bytes);
        Ok(OpAck::Ok)
    }

    fn delete(
        &mut self,
        key: String,
        _ctx: Option<WitTenantCtx>,
    ) -> Result<OpAck, StateStoreError> {
        if !self.policy.allow_state_delete {
            return Err(StateStoreError {
                code: "state.delete.denied".into(),
                message: "state store deletes are disabled by policy".into(),
            });
        }
        let mut guard = self.state_store.lock().expect("state store mutex poisoned");
        guard.remove(&key);
        Ok(OpAck::Ok)
    }
}

pub fn make_exec_ctx(cref: &ComponentRef, tenant: &TenantCtx) -> node::ExecCtx {
    node::ExecCtx {
        tenant: make_component_tenant_ctx(tenant),
        i18n_id: tenant.i18n_id.clone(),
        flow_id: cref.name.clone(),
        node_id: None,
    }
}

pub fn make_component_tenant_ctx(tenant: &TenantCtx) -> node::TenantCtx {
    node::TenantCtx {
        tenant: tenant.tenant.as_str().to_string(),
        team: tenant.team.as_ref().map(|t| t.as_str().to_string()),
        user: tenant.user.as_ref().map(|u| u.as_str().to_string()),
        trace_id: tenant.trace_id.clone(),
        i18n_id: tenant.i18n_id.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{ErrorKind, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn spawn_http_server() -> std::io::Result<String> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 512];
                let _ = stream.read(&mut buffer);
                let response = "\
                    HTTP/1.1 200 OK\r\n\
                    Content-Type: text/plain\r\n\
                    X-Custom: value\r\n\
                    Content-Length: 5\r\n\
                    \r\n\
                    hello";
                let _ = stream.write_all(response.as_bytes());
            }
        });

        Ok(format!("http://{}:{}/test", addr.ip(), addr.port()))
    }

    fn host_state(
        allow_http: bool,
        allow_state_read: bool,
        allow_state_write: bool,
        allow_state_delete: bool,
    ) -> HostState {
        let state_store = Arc::new(Mutex::new(HashMap::new()));
        let policy = HostPolicy {
            allow_http_fetch: allow_http,
            allow_telemetry: true,
            allow_state_read,
            allow_state_write,
            allow_state_delete,
            state_store: state_store.clone(),
        };
        HostState::empty(policy)
    }
    #[test]
    fn http_fetch_denied_by_policy() {
        let mut host = host_state(false, false, false, false);
        let result = RunnerHost::http_request(
            &mut host.runner,
            "GET".into(),
            "http://localhost".into(),
            vec![],
            None,
        );
        assert!(matches!(result, Ok(Err(_))));
    }

    #[test]
    fn http_fetch_success() {
        let url = match spawn_http_server() {
            Ok(url) => url,
            Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                eprintln!("skipping http_fetch_success: {err}");
                return;
            }
            Err(err) => panic!("bind http listener: {err}"),
        };
        let mut host = host_state(true, false, false, false);
        let response = RunnerHost::http_request(&mut host.runner, "GET".into(), url, vec![], None)
            .expect("http fetch");
        let body = response.expect("http ok");
        assert_eq!(body, b"hello");
    }

    #[test]
    fn state_store_denies_write() {
        let mut host = host_state(false, false, false, false);
        let result = StateStoreHost::write(&mut host, "demo".into(), b"data".to_vec(), None);
        assert!(matches!(result, Err(err) if err.code == "state.write.denied"));
    }

    #[test]
    fn state_store_roundtrip() {
        let mut host = host_state(false, true, true, true);
        let write = StateStoreHost::write(&mut host, "demo".into(), b"data".to_vec(), None);
        assert!(matches!(write, Ok(OpAck::Ok)));

        let read = StateStoreHost::read(&mut host, "demo".into(), None).expect("state read");
        assert_eq!(read, b"data");

        let delete = StateStoreHost::delete(&mut host, "demo".into(), None);
        assert!(matches!(delete, Ok(OpAck::Ok)));

        let missing = StateStoreHost::read(&mut host, "demo".into(), None);
        assert!(matches!(missing, Err(err) if err.code == "state.read.miss"));
    }
}
