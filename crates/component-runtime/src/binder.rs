use std::collections::{HashMap, HashSet};

use jsonschema::JSONSchema;
use serde_json::Value;

use crate::error::CompError;
use crate::loader::{ComponentHandle, TenantBinding};
use greentic_types::TenantCtx;

#[derive(Debug, Clone)]
pub struct Bindings {
    pub config: Value,
    pub secrets: Vec<String>,
}

impl Bindings {
    pub fn new(config: Value, secrets: Vec<String>) -> Self {
        Self { config, secrets }
    }
}

#[derive(Debug, Default)]
pub struct Binder;

impl Binder {
    pub fn bind(
        &self,
        handle: &ComponentHandle,
        tenant: &TenantCtx,
        bindings: &Bindings,
        secret_resolver: &mut dyn FnMut(&str, &TenantCtx) -> Result<String, CompError>,
    ) -> Result<(), CompError> {
        let inner = &handle.inner;
        validate_config(inner.config_schema.as_ref(), &bindings.config)?;
        let allowed_secrets: HashSet<String> = inner
            .info
            .secrets
            .iter()
            .cloned()
            .collect();

        let mut resolved = HashSet::new();
        let mut secret_values = HashMap::new();
        for secret in &bindings.secrets {
            if !allowed_secrets.contains(secret) {
                return Err(CompError::SecretNotDeclared(secret.clone()));
            }
            if !resolved.insert(secret.clone()) {
                continue;
            }
            let value = secret_resolver(secret, tenant)
                .map_err(|err| CompError::secret_resolution(secret.clone(), err))?;
            secret_values.insert(secret.clone(), value);
        }

        let binding = TenantBinding {
            config: bindings.config.clone(),
            secrets: secret_values,
        };

        let key = binding_key(tenant);
        let mut guard = inner.bindings.lock().expect("binding mutex poisoned");
        guard.insert(key, binding);
        Ok(())
    }
}

pub(crate) fn binding_key(ctx: &TenantCtx) -> String {
    format!("{}::{}", ctx.env.as_str(), ctx.tenant.as_str())
}

fn validate_config(schema: &JSONSchema, config: &Value) -> Result<(), CompError> {
    schema.validate(config).map_err(CompError::from)
}
