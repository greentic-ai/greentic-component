use component_manifest::ManifestError;
use component_store::StoreError;
use jsonschema::ValidationError;
use thiserror::Error;
use wasmtime::Error as WasmtimeError;

#[derive(Debug, Error)]
pub enum CompError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Wasmtime(#[from] WasmtimeError),
    #[error("schema validation failed: {0}")]
    SchemaValidation(String),
    #[error("binding not found for tenant {0}")]
    BindingNotFound(String),
    #[error("secret `{0}` is not declared by the component")]
    SecretNotDeclared(String),
    #[error("secret `{key}` resolution failed: {source}")]
    SecretResolution {
        key: String,
        #[source]
        source: Box<CompError>,
    },
    #[error("operation `{0}` is not exported by the component")]
    OperationNotFound(String),
    #[error("host feature `{0}` is denied by policy")]
    HostFeatureDenied(&'static str),
    #[error("invalid manifest: {0}")]
    InvalidManifest(&'static str),
    #[error("runtime error: {0}")]
    Runtime(String),
}

impl<'a> From<ValidationError<'a>> for CompError {
    fn from(value: ValidationError<'a>) -> Self {
        CompError::SchemaValidation(value.to_string())
    }
}

impl CompError {
    pub fn secret_resolution(key: impl Into<String>, source: CompError) -> Self {
        CompError::SecretResolution {
            key: key.into(),
            source: Box::new(source),
        }
    }
}
