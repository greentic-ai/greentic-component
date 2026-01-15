use std::sync::Arc;

use anyhow::Result;
use greentic_interfaces::runner_host_v1::{self, RunnerHost};
use greentic_interfaces_host::component::v0_5::{self, ControlHost};
use greentic_interfaces_wasmtime::host_helpers::v1::secrets_store::{
    SecretsError, SecretsStoreHost, add_secrets_store_to_linker,
};
use greentic_interfaces_wasmtime::host_helpers::v1::state_store::{
    OpAck, StateStoreError, StateStoreHost, TenantCtx as WitTenantCtx, add_state_store_to_linker,
};
use wasmtime::Engine;
use wasmtime::component::Linker;
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::test_harness::secrets::InMemorySecretsStore;
use crate::test_harness::state::{InMemoryStateStore, StateScope};

pub struct HostState {
    control: ControlHostImpl,
    runner: RunnerHostImpl,
    state: StateStoreHostImpl,
    secrets: SecretsStoreHostImpl,
    wasi_ctx: WasiCtx,
    wasi_table: ResourceTable,
}

impl HostState {
    pub fn new(
        base_scope: StateScope,
        state_store: Arc<InMemoryStateStore>,
        secrets: Arc<InMemorySecretsStore>,
        allow_state_read: bool,
        allow_state_write: bool,
        allow_state_delete: bool,
    ) -> Self {
        Self {
            control: ControlHostImpl,
            runner: RunnerHostImpl,
            state: StateStoreHostImpl::new(
                base_scope,
                state_store,
                allow_state_read,
                allow_state_write,
                allow_state_delete,
            ),
            secrets: SecretsStoreHostImpl::new(secrets),
            wasi_ctx: WasiCtxBuilder::new().build(),
            wasi_table: ResourceTable::new(),
        }
    }
}

pub fn build_linker(engine: &Engine) -> Result<Linker<HostState>> {
    let mut linker = Linker::<HostState>::new(engine);
    runner_host_v1::add_to_linker(&mut linker, |state: &mut HostState| &mut state.runner)?;
    v0_5::add_control_to_linker(&mut linker, |state: &mut HostState| &mut state.control)?;
    add_state_store_to_linker(&mut linker, |state: &mut HostState| &mut state.state)?;
    add_secrets_store_to_linker(&mut linker, |state: &mut HostState| &mut state.secrets)?;
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    Ok(linker)
}

pub struct ControlHostImpl;

impl ControlHost for ControlHostImpl {
    fn should_cancel(&mut self) -> bool {
        false
    }

    fn yield_now(&mut self) {}
}

pub struct RunnerHostImpl;

impl RunnerHost for RunnerHostImpl {
    fn http_request(
        &mut self,
        _method: String,
        _url: String,
        _headers: Vec<String>,
        _body: Option<Vec<u8>>,
    ) -> wasmtime::Result<Result<Vec<u8>, String>> {
        Ok(Err(
            "http fetch denied in greentic-component test harness".to_string()
        ))
    }

    fn kv_get(&mut self, _ns: String, _key: String) -> wasmtime::Result<Option<String>> {
        Ok(None)
    }

    fn kv_put(&mut self, _ns: String, _key: String, _val: String) -> wasmtime::Result<()> {
        Ok(())
    }
}

pub struct StateStoreHostImpl {
    base_scope: StateScope,
    state_store: Arc<InMemoryStateStore>,
    allow_state_read: bool,
    allow_state_write: bool,
    allow_state_delete: bool,
}

impl StateStoreHostImpl {
    fn new(
        base_scope: StateScope,
        state_store: Arc<InMemoryStateStore>,
        allow_state_read: bool,
        allow_state_write: bool,
        allow_state_delete: bool,
    ) -> Self {
        Self {
            base_scope,
            state_store,
            allow_state_read,
            allow_state_write,
            allow_state_delete,
        }
    }

    fn scope_for_ctx(&self, ctx: Option<&WitTenantCtx>) -> StateScope {
        let mut scope = self.base_scope.clone();
        if let Some(ctx) = ctx {
            if !ctx.env.is_empty() {
                scope.env = ctx.env.clone();
            }
            if !ctx.tenant.is_empty() {
                scope.tenant = ctx.tenant.clone();
            }
            if let Some(team) = &ctx.team {
                scope.team = Some(team.clone());
            }
            if let Some(user) = &ctx.user {
                scope.user = Some(user.clone());
            }
        }
        scope
    }
}

impl StateStoreHost for StateStoreHostImpl {
    fn read(
        &mut self,
        key: String,
        ctx: Option<WitTenantCtx>,
    ) -> std::result::Result<Vec<u8>, StateStoreError> {
        if !self.allow_state_read {
            return Err(StateStoreError {
                code: "state.read.denied".into(),
                message: "state store reads are disabled by manifest capability".into(),
            });
        }
        let scope = self.scope_for_ctx(ctx.as_ref());
        self.state_store
            .read(&scope, &key)
            .ok_or_else(|| StateStoreError {
                code: "state.read.miss".into(),
                message: format!("state key `{key}` not found"),
            })
    }

    fn write(
        &mut self,
        key: String,
        bytes: Vec<u8>,
        ctx: Option<WitTenantCtx>,
    ) -> std::result::Result<OpAck, StateStoreError> {
        if !self.allow_state_write {
            return Err(StateStoreError {
                code: "state.write.denied".into(),
                message: "state store writes are disabled by manifest capability".into(),
            });
        }
        let scope = self.scope_for_ctx(ctx.as_ref());
        self.state_store.write(&scope, &key, bytes);
        Ok(OpAck::Ok)
    }

    fn delete(
        &mut self,
        key: String,
        ctx: Option<WitTenantCtx>,
    ) -> std::result::Result<OpAck, StateStoreError> {
        if !self.allow_state_delete {
            return Err(StateStoreError {
                code: "state.delete.denied".into(),
                message: "state store deletes are disabled by manifest capability".into(),
            });
        }
        let scope = self.scope_for_ctx(ctx.as_ref());
        self.state_store.delete(&scope, &key);
        Ok(OpAck::Ok)
    }
}

pub struct SecretsStoreHostImpl {
    secrets: Arc<InMemorySecretsStore>,
}

impl SecretsStoreHostImpl {
    fn new(secrets: Arc<InMemorySecretsStore>) -> Self {
        Self { secrets }
    }
}

impl SecretsStoreHost for SecretsStoreHostImpl {
    fn get(
        &mut self,
        key: wasmtime::component::__internal::String,
    ) -> std::result::Result<Option<wasmtime::component::__internal::Vec<u8>>, SecretsError> {
        self.secrets.get(&key)
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.wasi_table,
        }
    }
}
