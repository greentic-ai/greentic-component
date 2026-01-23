#![cfg(feature = "cli")]

use assert_cmd::prelude::*;
use greentic_component::cmd::component_world::canonical_component_world;
use greentic_types::component::ComponentManifest as TypesManifest;
use insta::assert_snapshot;
use serde_json::Value as JsonValue;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[path = "snapshot_util.rs"]
mod snapshot_util;

use snapshot_util::normalize_text;

#[test]
fn scaffold_rust_wasi_template() {
    let temp = TempDir::new().expect("temp dir");
    let component_dir = temp.path().join("demo-component");
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("greentic-component"));
    cmd.arg("new")
        .arg("--name")
        .arg("demo-component")
        .arg("--org")
        .arg("ai.greentic")
        .arg("--path")
        .arg(&component_dir)
        .arg("--no-check")
        .env("HOME", temp.path())
        .env("GREENTIC_TEMPLATE_YEAR", "2030")
        .env("GREENTIC_TEMPLATE_ROOT", temp.path().join("templates"))
        .env("GIT_AUTHOR_NAME", "Greentic Labs")
        .env("GIT_AUTHOR_EMAIL", "greentic-labs@example.com")
        .env("GIT_COMMITTER_NAME", "Greentic Labs")
        .env("GIT_COMMITTER_EMAIL", "greentic-labs@example.com")
        .env_remove("USER")
        .env_remove("USERNAME");
    cmd.assert().success();

    let cargo = fs::read_to_string(component_dir.join("Cargo.toml")).expect("Cargo.toml");
    let manifest =
        fs::read_to_string(component_dir.join("component.manifest.json")).expect("manifest");
    let lib_rs = fs::read_to_string(component_dir.join("src/lib.rs")).expect("lib.rs");
    let input_schema = fs::read_to_string(
        component_dir
            .join("schemas")
            .join("io")
            .join("input.schema.json"),
    )
    .expect("input schema");
    let input_schema_json: JsonValue =
        serde_json::from_str(&input_schema).expect("input schema json");
    let manifest_json: JsonValue = serde_json::from_str(&manifest).expect("manifest json");
    let operations = manifest_json["operations"]
        .as_array()
        .expect("operations array in scaffold");
    assert!(
        !operations.is_empty(),
        "scaffolded manifest should include at least one operation"
    );
    let first_op = operations[0].as_object().expect("operation object");
    assert!(first_op["input_schema"].is_object());
    assert!(first_op["output_schema"].is_object());
    let first_op_name = first_op["name"].as_str().expect("operation name");
    assert_eq!(
        manifest_json["default_operation"].as_str(),
        Some(first_op_name),
        "default_operation should be set for scaffolds"
    );
    let manifest_parsed: TypesManifest =
        serde_json::from_str(&manifest).expect("manifest parses as greentic-types");
    assert!(
        !manifest_parsed.operations.is_empty(),
        "operations should deserialize"
    );
    assert_eq!(manifest_parsed.operations[0].name, "handle_message");

    assert_snapshot!("scaffold_cargo_toml", normalize_text(cargo.trim()));
    assert_snapshot!("scaffold_manifest", normalize_text(manifest.trim()));
    assert_snapshot!("scaffold_lib", normalize_text(lib_rs.trim()));
    assert_eq!(
        input_schema_json["properties"]["input"]["default"]
            .as_str()
            .expect("input default"),
        "Hello from demo-component!"
    );
    let status = Command::new("cargo")
        .arg("test")
        .current_dir(&component_dir)
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_NET_OFFLINE", "true")
        .status()
        .expect("run cargo test");
    assert!(
        status.success(),
        "scaffolded project should pass host tests"
    );
    let cargo_wrapper = component_dir.join("fake_cargo.sh");
    std::fs::write(
        &cargo_wrapper,
        r#"#!/bin/sh
set -e
REAL_CARGO="$(command -v cargo)"
"$REAL_CARGO" check --quiet
wasm_path=$(python3 - <<'PY'
import json, os
path=os.path.join(os.getcwd(),"component.manifest.json")
try:
    with open(path, "r") as f:
        data=json.load(f)
    print(data.get("artifacts", {}).get("component_wasm") or "target/wasm32-wasip2/release/component.wasm")
except Exception:
    print("target/wasm32-wasip2/release/component.wasm")
PY
)
mkdir -p "$(dirname "$wasm_path")"
printf '\0' > "$wasm_path"
"#,
    )
    .expect("write cargo wrapper");
    let mut perms = std::fs::metadata(&cargo_wrapper)
        .expect("metadata")
        .permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        std::fs::set_permissions(&cargo_wrapper, perms).expect("chmod");
    }
    let mut build = Command::new(assert_cmd::cargo::cargo_bin!("greentic-component"));
    build
        .current_dir(&component_dir)
        .env("CARGO", &cargo_wrapper)
        .env("CARGO_NET_OFFLINE", "true")
        .env("GREENTIC_SKIP_NODE_EXPORT_CHECK", "1")
        .arg("build");
    build.assert().success();
    let mut doctor = Command::new(assert_cmd::cargo::cargo_bin!("greentic-component"));
    doctor.current_dir(&component_dir).arg("doctor").arg(".");
    doctor.assert().success();
    let wit_dir = component_dir.join("wit");
    assert!(
        wit_dir.exists(),
        "template should emit WIT files for config inference"
    );
    assert!(
        wit_dir.join("world.wit").exists(),
        "world.wit should be scaffolded"
    );

    assert!(
        component_dir.join(".git").exists(),
        "post-render hook should initialize git"
    );
}

