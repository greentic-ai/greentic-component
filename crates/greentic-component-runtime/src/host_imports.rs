use std::collections::{HashMap, VecDeque};
use std::convert::TryFrom;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use greentic_interfaces_host::component::v0_4::{
    self, ControlHost, exports::greentic::component::node,
};
use greentic_interfaces_host::host_import::v0_4::{
    self as host_import_v0_4,
    greentic::host_import::{http, secrets, telemetry},
    greentic::types_core::types as core_types,
};
use greentic_types::TenantCtx;
use reqwest::blocking::Client as HttpClient;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Map, Value};
use tracing::debug;
use wasmtime::component::Linker;
use wasmtime::{Engine, Result as WasmtimeResult};

use crate::error::CompError;
use crate::loader::ComponentRef;
use crate::policy::HostPolicy;

#[derive(Debug, Clone)]
pub struct HostState {
    _tenant: Option<TenantCtx>,
    _config: Value,
    secrets: HashMap<String, String>,
    policy: HostPolicy,
    http_client: HttpClient,
}

impl HostState {
    pub fn empty(policy: HostPolicy) -> Self {
        Self {
            _tenant: None,
            _config: Value::Null,
            secrets: HashMap::new(),
            policy,
            http_client: HttpClient::new(),
        }
    }

    pub fn from_binding(
        tenant: TenantCtx,
        config: Value,
        secrets: HashMap<String, String>,
        policy: HostPolicy,
    ) -> Self {
        Self {
            _tenant: Some(tenant),
            _config: config,
            secrets,
            policy,
            http_client: HttpClient::new(),
        }
    }
}

pub fn build_linker(engine: &Engine, _policy: &HostPolicy) -> Result<Linker<HostState>, CompError> {
    let mut linker = Linker::<HostState>::new(engine);
    host_import_v0_4::add_to_linker(&mut linker, |state: &mut HostState| state)?;
    v0_4::add_control_to_linker(&mut linker, |state: &mut HostState| state)?;
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
    fn emit(&mut self, span_json: String, _ctx: Option<core_types::TenantCtx>) {
        if !self.policy.allow_telemetry {
            debug!(
                "dropping telemetry event because policy denies telemetry: {}",
                span_json
            );
            return;
        }
        debug!("component telemetry: {}", span_json);
    }
}

impl http::Host for HostState {
    fn fetch(
        &mut self,
        req: http::HttpRequest,
        _ctx: Option<core_types::TenantCtx>,
    ) -> Result<http::HttpResponse, core_types::IfaceError> {
        if !self.policy.allow_http_fetch {
            return Err(core_types::IfaceError::Denied);
        }

        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|_| core_types::IfaceError::InvalidArg)?;
        let url = req
            .url
            .parse::<reqwest::Url>()
            .map_err(|_| core_types::IfaceError::InvalidArg)?;

        let mut builder = self.http_client.request(method, url);

        if let Some(headers_json) = req.headers_json.as_ref() {
            let headers_value: Value = serde_json::from_str(headers_json)
                .map_err(|_| core_types::IfaceError::InvalidArg)?;
            let headers_map = headers_value
                .as_object()
                .ok_or(core_types::IfaceError::InvalidArg)?;
            let mut header_map = HeaderMap::new();
            for (key, value) in headers_map {
                let header_name = HeaderName::from_bytes(key.as_bytes())
                    .map_err(|_| core_types::IfaceError::InvalidArg)?;
                match value {
                    Value::Array(values) => {
                        let mut queue: VecDeque<&Value> = values.iter().collect();
                        while let Some(entry) = queue.pop_front() {
                            if let Some(s) = entry.as_str() {
                                let header_value = HeaderValue::from_str(s)
                                    .map_err(|_| core_types::IfaceError::InvalidArg)?;
                                header_map.append(header_name.clone(), header_value);
                            } else {
                                return Err(core_types::IfaceError::InvalidArg);
                            }
                        }
                    }
                    Value::String(single) => {
                        let header_value = HeaderValue::from_str(single)
                            .map_err(|_| core_types::IfaceError::InvalidArg)?;
                        header_map.append(header_name, header_value);
                    }
                    _ => return Err(core_types::IfaceError::InvalidArg),
                }
            }
            builder = builder.headers(header_map);
        }

