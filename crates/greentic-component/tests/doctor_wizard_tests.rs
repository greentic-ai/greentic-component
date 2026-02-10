#![cfg(feature = "cli")]

use greentic_component::cmd::doctor::{DoctorArgs, run as doctor_run};
use greentic_component::cmd::wizard::{
    WizardCommand, WizardMode, WizardNewArgs, run as wizard_run,
};
use std::fs;

#[test]
fn doctor_accepts_wizard_scaffold() {
    let temp = tempfile::TempDir::new().unwrap();
    let args = WizardNewArgs {
        name: "demo-component".into(),
        abi_version: "0.6.0".into(),
        mode: WizardMode::Default,
        answers: None,
        out: Some(temp.path().to_path_buf()),
    };
    wizard_run(WizardCommand::New(args)).unwrap();

    let root = temp.path().join("demo-component");
    let doctor_args = DoctorArgs {
        target: root.to_string_lossy().to_string(),
        manifest: None,
        permissive: false,
    };
    doctor_run(doctor_args).expect("doctor should accept wizard scaffold");
}

#[test]
fn doctor_flags_missing_abi_metadata() {
    let temp = tempfile::TempDir::new().unwrap();
    let args = WizardNewArgs {
        name: "demo-component".into(),
        abi_version: "0.6.0".into(),
        mode: WizardMode::Default,
        answers: None,
        out: Some(temp.path().to_path_buf()),
    };
    wizard_run(WizardCommand::New(args)).unwrap();

    let root = temp.path().join("demo-component");
    let cargo_path = root.join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path).unwrap();
    let stripped = cargo
        .lines()
        .filter(|line| !line.starts_with("[package.metadata.greentic]"))
        .filter(|line| !line.starts_with("abi_version"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&cargo_path, stripped).unwrap();

    let doctor_args = DoctorArgs {
        target: root.to_string_lossy().to_string(),
        manifest: None,
        permissive: false,
    };
    assert!(doctor_run(doctor_args).is_err());
}
