use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use blake3::Hasher;
use serde_json::json;
use tempfile::TempDir;

const FIXTURE_CARGO_TOML: &str = r#"[package]
name = "contract_fixture"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
greentic-interfaces-guest = { version = "0.4", default-features = false, features = ["component-node"] }
serde_json = "1"
"#;

const FIXTURE_LIB_RS: &str = r#"use greentic_interfaces_guest::component::node::{InvokeResult, NodeError};
use greentic_interfaces_guest::component_entrypoint;
use serde_json::Value;

#[cfg(target_arch = "wasm32")]
#[used]
#[unsafe(link_section = ".greentic.wasi")]
static WASI_TARGET_MARKER: [u8; 13] = *b"wasm32-wasip2";

fn manifest() -> String {
    serde_json::json!({
        "component": {
            "name": "Contract Fixture",
            "org": "greentic",
            "version": "0.1.0",
            "world": "greentic:component/component@0.5.0"
        }
    })
    .to_string()
}

fn invoke(_op: String, input: String) -> InvokeResult {
    let parsed: Value = serde_json::from_str(&input).unwrap_or(Value::Null);
    let is_valid = match parsed.as_object() {
        Some(map) => {
            map.len() == 1
                && map
                    .get("message")
                    .and_then(|value| value.as_str())
                    .is_some()
        }
        None => false,
    };
    if is_valid {
        InvokeResult::Ok("{}".to_string())
    } else {
        InvokeResult::Err(NodeError {
            code: "INVALID_INPUT".to_string(),
            message: "expected object with message".to_string(),
            retryable: false,
            backoff_ms: None,
            details: None,
        })
    }
}

component_entrypoint!({
    manifest: manifest,
    invoke: invoke,
    invoke_stream: false,
});
"#;

fn main() -> Result<()> {
    let fixture_dir =
        PathBuf::from("crates/greentic-component/tests/contract/fixtures/component_v0_5_0");
    fs::create_dir_all(&fixture_dir)?;

    let temp = TempDir::new().context("create temp dir")?;
    let temp_path = temp.path();
    fs::write(temp_path.join("Cargo.toml"), FIXTURE_CARGO_TOML)?;
    fs::create_dir_all(temp_path.join("src"))?;
    fs::write(temp_path.join("src/lib.rs"), FIXTURE_LIB_RS)?;

    let cargo_bin = std::env::var_os("CARGO")
        .map(PathBuf::from)
        .unwrap_or_else(|| "cargo".into());
    let status = Command::new(&cargo_bin)
        .arg("build")
        .arg("--release")
        .arg("--target")
        .arg("wasm32-wasip2")
        .arg("--offline")
        .current_dir(temp_path)
        .status()
        .with_context(|| format!("failed to run {}", cargo_bin.display()))?;
    if !status.success() {
        bail!("cargo build failed with status {}", status);
    }

    let wasm_path = temp_path.join("target/wasm32-wasip2/release/contract_fixture.wasm");
    let wasm_bytes =
        fs::read(&wasm_path).with_context(|| format!("read wasm {}", wasm_path.display()))?;
    fs::write(fixture_dir.join("component.wasm"), &wasm_bytes)?;

    let hash = blake3_hash(&wasm_bytes);
    let manifest = json!({
        "id": "com.greentic.contract.fixture",
        "name": "Contract Fixture",
        "version": "0.1.0",
        "world": "greentic:component/component@0.5.0",
        "describe_export": "describe",
        "operations": [
            { "name": "handle_message", "input_schema": {}, "output_schema": {} }
        ],
        "default_operation": "handle_message",
        "supports": ["messaging"],
        "profiles": {
            "default": "stateless",
            "supported": ["stateless"]
        },
        "config_schema": {
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        },
        "dev_flows": {
            "default": {
                "format": "flow-ir-json",
                "graph": {
                    "nodes": [
                        { "id": "start", "type": "start" },
                        { "id": "end", "type": "end" }
                    ],
                    "edges": [
                        { "from": "start", "to": "end" }
                    ]
                }
            }
        },
        "capabilities": {
            "wasi": {
                "filesystem": {
                    "mode": "none",
                    "mounts": []
                },
                "random": true,
                "clocks": true
            },
            "host": {
                "messaging": {
                    "inbound": true,
                    "outbound": true
                },
                "telemetry": {
                    "scope": "tenant"
                }
            }
        },
        "limits": {
            "memory_mb": 64,
            "wall_time_ms": 1000,
            "fuel": 10,
            "files": 2
        },
        "telemetry": {
            "span_prefix": "contract.fixture",
            "attributes": {
                "component": "fixture"
            },
            "emit_node_spans": true
        },
        "provenance": {
            "builder": "greentic-component",
            "git_commit": "abcdef1",
            "toolchain": "rustc",
            "built_at_utc": "2024-01-01T00:00:00Z"
        },
        "artifacts": { "component_wasm": "component.wasm" },
        "hashes": { "component_wasm": hash }
    });
    fs::write(
        fixture_dir.join("component.manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    println!("wrote contract fixture to {}", fixture_dir.display());
    Ok(())
}

fn blake3_hash(bytes: &[u8]) -> String {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    format!("blake3:{}", hex::encode(hasher.finalize().as_bytes()))
}
