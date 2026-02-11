#![cfg(feature = "cli")]

use greentic_component::cmd::wizard::{WizardCommand, WizardMode, WizardNewArgs, run};
use std::fs;

#[test]
fn wizard_new_creates_template_files() {
    let temp = tempfile::TempDir::new().unwrap();
    let args = WizardNewArgs {
        name: "demo-component".into(),
        abi_version: "0.6.0".into(),
        mode: WizardMode::Default,
        answers: None,
        out: Some(temp.path().to_path_buf()),
    };

    run(WizardCommand::New(args)).expect("wizard new should succeed");

    let root = temp.path().join("demo-component");
    assert!(root.join("Cargo.toml").exists());
    assert!(root.join("src/lib.rs").exists());
    assert!(root.join("src/descriptor.rs").exists());
    assert!(root.join("src/schema.rs").exists());
    assert!(root.join("src/runtime.rs").exists());
    assert!(root.join("Makefile").exists());
    assert!(root.join("src/qa.rs").exists());
    assert!(root.join("src/i18n.rs").exists());
    assert!(root.join("wit/package.wit").exists());
    assert!(root.join("assets/i18n/en.json").exists());
    assert!(!root.join("examples/default.answers.json").exists());

    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(cargo_toml.contains("name = \"demo-component\""));
    assert!(cargo_toml.contains("[package.metadata.greentic]"));
    assert!(cargo_toml.contains("abi_version = \"0.6.0\""));

    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    assert!(makefile.contains("WASM_OUT := $(DIST_DIR)/$(NAME)__$(ABI_VERSION_UNDERSCORE).wasm"));

    let wit = fs::read_to_string(root.join("wit/package.wit")).unwrap();
    assert!(wit.contains("world component-v0-v6-v0"));
}

#[test]
fn wizard_new_writes_answers_files_when_provided() {
    let temp = tempfile::TempDir::new().unwrap();
    let answers_path = temp.path().join("answers.json");
    fs::write(&answers_path, r#"{"enabled": true}"#).unwrap();
    let args = WizardNewArgs {
        name: "answers-component".into(),
        abi_version: "0.6.0".into(),
        mode: WizardMode::Default,
        answers: Some(answers_path),
        out: Some(temp.path().to_path_buf()),
    };

    run(WizardCommand::New(args)).expect("wizard new should succeed");

    let root = temp.path().join("answers-component");
    let json_path = root.join("examples/default.answers.json");
    let cbor_path = root.join("examples/default.answers.cbor");
    assert!(json_path.exists());
    assert!(cbor_path.exists());
    let json = fs::read_to_string(json_path).unwrap();
    assert!(json.contains("\"enabled\""));
    assert!(!root.join("examples/setup.answers.json").exists());
}