#[test]
fn doctor_validates_canonical_worlds_for_scaffold() {
    let temp = TempDir::new().expect("temp dir");
    let component_dir = temp.path().join("canonical-component");
    let reg = assert_cmd::cargo::cargo_bin!("greentic-component");
    let mut cmd = Command::new(reg);
    cmd.arg("new")
        .arg("--name")
        .arg("canonical-component")
        .arg("--org")
        .arg("ai.greentic")
        .arg("--path")
        .arg(&component_dir)
        .arg("--no-check")
        .env("HOME", temp.path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("GREENTIC_TEMPLATE_YEAR", "2030")
        .env("GREENTIC_TEMPLATE_ROOT", temp.path().join("templates"))
        .env("GIT_AUTHOR_NAME", "Greentic Labs")
        .env("GIT_AUTHOR_EMAIL", "greentic-labs@example.com")
        .env("GIT_COMMITTER_NAME", "Greentic Labs")
        .env("GIT_COMMITTER_EMAIL", "greentic-labs@example.com")
        .env_remove("USER")
        .env_remove("USERNAME");
    cmd.assert().success();

    let manifest_path = component_dir.join("component.manifest.json");
    let manifest = fs::read_to_string(&manifest_path).expect("read scaffold manifest after build");
    let manifest_json: JsonValue =
        serde_json::from_str(&manifest).expect("manifest parses as JSON after build");
    let manifest_world = manifest_json["world"]
        .as_str()
        .expect("manifest world should be a string");
    let canonical_world = canonical_component_world();
    assert_eq!(
        canonical_world, manifest_world,
        "scaffold uses the canonical component world"
    );

    let mut doctor = Command::new(assert_cmd::cargo::cargo_bin!("greentic-component"));
    doctor.current_dir(&component_dir).arg("doctor").arg(".");
    doctor.assert().success();
}

#[test]
fn doctor_accepts_built_scaffold_artifact() {
    let temp = TempDir::new().expect("temp dir");
    let component_dir = temp.path().join("artifact-component");
    let mut new_cmd = Command::new(assert_cmd::cargo::cargo_bin!("greentic-component"));
    new_cmd
        .arg("new")
        .arg("--name")
        .arg("artifact-component")
        .arg("--org")
        .arg("ai.greentic")
        .arg("--path")
        .arg(&component_dir)
        .arg("--no-check")
        .env("HOME", temp.path())
        .env("GREENTIC_TEMPLATE_YEAR", "2030")
        .env("GREENTIC_TEMPLATE_ROOT", temp.path().join("templates"))
        .env("GIT_AUTHOR_NAME", "Greentic Labs")
        .env("GIT_AUTHOR_EMAIL", "greentic-labs@example.com")
        .env("GIT_COMMITTER_NAME", "Greentic Labs")
        .env("GIT_COMMITTER_EMAIL", "greentic-labs@example.com")
        .env("CARGO_NET_OFFLINE", "true")
        .env_remove("USER")
        .env_remove("USERNAME");
    new_cmd.assert().success();

    let mut build_cmd = Command::new(assert_cmd::cargo::cargo_bin!("greentic-component"));
    build_cmd
        .current_dir(&component_dir)
        .env("CARGO_NET_OFFLINE", "true")
        .arg("build");
    build_cmd.assert().success();

    let manifest_path = component_dir.join("component.manifest.json");
    let manifest = fs::read_to_string(&manifest_path).expect("read built manifest");
    let manifest_json: JsonValue =
        serde_json::from_str(&manifest).expect("manifest parses as JSON after build");
    assert_eq!(
        manifest_json["world"]
            .as_str()
            .expect("manifest world should be a string"),
        canonical_component_world()
    );

    let wasm_path = component_dir.join(
        manifest_json["artifacts"]["component_wasm"]
            .as_str()
            .expect("artifact path"),
    );
    let wasm_uri = format!("file://{}", wasm_path.display());
    let manifest_uri = format!("file://{}", manifest_path.display());

    let mut doctor = Command::new(assert_cmd::cargo::cargo_bin!("component-doctor"));
    doctor
        .current_dir(&component_dir)
        .arg(wasm_uri)
        .arg("--manifest")
        .arg(manifest_uri)
        .env("CARGO_NET_OFFLINE", "true");
    doctor.assert().success();
}
