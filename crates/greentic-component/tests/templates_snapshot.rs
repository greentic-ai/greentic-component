#![cfg(feature = "cli")]

use assert_cmd::prelude::*;
use assert_fs::TempDir;
use insta::assert_snapshot;
use std::process::Command;

#[test]
fn templates_only_builtin_json() {
    let temp_home = TempDir::new().expect("temp dir");
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("greentic-component"));
    let assert = cmd
        .arg("templates")
        .arg("--json")
        .env("HOME", temp_home.path())
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");
    assert_snapshot!("templates_only_builtin_json", stdout.trim_end());
    temp_home.close().unwrap();
}
