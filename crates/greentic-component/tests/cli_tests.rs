#![cfg(all(feature = "cli", feature = "prepare"))]

#[path = "support/mod.rs"]
mod support;

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
