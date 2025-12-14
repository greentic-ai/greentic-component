use jsonschema::validator_for;
use regex::Regex;
use semver::VersionReq;
use serde_json::Value;

use crate::types::{
    CompiledExportSchema, ComponentExport, ComponentInfo, ComponentManifest, ManifestError,
    WitCompat,
};
use greentic_types::{SecretKey, SecretRequirement};

pub struct ManifestValidator {
    capability_pattern: Regex,
    operation_pattern: Regex,
}

impl Default for ManifestValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl ManifestValidator {
    pub fn new() -> Self {
        Self {
            capability_pattern: Regex::new(r"^[a-z][a-z0-9_.:-]*$").expect("valid regex"),
            operation_pattern: Regex::new(r"^[a-z][a-z0-9_.:-]*$").expect("valid regex"),
        }
    }

    pub fn validate_value(&self, manifest_json: Value) -> Result<ComponentInfo, ManifestError> {
        prevalidate_secret_keys(&manifest_json)?;
        let manifest = ComponentManifest::from_value(manifest_json.clone())?;
        self.validate_manifest(manifest, manifest_json)
    }

    pub fn validate_manifest(
        &self,
        manifest: ComponentManifest,
        raw: Value,
    ) -> Result<ComponentInfo, ManifestError> {
        if manifest.capabilities.is_empty() {
            return Err(ManifestError::MissingCapabilities);
        }
        for capability in &manifest.capabilities {
            capability.validate(&self.capability_pattern)?;
        }
        crate::types::ensure_unique(manifest.capabilities.iter().cloned(), |capability| {
            ManifestError::DuplicateCapability(capability.0)
        })?;

        if manifest.exports.is_empty() {
            return Err(ManifestError::MissingExports);
        }
        crate::types::ensure_unique(
            manifest
                .exports
                .iter()
                .map(|export| export.operation.clone()),
            ManifestError::DuplicateOperation,
        )?;
        for export in &manifest.exports {
            export.validate(&self.operation_pattern)?;
        }

        validate_wit_compat(&manifest.wit_compat)?;

        if !manifest.secret_requirements.is_empty() {
            validate_secret_requirements(&manifest.secret_requirements)?;
        }

        let config_schema = validate_config_schema(&manifest.config_schema)?;
        let compiled_exports = manifest
            .exports
            .iter()
            .map(compile_export_schema)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ComponentInfo {
            name: manifest.name,
            description: manifest.description,
            capabilities: manifest.capabilities,
            exports: compiled_exports,
            config_schema,
            secret_requirements: manifest.secret_requirements,
            wit_compat: manifest.wit_compat,
            metadata: manifest.metadata,
            raw,
        })
    }
}

pub fn validate_config_schema(schema: &Value) -> Result<Value, ManifestError> {
    if !schema.is_object() {
        return Err(ManifestError::ConfigSchemaNotObject);
    }
    validator_for(schema).map_err(|err| ManifestError::InvalidConfigSchema(err.to_string()))?;
    Ok(schema.clone())
}

fn prevalidate_secret_keys(manifest: &Value) -> Result<(), ManifestError> {
    let Some(requirements) = manifest
        .get("secret_requirements")
        .and_then(Value::as_array)
    else {
        return Ok(());
    };

    for entry in requirements {
        if let Some(key) = entry.get("key").and_then(Value::as_str)
            && SecretKey::new(key).is_err()
        {
            return Err(ManifestError::InvalidSecret(key.to_string()));
        }
    }

    Ok(())
}

fn compile_export_schema(export: &ComponentExport) -> Result<CompiledExportSchema, ManifestError> {
    let input_schema = export
        .input_schema
        .as_ref()
        .map(|schema| parse_schema(schema, export, "input_schema"))
        .transpose()?;
    let output_schema = export
        .output_schema
        .as_ref()
        .map(|schema| parse_schema(schema, export, "output_schema"))
        .transpose()?;

    Ok(CompiledExportSchema {
        operation: export.operation.clone(),
        description: export.description.clone(),
        input_schema,
        output_schema,
    })
}

fn parse_schema(
    schema: &Value,
    export: &ComponentExport,
    field: &str,
) -> Result<Value, ManifestError> {
    if !schema.is_object() {
        return Err(ManifestError::InvalidExportSchema {
            operation: export.operation.clone(),
            reason: format!("{field} must be an object"),
        });
    }
    validator_for(schema).map_err(|err| ManifestError::InvalidExportSchema {
        operation: export.operation.clone(),
        reason: err.to_string(),
    })?;
    Ok(schema.clone())
}

fn validate_wit_compat(wit: &WitCompat) -> Result<(), ManifestError> {
    if wit.package != "greentic:component" {
        return Err(ManifestError::InvalidWitPackage {
            found: wit.package.clone(),
        });
    }
    VersionReq::parse(&wit.min).map_err(|source| ManifestError::InvalidVersionReq {
        field: "wit_compat.min",
        source,
    })?;
    if let Some(max) = &wit.max {
        VersionReq::parse(max).map_err(|source| ManifestError::InvalidVersionReq {
            field: "wit_compat.max",
            source,
        })?;
    }
    Ok(())
}

fn validate_secret_requirements(requirements: &[SecretRequirement]) -> Result<(), ManifestError> {
    crate::types::ensure_unique(
        requirements.iter().map(|req| req.key.as_str().to_string()),
        ManifestError::DuplicateSecret,
    )?;

    for req in requirements {
        SecretKey::new(req.key.as_str())
            .map_err(|_| ManifestError::InvalidSecret(req.key.as_str().to_string()))?;

        let scope = req
            .scope
            .as_ref()
            .ok_or_else(|| ManifestError::InvalidSecretRequirement {
                key: req.key.as_str().to_string(),
                reason: "scope must include env and tenant".into(),
            })?;
        if scope.env.trim().is_empty() {
            return Err(ManifestError::InvalidSecretRequirement {
                key: req.key.as_str().to_string(),
                reason: "scope.env must not be empty".into(),
            });
        }
        if scope.tenant.trim().is_empty() {
            return Err(ManifestError::InvalidSecretRequirement {
                key: req.key.as_str().to_string(),
                reason: "scope.tenant must not be empty".into(),
            });
        }
        if let Some(team) = &scope.team
            && team.trim().is_empty()
        {
            return Err(ManifestError::InvalidSecretRequirement {
                key: req.key.as_str().to_string(),
                reason: "scope.team must not be empty when provided".into(),
            });
        }
        if req.format.is_none() {
            return Err(ManifestError::InvalidSecretRequirement {
                key: req.key.as_str().to_string(),
                reason: "format must be specified".into(),
            });
        }
        if let Some(schema) = &req.schema
            && !schema.is_object()
        {
            return Err(ManifestError::InvalidSecretRequirement {
                key: req.key.as_str().to_string(),
                reason: "schema must be an object when provided".into(),
            });
        }
    }
    Ok(())
}
