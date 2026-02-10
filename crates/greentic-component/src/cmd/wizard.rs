#![cfg(feature = "cli")]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::Value as JsonValue;

use crate::scaffold::validate::{
    ComponentName, ValidationError, ensure_path_available, normalize_version,
};

#[derive(Subcommand, Debug, Clone)]
pub enum WizardCommand {
    /// Generate a component@0.6.0 template scaffold
    New(WizardNewArgs),
}

#[derive(Args, Debug, Clone)]
pub struct WizardNewArgs {
    /// Component name (kebab-or-snake case)
    #[arg(value_name = "name")]
    pub name: String,
    /// ABI version to target (template is fixed to 0.6.0 for now)
    #[arg(long = "abi-version", default_value = "0.6.0", value_name = "semver")]
    pub abi_version: String,
    /// QA mode to prefill when --answers is provided
    #[arg(long = "mode", value_enum, default_value = "default")]
    pub mode: WizardMode,
    /// Answers JSON to prefill QA setup
    #[arg(long = "answers", value_name = "answers.json")]
    pub answers: Option<PathBuf>,
    /// Output directory (template will be created under <out>/<name>)
    #[arg(long = "out", value_name = "dir")]
    pub out: Option<PathBuf>,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardMode {
    Default,
    Setup,
}

pub fn run(command: WizardCommand) -> Result<()> {
    match command {
        WizardCommand::New(args) => run_new(args),
    }
}

fn run_new(args: WizardNewArgs) -> Result<()> {
    let name = ComponentName::parse(&args.name)?;
    let abi_version = normalize_version(&args.abi_version)?;
    let target = resolve_out_path(&name, args.out.as_deref())?;
    ensure_path_available(&target)?;

    if abi_version != "0.6.0" {
        eprintln!(
            "wizard: warning: only component@0.6.0 template is generated (requested {})",
            abi_version
        );
    }

    let answers_cbor = match args.answers.as_ref() {
        Some(path) => Some(load_answers_cbor(path)?),
        None => None,
    };

    let context = WizardContext {
        name: name.into_string(),
        abi_version,
        prefill_mode: args.mode,
        prefill_answers_cbor: answers_cbor,
    };

    write_template(&target, &context)?;

    println!("wizard: created {}", target.display());
    Ok(())
}

fn resolve_out_path(
    name: &ComponentName,
    out: Option<&Path>,
) -> std::result::Result<PathBuf, ValidationError> {
    if let Some(out) = out {
        let base = if out.is_absolute() {
            out.to_path_buf()
        } else {
            env::current_dir()
                .map_err(ValidationError::WorkingDir)?
                .join(out)
        };
        Ok(base.join(name.as_str()))
    } else {
        crate::scaffold::validate::resolve_target_path(name, None)
    }
}

fn load_answers_cbor(path: &Path) -> Result<Vec<u8>> {
    let handle = fs::File::open(path)
        .with_context(|| format!("wizard: failed to open answers file {}", path.display()))?;
    let json: JsonValue = serde_json::from_reader(handle)
        .with_context(|| format!("wizard: answers file {} is not valid JSON", path.display()))?;
    let mut out = Vec::new();
    encode_self_described_cbor(&json, &mut out)?;
    Ok(out)
}

#[derive(Debug, Clone)]
struct WizardContext {
    name: String,
    abi_version: String,
    prefill_mode: WizardMode,
    prefill_answers_cbor: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct GeneratedFile {
    path: PathBuf,
    contents: Vec<u8>,
}

fn write_template(path: &Path, context: &WizardContext) -> Result<()> {
    let files = build_files(context)?;
    for file in files {
        let target = path.join(&file.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("wizard: failed to create directory {}", parent.display())
            })?;
        }
        fs::write(&target, &file.contents)
            .with_context(|| format!("wizard: failed to write {}", target.display()))?;
    }
    Ok(())
}

fn build_files(context: &WizardContext) -> Result<Vec<GeneratedFile>> {
    let files = vec![
        text_file("Cargo.toml", render_cargo_toml(context)),
        text_file("README.md", render_readme(context)),
        text_file("Makefile", render_makefile()),
        text_file("src/lib.rs", render_lib_rs(context)),
        text_file("src/qa.rs", render_qa_rs(context)),
        text_file("src/schemas.rs", render_schemas_rs()),
        text_file("src/i18n.rs", render_i18n_rs()),
        text_file("wit/package.wit", render_wit_package()),
        text_file(
            "examples/default.answers.json",
            render_example_answers("default"),
        ),
        text_file(
            "examples/setup.answers.json",
            render_example_answers("setup"),
        ),
        text_file(
            "examples/upgrade.answers.json",
            render_example_answers("upgrade"),
        ),
        text_file(
            "examples/remove.answers.json",
            render_example_answers("remove"),
        ),
        text_file("examples/example.schema.json", render_example_schema()),
    ];
    Ok(files)
}

