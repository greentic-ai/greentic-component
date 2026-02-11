use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Parser};
use serde::Serialize;
use serde_json::Value;
use wasmtime::component::{Component, Linker, Val};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use super::path::strip_file_scheme;
use crate::{ComponentError, PreparedComponent, prepare_component_with_manifest};
use greentic_types::cbor::canonical;
use greentic_types::schemas::common::schema_ir::{AdditionalProperties, SchemaIr};
use greentic_types::schemas::component::v0_6_0::{ComponentDescribe, schema_hash};

#[derive(Args, Debug, Clone)]
#[command(about = "Inspect a Greentic component artifact")]
pub struct InspectArgs {
    /// Path or identifier resolvable by the loader
    #[arg(value_name = "TARGET", required_unless_present = "describe")]
    pub target: Option<String>,
    /// Explicit path to component.manifest.json when it is not adjacent to the wasm
    #[arg(long)]
    pub manifest: Option<PathBuf>,
    /// Inspect a pre-generated describe CBOR file (skip WASM execution)
    #[arg(long)]
    pub describe: Option<PathBuf>,
    /// Emit structured JSON instead of human output
    #[arg(long)]
    pub json: bool,
    /// Verify schema_hash values against typed SchemaIR
    #[arg(long)]
    pub verify: bool,
    /// Treat warnings as errors
    #[arg(long)]
    pub strict: bool,
}

#[derive(Parser, Debug)]
struct InspectCli {
    #[command(flatten)]
    args: InspectArgs,
}

pub fn parse_from_cli() -> InspectArgs {
    InspectCli::parse().args
}

#[derive(Default)]
pub struct InspectResult {
    pub warnings: Vec<String>,
}

pub fn run(args: &InspectArgs) -> Result<InspectResult, ComponentError> {
    if should_use_describe_path(args) {
        return inspect_describe(args);
    }

    let target = args
        .target
        .as_ref()
        .ok_or_else(|| ComponentError::Doctor("inspect target is required".to_string()))?;
    let manifest_override = args.manifest.as_deref().map(strip_file_scheme);
    let prepared = prepare_component_with_manifest(target, manifest_override.as_deref())?;
    if args.json {
        let json = serde_json::to_string_pretty(&build_report(&prepared))
            .expect("serializing inspect report");
        println!("{json}");
    } else {
        println!("component: {}", prepared.manifest.id.as_str());
        println!("  wasm: {}", prepared.wasm_path.display());
        println!("  world ok: {}", prepared.world_ok);
        println!("  hash: {}", prepared.wasm_hash);
        println!("  supports: {:?}", prepared.manifest.supports);
        println!(
            "  profiles: default={:?} supported={:?}",
            prepared.manifest.profiles.default, prepared.manifest.profiles.supported
        );
        println!(
            "  lifecycle: init={} health={} shutdown={}",
            prepared.lifecycle.init, prepared.lifecycle.health, prepared.lifecycle.shutdown
        );
        let caps = &prepared.manifest.capabilities;
        println!(
            "  capabilities: wasi(fs={}, env={}, random={}, clocks={}) host(secrets={}, state={}, messaging={}, events={}, http={}, telemetry={}, iac={})",
            caps.wasi.filesystem.is_some(),
            caps.wasi.env.is_some(),
            caps.wasi.random,
            caps.wasi.clocks,
            caps.host.secrets.is_some(),
            caps.host.state.is_some(),
            caps.host.messaging.is_some(),
            caps.host.events.is_some(),
            caps.host.http.is_some(),
            caps.host.telemetry.is_some(),
            caps.host.iac.is_some(),
        );
        println!(
            "  limits: {}",
            prepared
                .manifest
                .limits
                .as_ref()
                .map(|l| format!("{} MB / {} ms", l.memory_mb, l.wall_time_ms))
                .unwrap_or_else(|| "default".into())
        );
        println!(
            "  telemetry prefix: {}",
            prepared
                .manifest
                .telemetry
                .as_ref()
                .map(|t| t.span_prefix.as_str())
                .unwrap_or("<none>")
        );
        println!("  describe versions: {}", prepared.describe.versions.len());
        println!("  redaction paths: {}", prepared.redaction_paths().len());
        println!("  defaults applied: {}", prepared.defaults_applied().len());
    }
    Ok(InspectResult::default())
}

