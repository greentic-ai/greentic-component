#![cfg(all(feature = "cli", feature = "prepare"))]

#[path = "support/mod.rs"]
mod support;

use greentic_component::scaffold::engine::{ScaffoldEngine, ScaffoldRequest};
use predicates::prelude::*;
use support::TestComponent;

const TEST_WIT: &str = r#"
package greentic:component@0.1.0;
world node {
    export describe: func();
}
"#;

#[test]
fn inspect_outputs_json() {
    let component = TestComponent::new(TEST_WIT, &["describe"]);
    let manifest_path = component.manifest_path.to_str().unwrap();
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("component-inspect");
    cmd.arg(manifest_path)
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"manifest\""));
}

#[test]
fn doctor_reports_success() {
    let component = TestComponent::new(TEST_WIT, &["describe"]);
    let manifest_path = component.manifest_path.to_str().unwrap();
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("component-doctor");
    cmd.arg(manifest_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("manifest schema: ok"));
}

#[test]
fn doctor_detects_scaffold_directory() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("demo-detect");
    let engine = ScaffoldEngine::new();
    let request = ScaffoldRequest {
        name: "demo-detect".into(),
        path: root.clone(),
        template_id: "rust-wasi-p2-min".into(),
        org: "ai.greentic".into(),
        version: "0.1.0".into(),
        license: "MIT".into(),
        wit_world: "component".into(),
        non_interactive: true,
        year_override: Some(2030),
    };
    engine.scaffold(request).unwrap();
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("component-doctor");
    cmd.arg(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Detected Greentic scaffold"));
}
