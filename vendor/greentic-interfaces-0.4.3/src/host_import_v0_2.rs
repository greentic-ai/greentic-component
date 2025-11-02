//! Typed bindings for `greentic:host-import@0.2.0`.
#![allow(missing_docs)]

use wasmtime::component::Linker;
use wasmtime::{Result, StoreContextMut};

mod bindings {
    wasmtime::component::bindgen!({
        path: "wit/greentic-host-import@0.2.0.wit",
        world: "host-imports",
    });
}

use bindings::greentic::host_import::imports;
pub use bindings::*;

/// Trait implemented by hosts to service the component imports.
pub trait HostImports {
    fn secrets_get(
        &mut self,
        key: String,
        ctx: Option<imports::TenantCtx>,
    ) -> Result<Result<String, imports::IfaceError>>;

    fn telemetry_emit(&mut self, span_json: String, ctx: Option<imports::TenantCtx>) -> Result<()>;

    fn tool_invoke(
        &mut self,
        tool: String,
        action: String,
        args_json: String,
        ctx: Option<imports::TenantCtx>,
    ) -> Result<Result<String, imports::IfaceError>>;

    fn http_fetch(
        &mut self,
        req: imports::HttpRequest,
        ctx: Option<imports::TenantCtx>,
    ) -> Result<Result<imports::HttpResponse, imports::IfaceError>>;
}

/// Registers the host import functions with the provided linker.
pub fn add_to_linker<T>(
    linker: &mut Linker<T>,
    get_host: impl Fn(&mut T) -> &mut (dyn HostImports + Send + Sync + 'static)
    + Send
    + Sync
    + Copy
    + 'static,
) -> Result<()>
where
    T: Send + 'static,
{
    let mut inst = linker.instance("greentic:host-import/host-imports@0.2.0")?;

    inst.func_wrap(
        "secrets-get",
        move |mut caller: StoreContextMut<'_, T>,
              (key, ctx): (String, Option<imports::TenantCtx>)| {
            let host = get_host(caller.data_mut());
            host.secrets_get(key, ctx).map(|res| (res,))
        },
    )?;

    inst.func_wrap(
        "telemetry-emit",
        move |mut caller: StoreContextMut<'_, T>,
              (span, ctx): (String, Option<imports::TenantCtx>)| {
            let host = get_host(caller.data_mut());
            host.telemetry_emit(span, ctx)
        },
    )?;

    inst.func_wrap(
        "tool-invoke",
        move |
            mut caller: StoreContextMut<'_, T>,
            (tool, action, args, ctx): (String, String, String, Option<imports::TenantCtx>),
        | {
            let host = get_host(caller.data_mut());
            host.tool_invoke(tool, action, args, ctx).map(|res| (res,))
        },
    )?;

    inst.func_wrap(
        "http-fetch",
        move |mut caller: StoreContextMut<'_, T>,
              (req, ctx): (imports::HttpRequest, Option<imports::TenantCtx>)| {
            let host = get_host(caller.data_mut());
            host.http_fetch(req, ctx).map(|res| (res,))
        },
    )?;

    Ok(())
}

/// Returns the canonical package name for compatibility checks.
pub const PACKAGE_ID: &str = "greentic:host-import@0.2.0";
