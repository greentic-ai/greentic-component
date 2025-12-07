#![cfg(feature = "cli")]

use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_fs::TempDir;
use predicates::str::contains;
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

#[test]
fn scaffolds_config_flows_from_manifest_schema() {
    let temp = TempDir::new().expect("tempdir");
    let manifest = r#"
{
  "id": "component-demo",
  "mode": "qa",
  "config_schema": {
    "type": "object",
    "properties": {
      "title": {
        "type": "string",
        "description": "Greeting shown to users",
        "default": "Hello world"
      },
      "threshold": {
        "type": "number",
        "default": 0.42
      },
      "kind": {
        "enum": ["Text", "Number"],
        "description": "Answer type",
        "default": "Text"
      },
      "internal": {
        "type": "string",
        "x_flow_hidden": true,
        "default": "skip-me"
      }
    },
    "required": ["title", "threshold"]
  }
}
"#;
    fs::write(temp.path().join("component.manifest.json"), manifest).expect("write manifest");

    let mut cmd = cargo_bin_cmd!("greentic-component");
    cmd.current_dir(temp.path()).arg("flow").arg("scaffold");
    cmd.assert().success();

    let default_flow =
        fs::read_to_string(temp.path().join("flows/default.ygtc")).expect("default flow");
    let custom_flow =
        fs::read_to_string(temp.path().join("flows/custom.ygtc")).expect("custom flow");

    let default_yaml: YamlValue =
        serde_yaml::from_str(&default_flow).expect("default flow parses as yaml");
    assert_eq!(
        default_yaml["id"],
        YamlValue::String("component-demo.default".into())
    );
    assert_eq!(
        default_yaml["kind"],
        YamlValue::String("component-config".into())
    );
    let default_template = default_yaml["nodes"]["emit_config"]["template"]
        .as_str()
        .expect("template string");
    let default_payload: JsonValue =
        serde_json::from_str(default_template).expect("default template to be valid json");
    assert_eq!(default_payload["node"]["qa"]["component"], "component-demo");
    assert_eq!(default_payload["node"]["qa"]["title"], "Hello world");
    assert_eq!(default_payload["node"]["qa"]["threshold"], 0.42);
    assert!(
        default_payload["node"]["qa"].get("kind").is_none(),
        "optional fields should not appear in default flow"
    );

    let custom_yaml: YamlValue =
        serde_yaml::from_str(&custom_flow).expect("custom flow parses as yaml");
    assert_eq!(
        custom_yaml["id"],
        YamlValue::String("component-demo.custom".into())
    );
    let question_fields = custom_yaml["nodes"]["ask_config"]["questions"]["fields"]
        .as_sequence()
        .expect("question fields");
    let field_ids: Vec<String> = question_fields
        .iter()
        .filter_map(|entry| entry["id"].as_str().map(str::to_string))
        .collect();
    assert_eq!(field_ids, vec!["kind", "threshold", "title"]);
    let kind_field = question_fields
        .iter()
        .find(|entry| entry["id"] == YamlValue::String("kind".into()))
        .expect("kind field");
    let options = kind_field["options"].as_sequence().expect("enum options");
    assert_eq!(
        options
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        vec!["Text", "Number"]
    );
    let custom_template = custom_yaml["nodes"]["emit_config"]["template"]
        .as_str()
        .expect("template string");
    assert!(
        custom_template.contains(r#""component": "component-demo""#),
        "component id should be embedded"
    );
    assert!(
        custom_template.contains(r#""title": "{{state.title}}""#),
        "string fields should be quoted state values"
    );
    assert!(
        custom_template.contains(r#""threshold": {{state.threshold}}"#),
        "number fields should be raw state values"
    );
    assert!(
        !custom_template.contains("internal"),
        "hidden fields should be skipped"
    );
}

#[test]
fn requires_force_for_existing_flows_in_non_interactive_mode() {
    let temp = TempDir::new().expect("tempdir");
    let manifest = r#"{"id":"component-demo","config_schema":{"type":"object","properties":{},"required":[]}}"#;
    fs::write(temp.path().join("component.manifest.json"), manifest).expect("write manifest");
    let flows = temp.path().join("flows");
    fs::create_dir_all(&flows).expect("create flows dir");
    fs::write(flows.join("default.ygtc"), "existing").expect("default");

    let mut cmd = cargo_bin_cmd!("greentic-component");
    cmd.current_dir(temp.path()).arg("flow").arg("scaffold");
    cmd.assert()
        .failure()
        .stderr(contains("already exists; rerun with --force"));
}
