use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use serde_json::Value;

const ARTIFACT_ROOT: &str = "target/contract-artifacts";

pub struct WorldContract {
    pub id: &'static str,
    pub fixture_dir: PathBuf,
    pub operation: &'static str,
}

pub fn registry() -> Vec<WorldContract> {
    vec![WorldContract {
        id: "greentic:component/component@0.5.0",
        fixture_dir: PathBuf::from("tests/contract/fixtures/component_v0_5_0"),
        operation: "handle_message",
    }]
}

pub fn run_contract_suite(world: &WorldContract) {
    let valid_inputs = load_inputs(&world.fixture_dir.join("valid_inputs"));
    let invalid_inputs = load_inputs(&world.fixture_dir.join("invalid_inputs"));
    for (name, input) in valid_inputs.iter() {
        run_case(world, name, input, "ok");
        for (idx, mutated) in mutate_inputs(input).into_iter().enumerate() {
            let case_name = format!("{name}-mutated-{idx}");
            run_case(world, &case_name, &mutated, "error");
        }
    }
    for (name, input) in invalid_inputs.iter() {
        run_case(world, name, input, "error");
    }
}

fn run_case(world: &WorldContract, name: &str, input: &Value, expected_status: &str) {
    let output = run_harness_once(world, input);
    let status = output
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let diagnostics = output
        .get("diagnostics")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    if status != expected_status {
        write_artifacts(world, name, input, &output);
        panic!(
            "expected status {expected_status} for {}, got {status}",
            world.id
        );
    }
    if expected_status == "error" && diagnostics.is_empty() {
        write_artifacts(world, name, input, &output);
        panic!("expected diagnostics for error case {} {}", world.id, name);
    }
    let diag_size = serde_json::to_string(&diagnostics)
        .map(|s| s.len())
        .unwrap_or(0);
    if diag_size > 64 * 1024 {
        write_artifacts(world, name, input, &output);
        panic!(
            "diagnostics too large ({diag_size} bytes) for {} {}",
            world.id, name
        );
    }
}

pub fn run_harness_once(world: &WorldContract, input: &Value) -> Value {
    let wasm_path = world.fixture_dir.join("component.wasm");
    let manifest_path = world.fixture_dir.join("component.manifest.json");
    let temp = tempfile::TempDir::new().expect("temp dir");
    let input_path = temp.path().join("input.json");
    fs::write(
        &input_path,
        serde_json::to_string(input).expect("input json"),
    )
    .expect("write input");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("greentic-component"));
    cmd.arg("test")
        .arg("--wasm")
        .arg(&wasm_path)
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("--op")
        .arg(world.operation)
        .arg("--input")
        .arg(&input_path);

    let output = cmd.output().expect("run greentic-component test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).unwrap_or_else(|_| {
        serde_json::json!({
            "status": "error",
            "diagnostics": [{
                "severity": "error",
                "code": "contract.parse",
                "message": "failed to parse harness output"
            }],
            "raw": stdout,
        })
    })
}

fn load_inputs(dir: &Path) -> Vec<(String, Value)> {
    let mut cases = Vec::new();
    if !dir.exists() {
        return cases;
    }
    for entry in fs::read_dir(dir).expect("read input dir") {
        let entry = entry.expect("input entry");
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let path = entry.path();
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("input")
            .to_string();
        let contents = fs::read_to_string(&path).expect("input file");
        let value: Value = serde_json::from_str(&contents).expect("input json");
        cases.push((name, value));
    }
    cases
}

fn mutate_inputs(input: &Value) -> Vec<Value> {
    let mut mutations = Vec::new();
    mutations.push(Value::Null);
    mutations.push(Value::Array(Vec::new()));
    if let Value::Object(map) = input {
        let mut removed = map.clone();
        if let Some(first_key) = removed.keys().next().cloned() {
            removed.remove(&first_key);
            mutations.push(Value::Object(removed));
        }
        let mut wrong_type = map.clone();
        wrong_type.insert("unexpected".to_string(), Value::Bool(true));
        mutations.push(Value::Object(wrong_type));
        let mut extra = map.clone();
        extra.insert("extra_field".to_string(), Value::String("noise".into()));
        mutations.push(Value::Object(extra));
    }
    mutations
}

fn write_artifacts(world: &WorldContract, name: &str, input: &Value, output: &Value) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| dur.as_secs())
        .unwrap_or(0);
    let sanitized_world = world.id.replace([':', '/', '@'], "_");
    let dir = PathBuf::from(ARTIFACT_ROOT)
        .join(sanitized_world)
        .join(format!("{timestamp}-{name}"));
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = fs::write(
        dir.join("input.json"),
        serde_json::to_string_pretty(input).unwrap(),
    );
    let _ = fs::write(dir.join("config.json"), "{}");
    let _ = fs::write(dir.join("secrets.json"), "{}");
    let _ = fs::write(
        dir.join("output.json"),
        serde_json::to_string_pretty(output).unwrap(),
    );
}
