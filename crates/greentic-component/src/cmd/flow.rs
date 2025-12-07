#![cfg(feature = "cli")]

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};
use component_manifest::validate_config_schema;
use serde::Serialize;
use serde_json::Value as JsonValue;
use serde_yaml::{Mapping, Value as YamlValue};

const DEFAULT_MANIFEST: &str = "component.manifest.json";
const DEFAULT_NODE_ID: &str = "COMPONENT_STEP";
const DEFAULT_KIND: &str = "component-config";

#[derive(Subcommand, Debug, Clone)]
pub enum FlowCommand {
    /// Scaffold config flows (default/custom) from component.manifest.json
    Scaffold(FlowScaffoldArgs),
}

#[derive(Args, Debug, Clone)]
pub struct FlowScaffoldArgs {
    /// Path to component.manifest.json (or directory containing it)
    #[arg(long = "manifest", value_name = "PATH", default_value = DEFAULT_MANIFEST)]
    pub manifest: PathBuf,
    /// Overwrite existing flows without prompting
    #[arg(long = "force")]
    pub force: bool,
}

pub fn run(command: FlowCommand) -> Result<()> {
    match command {
        FlowCommand::Scaffold(args) => scaffold(args),
    }
}

fn scaffold(args: FlowScaffoldArgs) -> Result<()> {
    let manifest_path = resolve_manifest_path(&args.manifest);
    let manifest_dir = manifest_path
        .parent()
        .ok_or_else(|| anyhow!("manifest path has no parent: {}", manifest_path.display()))?;
    let manifest_raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest_json: JsonValue = serde_json::from_str(&manifest_raw)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    let component_id = manifest_json
        .get("id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("component.manifest.json must contain a string `id` field"))?;
    let mode = manifest_json
        .get("mode")
        .or_else(|| manifest_json.get("kind"))
        .and_then(|value| value.as_str())
        .unwrap_or("tool");
    let config_schema = manifest_json
        .get("config_schema")
        .ok_or_else(|| anyhow!("component.manifest.json is missing `config_schema`"))?;
    validate_config_schema(config_schema)
        .map_err(|err| anyhow!("config_schema failed validation: {err}"))?;

    let fields = collect_fields(config_schema)?;

    let flows_dir = manifest_dir.join("flows");
    fs::create_dir_all(&flows_dir).with_context(|| {
        format!(
            "failed to create flows directory at {}",
            flows_dir.display()
        )
    })?;

    let default_flow = render_default_flow(component_id, mode, &fields)?;
    let default_path = flows_dir.join("default.ygtc");
    let default_written = write_flow_file(&default_path, &default_flow, args.force)?;

    let custom_flow = render_custom_flow(component_id, mode, &fields)?;
    let custom_path = flows_dir.join("custom.ygtc");
    let custom_written = write_flow_file(&custom_path, &custom_flow, args.force)?;

    if !default_written && !custom_written {
        println!("No flows written (existing files kept).");
    } else {
        if default_written {
            println!("Wrote {}", default_path.display());
        }
        if custom_written {
            println!("Wrote {}", custom_path.display());
        }
    }

    Ok(())
}

fn resolve_manifest_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join(DEFAULT_MANIFEST)
    } else {
        path.to_path_buf()
    }
}

fn write_flow_file(path: &Path, contents: &str, force: bool) -> Result<bool> {
    if path.exists() && !confirm_overwrite(path, force)? {
        return Ok(false);
    }
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(true)
}

fn confirm_overwrite(path: &Path, force: bool) -> Result<bool> {
    if force {
        return Ok(true);
    }
    if !path.exists() {
        return Ok(true);
    }
    if io::stdin().is_terminal() {
        print!("{} already exists. Overwrite? [y/N]: ", path.display());
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read response")?;
        let normalized = input.trim().to_ascii_lowercase();
        Ok(normalized == "y" || normalized == "yes")
    } else {
        bail!(
            "{} already exists; rerun with --force to overwrite",
            path.display()
        );
    }
}

fn collect_fields(config_schema: &JsonValue) -> Result<Vec<ConfigField>> {
    let properties = config_schema
        .get("properties")
        .and_then(|value| value.as_object())
        .ok_or_else(|| anyhow!("config_schema.properties must be an object"))?;
    let required = config_schema
        .get("required")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<HashSet<String>>()
        })
        .unwrap_or_default();

    let mut fields = properties
        .iter()
        .map(|(name, schema)| ConfigField::from_schema(name, schema, required.contains(name)))
        .collect::<Vec<_>>();
    fields.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(fields)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldType {
    String,
    Number,
    Integer,
    Boolean,
    Unknown,
}

