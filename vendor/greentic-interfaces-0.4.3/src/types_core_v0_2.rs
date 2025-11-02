//! Typed bindings for `greentic:types-core@0.2.0`.
#![allow(missing_docs)]

mod bindings {
    wasmtime::component::bindgen!({
        path: "wit/greentic-types-core@0.2.0.wit",
        world: "core",
    });
}

pub use bindings::*;

/// Returns the canonical package name for compatibility checks.
pub const PACKAGE_ID: &str = "greentic:types-core@0.2.0";
