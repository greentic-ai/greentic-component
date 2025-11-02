//! WIT bindings for `greentic:secrets@0.1.0`.
#![allow(missing_docs)]

/// Raw WIT document for the secrets host world.
pub const HOST_WORLD: &str = include_str!("../wit/greentic-secrets@0.1.0.wit");

/// Returns the WIT document for the secrets host world.
pub fn host_world() -> &'static str {
    HOST_WORLD
}