fn text_file(path: &str, contents: String) -> GeneratedFile {
    GeneratedFile {
        path: PathBuf::from(path),
        contents: contents.into_bytes(),
    }
}

fn render_cargo_toml(context: &WizardContext) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
license = "MIT"
rust-version = "1.90"
description = "Greentic component {name}"

[lib]
crate-type = ["cdylib", "rlib"]

[package.metadata.greentic]
abi_version = "{abi_version}"

[dependencies]
greentic-interfaces-guest = "0.4"
greentic-types = "0.4"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
"#,
        name = context.name,
        abi_version = context.abi_version
    )
}

fn render_readme(context: &WizardContext) -> String {
    format!(
        r#"# {name}

Generated by `greentic-component wizard new` for component@0.6.0.

## Next steps
- Update `wit/package.wit` to match your component interface.
- Implement CBOR encoding in `src/lib.rs` and `src/qa.rs`.
- Define schemas in `src/schemas.rs` and `examples/example.schema.json`.
- Register i18n keys in `src/i18n.rs`.

## ABI version
Requested ABI version: {abi_version}

Note: the wizard currently emits a fixed 0.6.0 template.
"#,
        name = context.name,
        abi_version = context.abi_version
    )
}

fn render_makefile() -> String {
    r#"SHELL := /bin/sh

NAME := $(shell awk 'BEGIN{in=0} /^\[package\]/{in=1; next} /^\[/{in=0} in && /^name = / {gsub(/"/ , "", $$3); print $$3; exit}' Cargo.toml)
ABI_VERSION := $(shell awk 'BEGIN{in=0} /^\[package.metadata.greentic\]/{in=1; next} /^\[/{in=0} in && /^abi_version = / {gsub(/"/ , "", $$3); print $$3; exit}' Cargo.toml)
ABI_VERSION_UNDERSCORE := $(subst .,_,$(ABI_VERSION))
DIST_DIR := dist
WASM_OUT := $(DIST_DIR)/$(NAME)__$(ABI_VERSION_UNDERSCORE).wasm
WASM_SRC := target/wasm32-wasip2/release/$(NAME).wasm

.PHONY: build test fmt clippy wasm doctor

build:
	cargo build

test:
	cargo test

fmt:
	cargo fmt

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

wasm:
	cargo component build --release
	mkdir -p $(DIST_DIR)
	cp $(WASM_SRC) $(WASM_OUT)

doctor:
	greentic-component doctor $(WASM_OUT)
"#
    .to_string()
}

fn render_lib_rs(_context: &WizardContext) -> String {
    r#"mod i18n;
mod qa;
mod schemas;

pub fn describe() -> Vec<u8> {
    schemas::describe_cbor().to_vec()
}

pub fn qa_spec(mode: &str) -> Vec<u8> {
    qa::qa_spec_cbor(mode).to_vec()
}

pub fn apply_answers(mode: &str, answers: Vec<u8>) -> Vec<u8> {
    qa::apply_answers(mode, answers)
}

pub fn run() -> Vec<u8> {
    Vec::new()
}
"#
    .to_string()
}

fn render_qa_rs(context: &WizardContext) -> String {
    let (default_prefill, setup_prefill) = match context.prefill_answers_cbor.as_ref() {
        Some(bytes) if context.prefill_mode == WizardMode::Default => {
            (bytes_literal(bytes), "&[]".to_string())
        }
        Some(bytes) if context.prefill_mode == WizardMode::Setup => {
            ("&[]".to_string(), bytes_literal(bytes))
        }
        _ => ("&[]".to_string(), "&[]".to_string()),
    };

    format!(
        r#"pub const QA_MODES: &[&str] = &["default", "setup", "upgrade", "remove"];

const DEFAULT_PREFILLED_ANSWERS_CBOR: &[u8] = {default_prefill};
const SETUP_PREFILLED_ANSWERS_CBOR: &[u8] = {setup_prefill};
const UPGRADE_PREFILLED_ANSWERS_CBOR: &[u8] = &[];
const REMOVE_PREFILLED_ANSWERS_CBOR: &[u8] = &[];

pub fn qa_spec_cbor(mode: &str) -> &'static [u8] {{
    match mode {{
        "default" => &[],
        "setup" => &[],
        "upgrade" => &[],
        "remove" => &[],
        _ => &[],
    }}
}}

pub fn prefilled_answers_cbor(mode: &str) -> &'static [u8] {{
    match mode {{
        "default" => DEFAULT_PREFILLED_ANSWERS_CBOR,
        "setup" => SETUP_PREFILLED_ANSWERS_CBOR,
        "upgrade" => UPGRADE_PREFILLED_ANSWERS_CBOR,
        "remove" => REMOVE_PREFILLED_ANSWERS_CBOR,
        _ => &[],
    }}
}}

