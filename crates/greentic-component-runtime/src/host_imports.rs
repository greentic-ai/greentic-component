use std::collections::HashMap;
use std::convert::TryFrom;

use greentic_interfaces::runner_host_v1::{self, RunnerHost};
use greentic_interfaces_host::component::v0_4::{
    self, ControlHost, exports::greentic::component::node,
};
use greentic_types::TenantCtx;
use reqwest::blocking::Client as HttpClient;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use wasmtime::component::Linker;
use wasmtime::{Engine, Result as WasmtimeResult};

use crate::error::CompError;
use crate::loader::ComponentRef;
use crate::policy::HostPolicy;

#[derive(Debug, Clone)]
pub struct HostState {
    _tenant: Option<TenantCtx>,
    _config: Value,
    _secrets: HashMap<String, Vec<u8>>,
    policy: HostPolicy,
    http_client: HttpClient,
}

impl HostState {
    pub fn empty(policy: HostPolicy) -> Self {
        Self {
            _tenant: None,
            _config: Value::Null,
            _secrets: HashMap::new(),
            policy,
            http_client: HttpClient::new(),
        }
    }

    pub fn from_binding(
        tenant: TenantCtx,
        config: Value,
        secrets: HashMap<String, Vec<u8>>,
        policy: HostPolicy,
    ) -> Self {
        Self {
            _tenant: Some(tenant),
            _config: config,
            _secrets: secrets,
            policy,
            http_client: HttpClient::new(),
        }
    }
}

pub fn build_linker(engine: &Engine, _policy: &HostPolicy) -> Result<Linker<HostState>, CompError> {
    let mut linker = Linker::<HostState>::new(engine);
    runner_host_v1::add_to_linker(&mut linker, |state: &mut HostState| state)?;
    v0_4::add_control_to_linker(&mut linker, |state: &mut HostState| state)?;
    Ok(linker)
}

impl ControlHost for HostState {
    fn should_cancel(&mut self) -> bool {
        false
    }

    fn yield_now(&mut self) {}
}

impl RunnerHost for HostState {
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
        Ok(None)
    }

    fn kv_put(&mut self, _ns: String, _key: String, _val: String) -> WasmtimeResult<()> {
        Ok(())
    }
}

pub fn make_exec_ctx(cref: &ComponentRef, tenant: &TenantCtx) -> node::ExecCtx {
    node::ExecCtx {
        tenant: make_component_tenant_ctx(tenant),
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

    fn host_state(allow_http: bool) -> HostState {
        HostState {
            _tenant: None,
            _config: Value::Null,
            _secrets: HashMap::new(),
            policy: HostPolicy {
                allow_http_fetch: allow_http,
                allow_telemetry: true,
            },
            http_client: HttpClient::new(),
        }
    }

    #[test]
    fn http_fetch_denied_by_policy() {
        let mut host = host_state(false);
        let result = RunnerHost::http_request(
            &mut host,
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
        let mut host = host_state(true);
        let response = RunnerHost::http_request(&mut host, "GET".into(), url, vec![], None)
            .expect("http fetch");
        let body = response.expect("http ok");
        assert_eq!(body, b"hello");
    }
}
