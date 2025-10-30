use std::collections::{HashMap, HashSet};

use jsonschema::Validator;
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
        let binding = resolve_binding(
            &inner.info,
            inner.config_schema.as_ref(),
            bindings,
            tenant,
            secret_resolver,
        )?;

        let key = binding_key(tenant);
        let mut guard = inner.bindings.lock().expect("binding mutex poisoned");
        guard.insert(key, binding);
        Ok(())
    }
}

pub(crate) fn binding_key(ctx: &TenantCtx) -> String {
    format!("{}::{}", ctx.env.as_str(), ctx.tenant.as_str())
}

pub(crate) fn resolve_binding(
    info: &component_manifest::ComponentInfo,
    schema: &Validator,
    bindings: &Bindings,
    tenant: &TenantCtx,
    secret_resolver: &mut dyn FnMut(&str, &TenantCtx) -> Result<String, CompError>,
) -> Result<TenantBinding, CompError> {
    validate_config(schema, &bindings.config)?;
    let allowed_secrets: HashSet<String> = info.secrets.iter().cloned().collect();

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

    Ok(TenantBinding {
        config: bindings.config.clone(),
        secrets: secret_values,
    })
}

fn validate_config(schema: &Validator, config: &Value) -> Result<(), CompError> {
    let mut errors = schema.iter_errors(config);
    if let Some(first_error) = errors.next() {
        let message = std::iter::once(first_error.to_string())
            .chain(errors.map(|err| err.to_string()))
            .collect::<Vec<_>>()
            .join(", ");
        Err(CompError::SchemaValidation(message))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use component_manifest::{CapabilityRef, ComponentInfo, WitCompat};
    use greentic_types::{EnvId, TenantCtx, TenantId};
    use jsonschema::validator_for;
    use serde_json::{json, Map};

    fn component_fixture() -> (ComponentInfo, Validator) {
        let manifest_json = json!({
            "capabilities": ["telemetry"],
            "exports": [{"operation": "noop"}],
            "config_schema": {
                "type": "object",
                "properties": {"enabled": {"type": "boolean"}},
                "required": ["enabled"],
                "additionalProperties": false
            },
            "secrets": ["API_TOKEN"],
            "wit_compat": {
                "package": "greentic:component",
                "min": "0.4.0",
                "max": "0.4.x"
            }
        });

        let config_schema_json = manifest_json.get("config_schema").cloned().unwrap();
        let schema = validator_for(&config_schema_json).unwrap();

        let info = ComponentInfo {
            name: Some("fixture".into()),
            description: None,
            capabilities: vec![CapabilityRef("telemetry".into())],
            exports: vec![component_manifest::CompiledExportSchema {
                operation: "noop".into(),
                description: None,
                input_schema: None,
                output_schema: None,
            }],
            config_schema: config_schema_json,
            secrets: vec!["API_TOKEN".into()],
            wit_compat: WitCompat {
                package: "greentic:component".into(),
                min: "0.4.0".into(),
                max: Some("0.4.x".into()),
            },
            metadata: Map::new(),
            raw: manifest_json,
        };

        (info, schema)
    }

    fn tenant_ctx() -> TenantCtx {
        TenantCtx {
            env: EnvId("dev".into()),
            tenant: TenantId("tenant".into()),
            team: None,
            user: None,
            trace_id: None,
            correlation_id: None,
            deadline: None,
            attempt: 0,
            idempotency_key: None,
        }
    }

    #[test]
    fn resolves_valid_binding() {
        let (info, schema) = component_fixture();
        let tenant = tenant_ctx();
        let bindings = Bindings {
            config: json!({"enabled": true}),
            secrets: vec!["API_TOKEN".into()],
        };
        let mut resolver = |key: &str, _ctx: &TenantCtx| -> Result<String, CompError> {
            Ok(format!("value-for-{key}"))
        };

        let binding = resolve_binding(&info, &schema, &bindings, &tenant, &mut resolver).unwrap();
        assert_eq!(
            binding.secrets.get("API_TOKEN").unwrap(),
            "value-for-API_TOKEN"
        );
    }

    #[test]
    fn rejects_unknown_secret() {
        let (info, schema) = component_fixture();
        let tenant = tenant_ctx();
        let bindings = Bindings {
            config: json!({"enabled": true}),
            secrets: vec!["UNKNOWN".into()],
        };
        let mut resolver =
            |_key: &str, _ctx: &TenantCtx| -> Result<String, CompError> { Ok("secret".into()) };

        let err = resolve_binding(&info, &schema, &bindings, &tenant, &mut resolver).unwrap_err();
        assert!(matches!(err, CompError::SecretNotDeclared(_)));
    }

    #[test]
    fn rejects_invalid_config() {
        let (info, schema) = component_fixture();
        let tenant = tenant_ctx();
        let bindings = Bindings {
            config: json!({"enabled": "not-bool"}),
            secrets: vec![],
        };
        let mut resolver =
            |_key: &str, _ctx: &TenantCtx| -> Result<String, CompError> { Ok("secret".into()) };

        let err = resolve_binding(&info, &schema, &bindings, &tenant, &mut resolver).unwrap_err();
        assert!(matches!(err, CompError::SchemaValidation(_)));
    }
}