pub fn apply_answers(_mode: &str, _answers: Vec<u8>) -> Vec<u8> {{
    // TODO: merge provided answers with defaults and return the resolved config.
    Vec::new()
}}
"#,
        default_prefill = default_prefill,
        setup_prefill = setup_prefill,
    )
}

fn render_schemas_rs() -> String {
    r#"pub fn describe_cbor() -> &'static [u8] {
    // TODO: return self-describing CBOR for the component descriptor.
    &[]
}
"#
    .to_string()
}

fn render_i18n_rs() -> String {
    r#"pub const I18N_KEYS: &[&str] = &[
    "component.title",
    "component.description",
    "qa.prompt.example",
];

pub fn all_keys() -> &'static [&'static str] {
    I18N_KEYS
}

pub fn contains(key: &str) -> bool {
    I18N_KEYS.iter().any(|value| value == &key)
}
"#
    .to_string()
}

fn render_wit_package() -> String {
    r#"package greentic:component@0.6.0;

world component {
    export describe: func() -> list<u8>;
    export qa-spec: func(mode: string) -> list<u8>;
    export apply-answers: func(mode: string, answers: list<u8>) -> list<u8>;
    export run: func() -> list<u8>;
}
"#
    .to_string()
}

fn render_example_answers(_mode: &str) -> String {
    r#"{
  "example": "value"
}
"#
    .to_string()
}

fn render_example_schema() -> String {
    r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "properties": {
    "example": {
      "type": "string"
    }
  },
  "additionalProperties": false
}
"#
    .to_string()
}

fn bytes_literal(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "&[]".to_string();
    }
    let rendered = bytes
        .iter()
        .map(|b| format!("0x{b:02x}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("&[{rendered}]")
}

fn encode_self_described_cbor(value: &JsonValue, out: &mut Vec<u8>) -> Result<()> {
    encode_tag(55799, out);
    encode_value(value, out)?;
    Ok(())
}

fn encode_value(value: &JsonValue, out: &mut Vec<u8>) -> Result<()> {
    match value {
        JsonValue::Null => out.push(0xf6),
        JsonValue::Bool(false) => out.push(0xf4),
        JsonValue::Bool(true) => out.push(0xf5),
        JsonValue::Number(num) => {
            if let Some(i) = num.as_i64() {
                encode_signed(i, out);
            } else if let Some(u) = num.as_u64() {
                encode_unsigned(0, u, out);
            } else if let Some(f) = num.as_f64() {
                encode_f64(f, out);
            } else {
                bail!("wizard: unsupported JSON number format");
            }
        }
        JsonValue::String(text) => encode_text(text, out),
        JsonValue::Array(values) => {
            encode_unsigned(4, values.len() as u64, out);
            for item in values {
                encode_value(item, out)?;
            }
        }
        JsonValue::Object(map) => {
            let mut items = map.iter().collect::<Vec<_>>();
            items.sort_by(|(a, _), (b, _)| {
                let a_bytes = a.as_bytes();
                let b_bytes = b.as_bytes();
                a_bytes
                    .len()
                    .cmp(&b_bytes.len())
                    .then_with(|| a_bytes.cmp(b_bytes))
            });
            encode_unsigned(5, items.len() as u64, out);
            for (key, value) in items {
                encode_text(key, out);
                encode_value(value, out)?;
            }
        }
    }
    Ok(())
}

fn encode_text(text: &str, out: &mut Vec<u8>) {
    encode_unsigned(3, text.len() as u64, out);
    out.extend_from_slice(text.as_bytes());
}

fn encode_signed(value: i64, out: &mut Vec<u8>) {
    if value >= 0 {
        encode_unsigned(0, value as u64, out);
    } else {
        let encoded = (-1 - value) as u64;
        encode_unsigned(1, encoded, out);
    }
}

fn encode_tag(tag: u64, out: &mut Vec<u8>) {
    encode_unsigned(6, tag, out);
}

fn encode_f64(value: f64, out: &mut Vec<u8>) {
    out.push(0xfb);
    out.extend_from_slice(&value.to_be_bytes());
}

fn encode_unsigned(major: u8, value: u64, out: &mut Vec<u8>) {
    let major = major << 5;
    match value {
        0..=23 => out.push(major | value as u8),
        24..=0xff => {
            out.push(major | 24);
            out.push(value as u8);
        }
        0x100..=0xffff => {
            out.push(major | 25);
            out.extend_from_slice(&(value as u16).to_be_bytes());
        }
        0x1_0000..=0xffff_ffff => {
            out.push(major | 26);
            out.extend_from_slice(&(value as u32).to_be_bytes());
        }
        _ => {
            out.push(major | 27);
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_text_deterministically() {
        let json = serde_json::json!({"b": 1, "a": 2});
        let mut out = Vec::new();
        encode_self_described_cbor(&json, &mut out).unwrap();
        assert!(!out.is_empty());
    }
}
