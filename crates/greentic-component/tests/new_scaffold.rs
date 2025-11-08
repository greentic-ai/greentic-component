#![cfg(feature = "cli")]

use assert_cmd::prelude::*;
use assert_fs::TempDir;
use insta::assert_snapshot;
use std::fs;
use std::process::Command;

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
        .env("GREENTIC_TEMPLATE_ROOT", temp.path().join("templates"));
    cmd.assert().success();

    let cargo = fs::read_to_string(component_dir.join("Cargo.toml")).expect("Cargo.toml");
    let manifest =
        fs::read_to_string(component_dir.join("component.manifest.json")).expect("manifest");
    let wit = fs::read_to_string(component_dir.join("wit/world.wit")).expect("wit");

    assert_snapshot!("scaffold_cargo_toml", cargo.trim());
    assert_snapshot!("scaffold_manifest", manifest.trim());
    assert_snapshot!("scaffold_wit", wit.trim());
}
