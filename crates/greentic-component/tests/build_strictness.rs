#![cfg(all(feature = "cli", unix))]

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::Value as JsonValue;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::TempDir;

fn write_fake_cargo(dir: &Path, wasm_name: &str) -> std::path::PathBuf {
    let script = format!(
        "#!/bin/sh\nset -e\nprintf '\\x00' > \"{wasm}\"\n",
        wasm = wasm_name
    );
    let path = dir.join("fake_cargo.sh");
    fs::write(&path, script).expect("write fake cargo");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod");
    path
}

fn minimal_manifest() -> JsonValue {
    serde_json::json!({
        "id": "ai.greentic.example",
        "name": "example",
        "version": "0.1.0",
        "world": "greentic:component/component@0.5.0",
        "describe_export": "get-manifest",
        "operations": [
            {
                "name": "handle_message",
                "input_schema": {},
                "output_schema": {}
            }
        ],
        "default_operation": "handle_message",
        "config_schema": {
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "default": "Hi"
                }
            },
            "required": ["title"],
            "additionalProperties": false
        },
        "supports": ["messaging"],
        "profiles": {
            "default": "stateless",
            "supported": ["stateless"]
        },
        "secret_requirements": [],
        "capabilities": {
            "wasi": {
                "filesystem": { "mode": "none", "mounts": [] },
                "random": true,
                "clocks": true
            },
            "host": {
                "messaging": { "inbound": true, "outbound": true },
                "telemetry": { "scope": "node" },
                "secrets": { "required": [] }
            }
        },
        "limits": { "memory_mb": 64, "wall_time_ms": 1000 },
        "artifacts": { "component_wasm": "component.wasm" },
        "hashes": { "component_wasm": "blake3:0000000000000000000000000000000000000000000000000000000000000000" }
    })
}

#[test]
fn build_emits_pack_valid_config_flow() {
    let temp = TempDir::new().expect("tempdir");
    let manifest_path = temp.path().join("component.manifest.json");
    let manifest = minimal_manifest();
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .expect("write manifest");
    let schema_dir = temp.path().join("schemas/io");
    fs::create_dir_all(&schema_dir).expect("schema dir");
    fs::write(
        schema_dir.join("input.schema.json"),
        r#"{
  "type": "object",
  "properties": { "title": { "type": "string", "default": "Hi" } },
  "required": ["title"]
}"#,
    )
    .expect("write input schema");

    let fake_cargo = write_fake_cargo(temp.path(), "component.wasm");

    let mut cmd = cargo_bin_cmd!("greentic-component");
    cmd.current_dir(temp.path())
        .env("CARGO", &fake_cargo)
        .arg("build");
    cmd.assert().success();

    let manifest_after = fs::read_to_string(&manifest_path).expect("read manifest");
    let value: JsonValue = serde_json::from_str(&manifest_after).expect("manifest json");
    let operations = value["operations"].as_array().expect("operations array");
    assert_eq!(operations.len(), 1);
    let operation = operations[0].as_object().expect("operation object");
    assert_eq!(operation["name"], "handle_message");
    assert!(operation["input_schema"].is_object());
    assert!(operation["output_schema"].is_object());
    let default_flow = &value["dev_flows"]["default"];
    let template = default_flow["graph"]["nodes"]["emit_config"]["template"]
        .as_str()
        .expect("template string");
    assert!(
        !template.contains("\"tool\""),
        "config flows must not emit tool nodes"
    );
    assert!(
        template.contains("\"component.exec\""),
        "component.exec node should be emitted"
    );
    let parsed: JsonValue = serde_json::from_str(template).expect("template json");
    let exec = &parsed["node"]["component.exec"];
    assert_eq!(parsed["node_id"], "example");
    assert_eq!(exec["component"], "ai.greentic.example");
    assert_eq!(exec["operation"], "handle_message");
    assert_eq!(exec["input"]["title"], "Hi");
    assert_eq!(
        parsed["node"]["routing"][0]["to"]
            .as_str()
            .expect("routing target"),
        "NEXT_NODE_PLACEHOLDER"
    );
    assert!(
        exec["operation"]
            .as_str()
            .map(|op| !op.trim().is_empty())
            .unwrap_or(false),
        "operation should not be empty"
    );
    assert!(
        !template.contains("COMPONENT_STEP") && !template.contains("\"tool\""),
        "template should be add-step ready"
    );
}

#[test]
fn build_fails_without_operations() {
    let temp = TempDir::new().expect("tempdir");
    let manifest_path = temp.path().join("component.manifest.json");
    let mut manifest = minimal_manifest();
    manifest["operations"] = serde_json::json!([]);
    manifest
        .as_object_mut()
        .map(|obj| obj.remove("default_operation"));
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .expect("write manifest");
    let fake_cargo = write_fake_cargo(temp.path(), "component.wasm");

    let mut cmd = cargo_bin_cmd!("greentic-component");
    cmd.current_dir(temp.path())
        .env("CARGO", &fake_cargo)
        .arg("build");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(
            "Component ai.greentic.example has no operations; add at least one operation (e.g. handle_message)",
        ));
}
