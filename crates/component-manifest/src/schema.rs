use regex::Regex;
use schemars::schema::RootSchema;
use serde_json::Value;
use semver::VersionReq;

use crate::types::{
    CompiledExportSchema, ComponentExport, ComponentInfo, ComponentManifest, ManifestError,
    WitCompat,
};

pub struct ManifestValidator {
    capability_pattern: Regex,
    operation_pattern: Regex,
    secret_pattern: Regex,
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
            secret_pattern: Regex::new(r"^[A-Z0-9_][A-Z0-9_.:-]*$").expect("valid regex"),
        }
    }

    pub fn validate_value(&self, manifest_json: Value) -> Result<ComponentInfo, ManifestError> {
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
        crate::types::ensure_unique(
            manifest.capabilities.iter().cloned(),
            |capability| ManifestError::DuplicateCapability(capability.0),
        )?;

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

        if !manifest.secrets.is_empty() {
            validate_secrets(&manifest.secrets, &self.secret_pattern)?;
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
            secrets: manifest.secrets,
            wit_compat: manifest.wit_compat,
            metadata: manifest.metadata,
            raw,
        })
    }
}

pub fn validate_config_schema(schema: &Value) -> Result<RootSchema, ManifestError> {
    if !schema.is_object() {
        return Err(ManifestError::ConfigSchemaNotObject);
    }
    serde_json::from_value(schema.clone())
        .map_err(|err| ManifestError::InvalidConfigSchema(err.to_string()))
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
) -> Result<RootSchema, ManifestError> {
    if !schema.is_object() {
        return Err(ManifestError::InvalidExportSchema {
            operation: export.operation.clone(),
            reason: format!("{field} must be an object"),
        });
    }
    serde_json::from_value(schema.clone()).map_err(|err| ManifestError::InvalidExportSchema {
        operation: export.operation.clone(),
        reason: err.to_string(),
    })
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

fn validate_secrets(secrets: &[String], pattern: &Regex) -> Result<(), ManifestError> {
    let mut seen = std::collections::HashSet::new();
    for secret in secrets {
        if secret.trim().is_empty() {
            return Err(ManifestError::EmptyField("secrets"));
        }
        if !pattern.is_match(secret) {
            return Err(ManifestError::InvalidSecret(secret.clone()));
        }
        if !seen.insert(secret.clone()) {
            return Err(ManifestError::DuplicateSecret(secret.clone()));
        }
    }
    Ok(())
}