pub fn emit_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
}

pub fn build_report(prepared: &PreparedComponent) -> Value {
    let caps = &prepared.manifest.capabilities;
    serde_json::json!({
        "manifest": &prepared.manifest,
        "manifest_path": prepared.manifest_path,
        "wasm_path": prepared.wasm_path,
        "wasm_hash": prepared.wasm_hash,
        "hash_verified": prepared.hash_verified,
        "world": {
            "expected": prepared.manifest.world.as_str(),
            "ok": prepared.world_ok,
        },
        "lifecycle": {
            "init": prepared.lifecycle.init,
            "health": prepared.lifecycle.health,
            "shutdown": prepared.lifecycle.shutdown,
        },
        "describe": prepared.describe,
        "capabilities": prepared.manifest.capabilities,
        "limits": prepared.manifest.limits,
        "telemetry": prepared.manifest.telemetry,
        "redactions": prepared
            .redaction_paths()
            .iter()
            .map(|p| p.as_str().to_string())
            .collect::<Vec<_>>(),
        "defaults_applied": prepared.defaults_applied(),
        "summary": {
            "supports": prepared.manifest.supports,
            "profiles": prepared.manifest.profiles,
            "capabilities": {
                "wasi": {
                    "filesystem": caps.wasi.filesystem.is_some(),
                    "env": caps.wasi.env.is_some(),
                    "random": caps.wasi.random,
                    "clocks": caps.wasi.clocks
                },
                "host": {
                    "secrets": caps.host.secrets.is_some(),
                    "state": caps.host.state.is_some(),
                    "messaging": caps.host.messaging.is_some(),
                    "events": caps.host.events.is_some(),
                    "http": caps.host.http.is_some(),
                    "telemetry": caps.host.telemetry.is_some(),
                    "iac": caps.host.iac.is_some()
                }
            },
        }
    })
}

fn should_use_describe_path(args: &InspectArgs) -> bool {
    if args.describe.is_some() {
        return true;
    }
    if args.manifest.is_some() {
        return false;
    }
    let Some(target) = args.target.as_ref() else {
        return false;
    };
    let target = strip_file_scheme(Path::new(target));
    target
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("wasm"))
        .unwrap_or(false)
}

fn inspect_describe(args: &InspectArgs) -> Result<InspectResult, ComponentError> {
    let mut warnings = Vec::new();
    let mut wasm_path = None;
    let bytes = if let Some(path) = args.describe.as_ref() {
        let path = strip_file_scheme(path);
        fs::read(path)
            .map_err(|err| ComponentError::Doctor(format!("failed to read describe file: {err}")))?
    } else {
        let target = args
            .target
            .as_ref()
            .ok_or_else(|| ComponentError::Doctor("inspect target is required".to_string()))?;
        let path = resolve_wasm_path(target).map_err(ComponentError::Doctor)?;
        wasm_path = Some(path.clone());
        call_describe(&path).map_err(ComponentError::Doctor)?
    };

    let payload = strip_self_describe_tag(&bytes);
    if let Err(err) = ensure_canonical_allow_floats(payload) {
        warnings.push(format!("describe payload not canonical: {err}"));
    }
    let describe: ComponentDescribe = canonical::from_cbor(payload)
        .map_err(|err| ComponentError::Doctor(format!("describe decode failed: {err}")))?;

    let mut report = DescribeReport::from(describe, args.verify)?;
    report.wasm_path = wasm_path;

    if args.json {
        let json = serde_json::to_string_pretty(&report)
            .map_err(|err| ComponentError::Doctor(format!("failed to encode json: {err}")))?;
        println!("{json}");
    } else {
        emit_describe_human(&report);
    }

    let verify_failed = args.verify
        && report
            .operations
            .iter()
            .any(|op| matches!(op.schema_hash_valid, Some(false)));
    if verify_failed {
        return Err(ComponentError::Doctor(
            "schema_hash verification failed".to_string(),
        ));
    }

    Ok(InspectResult { warnings })
}

