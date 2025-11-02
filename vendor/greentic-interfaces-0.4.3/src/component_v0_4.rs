//! Typed bindings for `greentic:component@0.4.0`.
#![allow(missing_docs)]

use wasmtime::Result;
use wasmtime::StoreContextMut;
use wasmtime::component::Linker;

mod bindings {
    wasmtime::component::bindgen!({
        path: "wit/greentic-component@0.4.0.wit",
        world: "component",
    });
}

pub use self::greentic::component::control::Host as ControlHost;
pub use self::greentic::component::control::HostWithStore as ControlHostWithStore;
pub use bindings::*;

/// Registers the Greentic control interface with the provided linker.
pub fn add_control_to_linker<T>(
    linker: &mut Linker<T>,
    get_host: impl Fn(&mut T) -> &mut (dyn ControlHost + Send + Sync + 'static)
    + Send
    + Sync
    + Copy
    + 'static,
) -> Result<()>
where
    T: Send + 'static,
{
    let mut inst = linker.instance("greentic:component/control@0.4.0")?;
    inst.func_wrap(
        "should-cancel",
        move |mut caller: StoreContextMut<'_, T>, (): ()| {
            let host = get_host(caller.data_mut());
            let result = host.should_cancel();
            Ok((result,))
        },
    )?;
    inst.func_wrap(
        "yield-now",
        move |mut caller: StoreContextMut<'_, T>, (): ()| {
            let host = get_host(caller.data_mut());
            host.yield_now();
            Ok(())
        },
    )?;
    Ok(())
}

/// Returns the canonical package name for compatibility checks.
pub const PACKAGE_ID: &str = "greentic:component@0.4.0";
