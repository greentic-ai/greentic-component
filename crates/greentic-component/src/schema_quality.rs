use greentic_types::component::ComponentOperation;
use serde_json::{Map, Value};

use crate::error::ComponentError;
use crate::manifest::ComponentManifest;

/// Mode used when validating operation schemas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaQualityMode {
    Strict,
    Permissive,
}

impl Default for SchemaQualityMode {
    fn default() -> Self {
        Self::Strict
    }
}

/// Details for a schema-quality warning emitted in permissive mode.
#[derive(Debug, Clone)]
pub struct SchemaQualityWarning {
    pub component_id: String,
    pub operation: String,
    pub direction: &'static str,
    pub message: String,
}

/// Ensure every operation schema is richer than an empty stub.
/// Returns any warnings that should be emitted when permissive mode is selected.
pub fn validate_operation_schemas(
    manifest: &ComponentManifest,
    mode: SchemaQualityMode,
) -> Result<Vec<SchemaQualityWarning>, ComponentError> {
    let mut warnings = Vec::new();
    let component_id = manifest.id.as_str().to_string();
    for operation in &manifest.operations {
        check_operation_schema(
            &component_id,
            operation,
            SchemaDirection::Input,
            mode,
            &mut warnings,
        )?;
        check_operation_schema(
            &component_id,
            operation,
            SchemaDirection::Output,
            mode,
            &mut warnings,
        )?;
    }
    Ok(warnings)
}

fn check_operation_schema(
    component_id: &str,
    operation: &ComponentOperation,
    direction: SchemaDirection,
    mode: SchemaQualityMode,
    warnings: &mut Vec<SchemaQualityWarning>,
) -> Result<(), ComponentError> {
    let schema = match direction {
        SchemaDirection::Input => &operation.input_schema,
        SchemaDirection::Output => &operation.output_schema,
    };

    if !is_effectively_empty_schema(schema) {
        return Ok(());
    }

    let direction_text = direction.as_str();
    let message = format!(
        "component {component_id}, operation `{}`, {direction_text} schema is empty. \
        Populate `operations[].{direction_text}_schema` with real JSON Schema (or reference `schemas/*.json`) and run \
        `greentic-component flow update/build` afterwards.",
        operation.name
    );

    if mode == SchemaQualityMode::Strict {
        return Err(ComponentError::SchemaQualityEmpty {
            component: component_id.to_string(),
            operation: operation.name.clone(),
            direction: direction_text,
            suggestion: message.clone(),
        });
    }

    warnings.push(SchemaQualityWarning {
        component_id: component_id.to_string(),
        operation: operation.name.clone(),
        direction: direction_text,
        message,
    });

    Ok(())
}

/// Indicates whether a schema provides no meaningful structure.
pub fn is_effectively_empty_schema(schema: &Value) -> bool {
    match schema {
        Value::Null => true,
        Value::Bool(flag) => *flag,
        Value::Object(map) => {
            if map.is_empty() {
                return true;
            }
            if let Some(type_value) = map.get("type")
                && type_allows_object(type_value)
                && object_schema_is_unconstrained(map)
            {
                return true;
            }
            false
        }
        _ => false,
    }
}

fn type_allows_object(type_value: &Value) -> bool {
    match type_value {
        Value::String(str_val) => str_val == "object",
        Value::Array(items) => items.iter().any(|item| match item {
            Value::String(value) => value == "object",
            _ => false,
        }),
        _ => false,
    }
}

fn object_schema_is_unconstrained(map: &Map<String, Value>) -> bool {
    if has_constraints(map) {
        return false;
    }

    !additional_properties_disallows_all(map)
}

fn has_constraints(map: &Map<String, Value>) -> bool {
    static CONSTRAINT_KEYS: &[&str] = &[
        "properties",
        "required",
        "oneOf",
        "anyOf",
        "allOf",
        "not",
        "if",
        "enum",
        "const",
        "$ref",
        "pattern",
        "patternProperties",
        "items",
        "dependentSchemas",
        "dependentRequired",
        "minProperties",
        "maxProperties",
        "minItems",
        "maxItems",
    ];

    for &key in CONSTRAINT_KEYS {
        if let Some(value) = map.get(key) {
            match key {
                "properties" => {
                    if let Value::Object(obj) = value {
                        if !obj.is_empty() {
                            return true;
                        }
                        continue;
                    }
                }
                "required" => {
                    if let Value::Array(arr) = value {
                        if !arr.is_empty() {
                            return true;
                        }
                        continue;
                    }
                }
                _ => {
                    return true;
                }
            }
        }
    }

    false
}

fn additional_properties_disallows_all(map: &Map<String, Value>) -> bool {
    matches!(
        map.get("additionalProperties"),
        Some(Value::Bool(false)) | Some(Value::Object(_))
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchemaDirection {
    Input,
    Output,
}

impl SchemaDirection {
    fn as_str(&self) -> &'static str {
        match self {
            SchemaDirection::Input => "input",
            SchemaDirection::Output => "output",
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::is_effectively_empty_schema;

    #[test]
    fn empty_object_schema_is_empty() {
        assert!(is_effectively_empty_schema(&json!({})));
    }

    #[test]
    fn unconstrained_object_is_empty() {
        assert!(is_effectively_empty_schema(&json!({"type": "object"})));
    }

    #[test]
    fn constrained_object_has_properties() {
        assert!(!is_effectively_empty_schema(&json!({
            "type": "object",
            "properties": {
                "foo": { "type": "string" }
            }
        })));
    }

    #[test]
    fn constrained_object_has_required() {
        assert!(!is_effectively_empty_schema(&json!({
            "type": "object",
            "required": ["foo"]
        })));
    }

    #[test]
    fn one_of_is_not_empty() {
        assert!(!is_effectively_empty_schema(&json!({
            "oneOf": [
                { "type": "string" },
                { "type": "number" }
            ]
        })));
    }

    #[test]
    fn additional_properties_false_is_not_empty() {
        assert!(!is_effectively_empty_schema(&json!({
            "type": "object",
            "additionalProperties": false
        })));
    }
}