impl FieldType {
    fn from_schema(schema: &JsonValue) -> Self {
        let type_value = schema.get("type");
        match type_value {
            Some(JsonValue::String(value)) => Self::from_type_str(value),
            Some(JsonValue::Array(types)) => types
                .iter()
                .filter_map(|v| v.as_str())
                .find_map(|value| {
                    let field_type = Self::from_type_str(value);
                    (field_type != FieldType::Unknown && value != "null").then_some(field_type)
                })
                .unwrap_or(FieldType::Unknown),
            _ => FieldType::Unknown,
        }
    }

    fn from_type_str(value: &str) -> Self {
        match value {
            "string" => FieldType::String,
            "number" => FieldType::Number,
            "integer" => FieldType::Integer,
            "boolean" => FieldType::Boolean,
            _ => FieldType::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
struct ConfigField {
    name: String,
    description: Option<String>,
    field_type: FieldType,
    enum_options: Vec<String>,
    default_value: Option<JsonValue>,
    required: bool,
    hidden: bool,
}

impl ConfigField {
    fn from_schema(name: &str, schema: &JsonValue, required: bool) -> Self {
        let field_type = FieldType::from_schema(schema);
        let description = schema
            .get("description")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let default_value = schema.get("default").cloned();
        let enum_options = schema
            .get("enum")
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .map(|entry| {
                        entry
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| entry.to_string())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let hidden = schema
            .get("x_flow_hidden")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        Self {
            name: name.to_string(),
            description,
            field_type,
            enum_options,
            default_value,
            required,
            hidden,
        }
    }

    fn prompt(&self) -> String {
        if let Some(desc) = &self.description {
            return desc.clone();
        }
        humanize(&self.name)
    }

    fn question_type(&self) -> &'static str {
        if !self.enum_options.is_empty() {
            "enum"
        } else {
            match self.field_type {
                FieldType::String => "string",
                FieldType::Number | FieldType::Integer => "number",
                FieldType::Boolean => "boolean",
                FieldType::Unknown => "string",
            }
        }
    }

    fn is_string_like(&self) -> bool {
        !self.enum_options.is_empty()
            || matches!(self.field_type, FieldType::String | FieldType::Unknown)
    }
}

fn humanize(raw: &str) -> String {
    let mut result = raw
        .replace(['_', '-'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if !result.ends_with(':') && !result.is_empty() {
        result.push(':');
    }
    result
}

fn render_default_flow(component_id: &str, mode: &str, fields: &[ConfigField]) -> Result<String> {
    let required_with_defaults = fields
        .iter()
        .filter(|field| field.required && field.default_value.is_some())
        .collect::<Vec<_>>();

    let field_values = required_with_defaults
        .iter()
        .map(|field| {
            let literal =
                serde_json::to_string(field.default_value.as_ref().expect("filtered to Some"))
                    .expect("json serialize default");
            EmitField {
                name: field.name.clone(),
                value: EmitFieldValue::Literal(literal),
            }
        })
        .collect::<Vec<_>>();

    let emit_template = render_emit_template(component_id, mode, field_values);
    let mut emit_node = Mapping::new();
    emit_node.insert(
        YamlValue::String("template".into()),
        YamlValue::String(emit_template),
    );

    let mut nodes = BTreeMap::new();
    nodes.insert("emit_config".to_string(), YamlValue::Mapping(emit_node));

    let doc = FlowDocument {
        id: format!("{component_id}.default"),
        kind: DEFAULT_KIND.to_string(),
        description: format!("Auto-generated default config for {component_id}"),
        nodes,
    };

    flow_to_string(&doc)
}

fn render_custom_flow(component_id: &str, mode: &str, fields: &[ConfigField]) -> Result<String> {
    let visible_fields = fields
        .iter()
        .filter(|field| !field.hidden)
        .collect::<Vec<_>>();

    let mut question_fields = Vec::new();
    for field in &visible_fields {
        let mut mapping = Mapping::new();
        mapping.insert(
            YamlValue::String("id".into()),
            YamlValue::String(field.name.clone()),
        );
        mapping.insert(
            YamlValue::String("prompt".into()),
            YamlValue::String(field.prompt()),
        );
        mapping.insert(
            YamlValue::String("type".into()),
            YamlValue::String(field.question_type().to_string()),
        );
        if !field.enum_options.is_empty() {
            let options = field
                .enum_options
                .iter()
                .map(|value| YamlValue::String(value.clone()))
                .collect::<Vec<_>>();
            mapping.insert(
                YamlValue::String("options".into()),
                YamlValue::Sequence(options),
            );
        }
        if let Some(default_value) = &field.default_value {
            mapping.insert(
                YamlValue::String("default".into()),
                serde_yaml::to_value(default_value.clone()).unwrap_or(YamlValue::Null),
            );
        }
        question_fields.push(YamlValue::Mapping(mapping));
    }

    let mut questions_inner = Mapping::new();
    questions_inner.insert(
        YamlValue::String("fields".into()),
        YamlValue::Sequence(question_fields),
    );

    let mut ask_node = Mapping::new();
    ask_node.insert(
        YamlValue::String("questions".into()),
        YamlValue::Mapping(questions_inner),
    );
    ask_node.insert(
        YamlValue::String("routing".into()),
        YamlValue::Sequence(vec![{
            let mut route = Mapping::new();
            route.insert(
                YamlValue::String("to".into()),
                YamlValue::String("emit_config".into()),
            );
            YamlValue::Mapping(route)
        }]),
    );

    let emit_field_values = visible_fields
        .iter()
        .map(|field| EmitField {
            name: field.name.clone(),
            value: if field.is_string_like() {
                EmitFieldValue::StateQuoted(field.name.clone())
            } else {
                EmitFieldValue::StateRaw(field.name.clone())
            },
        })
        .collect::<Vec<_>>();
    let emit_template = render_emit_template(component_id, mode, emit_field_values);
    let mut emit_node = Mapping::new();
    emit_node.insert(
        YamlValue::String("template".into()),
        YamlValue::String(emit_template),
    );

    let mut nodes = BTreeMap::new();
    nodes.insert("ask_config".to_string(), YamlValue::Mapping(ask_node));
    nodes.insert("emit_config".to_string(), YamlValue::Mapping(emit_node));

    let doc = FlowDocument {
        id: format!("{component_id}.custom"),
        kind: DEFAULT_KIND.to_string(),
        description: format!("Auto-generated custom config for {component_id}"),
        nodes,
    };

    flow_to_string(&doc)
}

fn render_emit_template(component_id: &str, mode: &str, fields: Vec<EmitField>) -> String {
    let mut lines = Vec::new();
    lines.push("{".to_string());
    lines.push(format!("  \"node_id\": \"{DEFAULT_NODE_ID}\","));
    lines.push("  \"node\": {".to_string());
    lines.push(format!("    \"{mode}\": {{"));
    lines.push(format!(
        "      \"component\": \"{component_id}\"{}",
        if fields.is_empty() { "" } else { "," }
    ));

    for (idx, field) in fields.iter().enumerate() {
        let suffix = if idx + 1 == fields.len() { "" } else { "," };
        lines.push(format!(
            "      \"{}\": {}{}",
            field.name,
            field.value.render(),
            suffix
        ));
    }

    lines.push("    },".to_string());
    lines.push("    \"routing\": [".to_string());
    lines.push("      { \"to\": \"NEXT_NODE_PLACEHOLDER\" }".to_string());
    lines.push("    ]".to_string());
    lines.push("  }".to_string());
    lines.push("}".to_string());
    lines.join("\n")
}

struct EmitField {
    name: String,
    value: EmitFieldValue,
}

enum EmitFieldValue {
    Literal(String),
    StateQuoted(String),
    StateRaw(String),
}

impl EmitFieldValue {
    fn render(&self) -> String {
        match self {
            EmitFieldValue::Literal(value) => value.clone(),
            EmitFieldValue::StateQuoted(name) => format!("\"{{{{state.{name}}}}}\""),
            EmitFieldValue::StateRaw(name) => format!("{{{{state.{name}}}}}"),
        }
    }
}

#[derive(Serialize)]
struct FlowDocument {
    id: String,
    kind: String,
    description: String,
    nodes: BTreeMap<String, YamlValue>,
}

fn flow_to_string(doc: &FlowDocument) -> Result<String> {
    let mut yaml = serde_yaml::to_string(doc).context("failed to render YAML")?;
    if yaml.starts_with("---\n") {
        yaml = yaml.replacen("---\n", "", 1);
    }
    if !yaml.ends_with('\n') {
        yaml.push('\n');
    }
    Ok(yaml)
}