fn emit_describe_human(report: &DescribeReport) {
    println!("component: {}", report.info.id);
    println!("  version: {}", report.info.version);
    println!("  role: {}", report.info.role);
    println!("  operations: {}", report.operations.len());
    for op in &report.operations {
        println!("  - {} ({})", op.id, op.schema_hash);
        println!("    input: {}", op.input.summary);
        println!("    output: {}", op.output.summary);
        if let Some(status) = op.schema_hash_valid {
            println!("    schema_hash ok: {status}");
        }
    }
    println!("  config: {}", report.config.summary);
}

#[derive(Debug, Serialize)]
struct DescribeReport {
    info: ComponentInfoSummary,
    operations: Vec<OperationSummary>,
    config: SchemaSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    wasm_path: Option<PathBuf>,
}

impl DescribeReport {
    fn from(describe: ComponentDescribe, verify: bool) -> Result<Self, ComponentError> {
        let info = ComponentInfoSummary {
            id: describe.info.id,
            version: describe.info.version,
            role: describe.info.role,
        };
        let config = SchemaSummary::from_schema(&describe.config_schema);
        let mut operations = Vec::new();
        for op in describe.operations {
            let input = SchemaSummary::from_schema(&op.input.schema);
            let output = SchemaSummary::from_schema(&op.output.schema);
            let schema_hash_valid = if verify {
                let expected =
                    schema_hash(&op.input.schema, &op.output.schema, &describe.config_schema)
                        .map_err(|err| {
                            ComponentError::Doctor(format!("schema_hash failed: {err}"))
                        })?;
                Some(expected == op.schema_hash)
            } else {
                None
            };
            operations.push(OperationSummary {
                id: op.id,
                schema_hash: op.schema_hash,
                schema_hash_valid,
                input,
                output,
            });
        }
        Ok(Self {
            info,
            operations,
            config,
            wasm_path: None,
        })
    }
}

#[derive(Debug, Serialize)]
struct ComponentInfoSummary {
    id: String,
    version: String,
    role: String,
}

#[derive(Debug, Serialize)]
struct OperationSummary {
    id: String,
    schema_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    schema_hash_valid: Option<bool>,
    input: SchemaSummary,
    output: SchemaSummary,
}

#[derive(Debug, Serialize)]
struct SchemaSummary {
    kind: String,
    summary: String,
}

impl SchemaSummary {
    fn from_schema(schema: &SchemaIr) -> Self {
        let (kind, summary) = summarize_schema(schema);
        Self { kind, summary }
    }
}

fn summarize_schema(schema: &SchemaIr) -> (String, String) {
    match schema {
        SchemaIr::Object {
            properties,
            required,
            additional,
        } => {
            let add = match additional {
                AdditionalProperties::Allow => "allow",
                AdditionalProperties::Forbid => "forbid",
                AdditionalProperties::Schema(_) => "schema",
            };
            let summary = format!(
                "object{{fields={}, required={}, additional={add}}}",
                properties.len(),
                required.len()
            );
            ("object".to_string(), summary)
        }
        SchemaIr::Array {
            min_items,
            max_items,
            ..
        } => (
            "array".to_string(),
            format!("array{{min={:?}, max={:?}}}", min_items, max_items),
        ),
        SchemaIr::String {
            min_len,
            max_len,
            format,
            ..
        } => (
            "string".to_string(),
            format!(
                "string{{min={:?}, max={:?}, format={:?}}}",
                min_len, max_len, format
            ),
        ),
        SchemaIr::Int { min, max } => (
            "int".to_string(),
            format!("int{{min={:?}, max={:?}}}", min, max),
        ),
        SchemaIr::Float { min, max } => (
            "float".to_string(),
            format!("float{{min={:?}, max={:?}}}", min, max),
        ),
        SchemaIr::Enum { values } => (
            "enum".to_string(),
            format!("enum{{values={}}}", values.len()),
        ),
        SchemaIr::OneOf { variants } => (
            "oneof".to_string(),
            format!("oneof{{variants={}}}", variants.len()),
        ),
        SchemaIr::Bool => ("bool".to_string(), "bool".to_string()),
        SchemaIr::Null => ("null".to_string(), "null".to_string()),
        SchemaIr::Bytes => ("bytes".to_string(), "bytes".to_string()),
        SchemaIr::Ref { id } => ("ref".to_string(), format!("ref{{id={id}}}")),
    }
}

