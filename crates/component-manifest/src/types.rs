use std::collections::HashSet;

use greentic_types::SecretRequirement;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct CapabilityRef(pub String);

impl CapabilityRef {
    pub fn validate(&self, pattern: &Regex) -> Result<(), ManifestError> {
        if self.0.trim().is_empty() {
            return Err(ManifestError::EmptyField("capabilities"));
        }
        if !pattern.is_match(&self.0) {
            return Err(ManifestError::InvalidCapability(self.0.clone()));
        }
        Ok(())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentExport {
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
}

impl ComponentExport {
    pub fn validate(&self, pattern: &Regex) -> Result<(), ManifestError> {
        if self.operation.trim().is_empty() {
            return Err(ManifestError::EmptyField("exports.operation"));
        }
        if !pattern.is_match(&self.operation) {
            return Err(ManifestError::InvalidOperation(self.operation.clone()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitCompat {
    pub package: String,
    pub min: String,
    pub max: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<CapabilityRef>,
    #[serde(default)]
    pub exports: Vec<ComponentExport>,
    pub config_schema: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_requirements: Vec<SecretRequirement>,
    pub wit_compat: WitCompat,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
}

impl ComponentManifest {
    pub fn from_value(value: Value) -> Result<Self, ManifestError> {
        Ok(serde_json::from_value(value)?)
    }
}

#[derive(Debug, Clone)]
pub struct CompiledExportSchema {
    pub operation: String,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct ComponentInfo {
    pub name: Option<String>,
    pub description: Option<String>,
    pub capabilities: Vec<CapabilityRef>,
    pub exports: Vec<CompiledExportSchema>,
    pub config_schema: Value,
    pub secret_requirements: Vec<SecretRequirement>,
    pub wit_compat: WitCompat,
    pub metadata: Map<String, Value>,
    pub raw: Value,
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("manifest json parse failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("config schema must be a JSON object")]
    ConfigSchemaNotObject,
    #[error("invalid config schema: {0}")]
    InvalidConfigSchema(String),
    #[error("invalid export schema for `{operation}`: {reason}")]
    InvalidExportSchema { operation: String, reason: String },
    #[error("capabilities must not be empty")]
    MissingCapabilities,
    #[error("exports must not be empty")]
    MissingExports,
    #[error("duplicate capability `{0}` detected")]
    DuplicateCapability(String),
    #[error("duplicate secret `{0}` detected")]
    DuplicateSecret(String),
    #[error("duplicate operation `{0}` detected")]
    DuplicateOperation(String),
    #[error("secret name `{0}` is invalid")]
    InvalidSecret(String),
    #[error("secret requirement `{key}` is invalid: {reason}")]
    InvalidSecretRequirement { key: String, reason: String },
    #[error("capability `{0}` is invalid")]
    InvalidCapability(String),
    #[error("operation `{0}` is invalid")]
    InvalidOperation(String),
    #[error("wit package must be `greentic:component`, found `{found}`")]
    InvalidWitPackage { found: String },
    #[error("invalid version requirement for `{field}`: {source}")]
    InvalidVersionReq {
        field: &'static str,
        #[source]
        source: semver::Error,
    },
    #[error("field `{0}` is required and cannot be empty")]
    EmptyField(&'static str),
}

pub(crate) fn ensure_unique<T, F>(
    values: impl IntoIterator<Item = T>,
    mut duplicate_err: F,
) -> Result<(), ManifestError>
where
    T: Eq + std::hash::Hash + Clone,
    F: FnMut(T) -> ManifestError,
{
    let mut seen = HashSet::new();
    for value in values {
        if !seen.insert(value.clone()) {
            return Err(duplicate_err(value));
        }
    }
    Ok(())
}
