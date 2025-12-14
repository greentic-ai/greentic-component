use component_manifest::{ManifestError, ManifestValidator};
use serde_json::json;

fn good_manifest() -> serde_json::Value {
    json!({
        "name": "example",
        "description": "Example component",
        "capabilities": ["http.fetch", "telemetry.emit"],
        "exports": [{
            "operation": "process",
            "description": "Process an input payload",
            "input_schema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "payload": {"type": "string"}
                },
                "required": ["payload"]
            },
            "output_schema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "status": {"type": "string"}
                },
                "required": ["status"]
            }
        }],
        "config_schema": {
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "enabled": {"type": "boolean"}
            },
            "required": ["enabled"]
        },
        "secret_requirements": [{
            "key": "configs/api_token",
            "required": true,
            "description": "API token",
            "scope": { "env": "dev", "tenant": "acme" },
            "format": "text",
            "schema": { "type": "string" }
        }],
        "wit_compat": {
            "package": "greentic:component",
            "min": "0.4.0",
            "max": "0.4.x"
        },
        "metadata": {
            "category": "demo"
        }
    })
}

#[test]
fn validate_good_manifest() {
    let manifest = good_manifest();
    let validator = ManifestValidator::new();
    let info = validator
        .validate_value(manifest.clone())
        .expect("manifest should be valid");

    assert_eq!(info.name.as_deref(), Some("example"));
    assert_eq!(info.capabilities.len(), 2);
    assert_eq!(info.exports.len(), 1);
    assert_eq!(info.secret_requirements.len(), 1);
    assert_eq!(
        info.secret_requirements[0].key.as_str(),
        "configs/api_token"
    );
    assert_eq!(info.wit_compat.package, "greentic:component");
    assert_eq!(info.raw, manifest);
}

#[test]
fn reject_duplicate_capabilities() {
    let mut manifest = good_manifest();
    manifest["capabilities"] = json!(["http.fetch", "http.fetch"]);
    let err = ManifestValidator::new()
        .validate_value(manifest)
        .expect_err("duplicate capability should be rejected");
    matches!(err, ManifestError::DuplicateCapability(_))
        .then_some(())
        .expect("expected duplicate capability error");
}

#[test]
fn reject_invalid_secret() {
    let mut manifest = good_manifest();
    manifest["secret_requirements"][0]["key"] = json!("/invalid");
    let err = ManifestValidator::new()
        .validate_value(manifest)
        .expect_err("invalid secret name should be rejected");
    matches!(err, ManifestError::InvalidSecret(_))
        .then_some(())
        .expect("expected invalid secret error");
}

#[test]
fn reject_invalid_scope() {
    let mut manifest = good_manifest();
    manifest["secret_requirements"][0]["scope"]["env"] = json!("");
    let err = ManifestValidator::new()
        .validate_value(manifest)
        .expect_err("empty scope env should be rejected");
    matches!(err, ManifestError::InvalidSecretRequirement { .. })
        .then_some(())
        .expect("expected invalid secret requirement error");
}
