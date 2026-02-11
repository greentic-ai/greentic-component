#![cfg(feature = "cli")]

use greentic_component::cmd::doctor::{DoctorArgs, DoctorFormat, run as doctor_run};
use greentic_component::cmd::wizard::{
    WizardCommand, WizardMode, WizardNewArgs, run as wizard_run,
};

#[test]
fn doctor_rejects_unbuilt_wizard_scaffold() {
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
        format: DoctorFormat::Human,
    };
    let err = doctor_run(doctor_args).expect_err("doctor should require a wasm artifact");
    assert!(err.to_string().contains("unable to resolve wasm"));
}
