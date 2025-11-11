use clap::{Args, Parser};
use serde_json::Value;

use crate::{ComponentError, PreparedComponent, prepare_component};

#[derive(Args, Debug, Clone)]
#[command(about = "Inspect a Greentic component artifact")]
pub struct InspectArgs {
    /// Path or identifier resolvable by the loader
    pub target: String,
    /// Emit structured JSON instead of human output
    #[arg(long)]
    pub json: bool,
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
    let prepared = prepare_component(&args.target)?;
    if args.json {
        let json = serde_json::to_string_pretty(&build_report(&prepared))
            .expect("serializing inspect report");
        println!("{json}");
    } else {
        println!("component: {}", prepared.manifest.id.as_str());
        println!("  wasm: {}", prepared.wasm_path.display());
        println!("  world ok: {}", prepared.world_ok);
        println!("  hash: {}", prepared.wasm_hash);
        println!(
            "  lifecycle: init={} health={} shutdown={}",
            prepared.lifecycle.init, prepared.lifecycle.health, prepared.lifecycle.shutdown
        );
        println!("  capabilities: {:?}", prepared.manifest.capabilities);
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
            "capabilities": {
                "http": prepared.manifest.capabilities.http.is_some(),
                "secrets": prepared.manifest.capabilities.secrets.is_some(),
                "kv": prepared.manifest.capabilities.kv.is_some(),
                "fs": prepared.manifest.capabilities.fs.is_some(),
                "net": prepared.manifest.capabilities.net.is_some(),
                "tools": prepared.manifest.capabilities.tools.is_some(),
            }
        }
    })
}