        if let Some(body) = req.body {
            let bytes = decode_body(body);
            builder = builder.body(bytes);
        }

        let response = builder.send().map_err(|err| {
            debug!("http.fetch request failed: {}", err);
            core_types::IfaceError::Unavailable
        })?;

        let status = response.status().as_u16();
        let headers_json = serialize_headers(response.headers());
        let body_bytes = response.bytes().map_err(|err| {
            debug!("http.fetch failed to read body: {}", err);
            core_types::IfaceError::Unavailable
        })?;
        let body_base64 = if body_bytes.is_empty() {
            None
        } else {
            Some(BASE64_STANDARD.encode(&body_bytes))
        };

        Ok(http::HttpResponse {
            status,
            headers_json,
            body: body_base64,
        })
    }
}

impl host_import_v0_4::HostImports for HostState {
    fn secrets_get(
        &mut self,
        key: String,
        ctx: Option<core_types::TenantCtx>,
    ) -> WasmtimeResult<Result<String, core_types::IfaceError>> {
        Ok(<Self as secrets::Host>::get(self, key, ctx))
    }

    fn telemetry_emit(
        &mut self,
        span_json: String,
        ctx: Option<core_types::TenantCtx>,
    ) -> WasmtimeResult<()> {
        <Self as telemetry::Host>::emit(self, span_json, ctx);
        Ok(())
    }

    fn http_fetch(
        &mut self,
        req: http::HttpRequest,
        ctx: Option<core_types::TenantCtx>,
    ) -> WasmtimeResult<Result<http::HttpResponse, core_types::IfaceError>> {
        Ok(<Self as http::Host>::fetch(self, req, ctx))
    }
}

pub fn make_exec_ctx(cref: &ComponentRef, tenant: &TenantCtx) -> node::ExecCtx {
    node::ExecCtx {
        tenant: make_component_tenant_ctx(tenant),
        flow_id: cref.name.clone(),
        node_id: None,
    }
}

fn decode_body(body: String) -> Vec<u8> {
    BASE64_STANDARD
        .decode(body.as_bytes())
        .unwrap_or_else(|_| body.into_bytes())
}

fn serialize_headers(headers: &HeaderMap) -> Option<String> {
    if headers.is_empty() {
        return None;
    }

    let mut map = Map::new();
    for (name, value) in headers.iter() {
        let entry = map
            .entry(name.as_str().to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        let Value::Array(arr) = entry else {
            continue;
        };
        match value.to_str() {
            Ok(text) => arr.push(Value::String(text.to_string())),
            Err(_) => arr.push(Value::String(BASE64_STANDARD.encode(value.as_bytes()))),
        }
    }

    if map.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&map).unwrap_or_default())
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
    use greentic_interfaces_host::host_import::v0_4::greentic::host_import::http::Host as HttpHost;
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
            secrets: HashMap::new(),
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
        let req = http::HttpRequest {
            method: "GET".into(),
            url: "http://localhost".into(),
            headers_json: None,
            body: None,
        };

        let result = HttpHost::fetch(&mut host, req, None);
        assert!(matches!(result, Err(core_types::IfaceError::Denied)));
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
        let req = http::HttpRequest {
            method: "GET".into(),
            url,
            headers_json: None,
            body: None,
        };

        let response = HttpHost::fetch(&mut host, req, None).expect("http fetch succeeds");
        assert_eq!(response.status, 200);
        let body = response.body.expect("body present");
        let decoded = BASE64_STANDARD
            .decode(body.as_bytes())
            .expect("decode body");
        assert_eq!(decoded, b"hello");
        assert!(response.headers_json.is_some());
    }
}
