//! Typed bindings for `greentic:pack-export@0.4.0`.
#![allow(missing_docs)]

mod bindings {
    wasmtime::component::bindgen!({
        path: "target/wit-bindgen/greentic-pack-export-0.4.0",
        world: "pack-exports",
    });
}

pub use bindings::*;

/// Returns the canonical package name for compatibility checks.
pub const PACKAGE_ID: &str = "greentic:pack-export@0.4.0";
