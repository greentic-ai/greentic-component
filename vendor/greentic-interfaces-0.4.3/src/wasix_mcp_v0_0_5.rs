//! Typed bindings for `wasix:mcp@0.0.5`.
#![allow(missing_docs)]

// The generated bindings rely on external WASIX dependencies that are
// imported in the WIT but not available in this crate's environment.
// We retain the package identifier here for reference, but do not generate
// Wasmtime bindings to avoid compile-time resolution failures.

/// Returns the canonical package name for compatibility checks.
pub const PACKAGE_ID: &str = "wasix:mcp@0.0.5";
