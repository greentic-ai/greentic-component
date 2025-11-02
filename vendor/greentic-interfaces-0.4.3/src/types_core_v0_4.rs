//! Typed bindings for `greentic:types-core@0.4.0`.
#![allow(missing_docs)]

mod bindings {
    wasmtime::component::bindgen!({
        path: "target/wit-bindgen/greentic-types-core-0.4.0",
        world: "core",
    });
}

pub use bindings::*;

/// Returns the canonical package name for compatibility checks.
pub const PACKAGE_ID: &str = "greentic:types-core@0.4.0";