fn resolve_wasm_path(target: &str) -> Result<PathBuf, String> {
    let target_path = strip_file_scheme(Path::new(target));
    if target_path.is_file() {
        return Ok(target_path.to_path_buf());
    }
    if target_path.is_dir()
        && let Some(found) = find_wasm_in_dir(&target_path)?
    {
        return Ok(found);
    }
    Err(format!("inspect: unable to resolve wasm for '{target}'"))
}

fn find_wasm_in_dir(dir: &Path) -> Result<Option<PathBuf>, String> {
    let mut candidates = Vec::new();
    let dist = dir.join("dist");
    if dist.is_dir() {
        collect_wasm_files(&dist, &mut candidates)?;
    }
    let target = dir.join("target").join("wasm32-wasip2");
    for profile in ["release", "debug"] {
        let profile_dir = target.join(profile);
        if profile_dir.is_dir() {
            collect_wasm_files(&profile_dir, &mut candidates)?;
        }
    }
    candidates.sort();
    candidates.dedup();
    match candidates.len() {
        0 => Ok(None),
        1 => Ok(Some(candidates.remove(0))),
        _ => Err(format!(
            "inspect: multiple wasm files found in {}; specify one explicitly",
            dir.display()
        )),
    }
}

fn collect_wasm_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(dir).map_err(|err| format!("failed to read {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read {}: {err}", dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("wasm") {
            out.push(path);
        }
    }
    Ok(())
}

fn call_describe(wasm_path: &Path) -> Result<Vec<u8>, String> {
    let mut config = wasmtime::Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config).map_err(|err| format!("engine init failed: {err}"))?;
    let component = Component::from_file(&engine, wasm_path)
        .map_err(|err| format!("failed to load component: {err}"))?;
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
        .map_err(|err| format!("failed to add wasi: {err}"))?;
    let mut store = Store::new(&engine, InspectWasi::new().map_err(|e| e.to_string())?);
    let instance = linker
        .instantiate(&mut store, &component)
        .map_err(|err| format!("failed to instantiate: {err}"))?;
    let instance_index = instance
        .get_export_index(&mut store, None, "component-descriptor")
        .ok_or_else(|| "missing export interface component-descriptor".to_string())?;
    let func_index = instance
        .get_export_index(&mut store, Some(&instance_index), "describe")
        .ok_or_else(|| "missing export component-descriptor.describe".to_string())?;
    let func = instance
        .get_func(&mut store, func_index)
        .ok_or_else(|| "describe export is not callable".to_string())?;
    let mut results = vec![Val::Bool(false); func.ty(&mut store).results().len()];
    func.call(&mut store, &[], &mut results)
        .map_err(|err| format!("describe call failed: {err}"))?;
    func.post_return(&mut store)
        .map_err(|err| format!("post-return failed: {err}"))?;
    let val = results
        .first()
        .ok_or_else(|| "describe returned no value".to_string())?;
    val_to_bytes(val)
}

fn val_to_bytes(val: &Val) -> Result<Vec<u8>, String> {
    match val {
        Val::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Val::U8(byte) => out.push(*byte),
                    _ => return Err("expected list<u8>".to_string()),
                }
            }
            Ok(out)
        }
        _ => Err("expected list<u8>".to_string()),
    }
}

fn strip_self_describe_tag(bytes: &[u8]) -> &[u8] {
    const SELF_DESCRIBE_TAG: [u8; 3] = [0xd9, 0xd9, 0xf7];
    if bytes.starts_with(&SELF_DESCRIBE_TAG) {
        &bytes[SELF_DESCRIBE_TAG.len()..]
    } else {
        bytes
    }
}

fn ensure_canonical_allow_floats(bytes: &[u8]) -> Result<(), String> {
    let canonicalized = canonical::canonicalize_allow_floats(bytes)
        .map_err(|err| format!("canonicalization failed: {err}"))?;
    if canonicalized.as_slice() != bytes {
        return Err("payload is not canonical".to_string());
    }
    Ok(())
}

struct InspectWasi {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl InspectWasi {
    fn new() -> Result<Self, anyhow::Error> {
        let ctx = WasiCtxBuilder::new().build();
        Ok(Self {
            ctx,
            table: ResourceTable::new(),
        })
    }
}

impl WasiView for InspectWasi {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}
