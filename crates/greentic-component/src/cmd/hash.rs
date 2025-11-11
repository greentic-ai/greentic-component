use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser};
use serde_json::Value;

#[derive(Args, Debug, Clone)]
#[command(about = "Recompute the wasm hash inside component.manifest.json")]
pub struct HashArgs {
    /// Path to component.manifest.json
    #[arg(default_value = "component.manifest.json")]
    pub manifest: PathBuf,
    /// Optional override for the wasm artifact path
    #[arg(long)]
    pub wasm: Option<PathBuf>,
}

#[derive(Parser, Debug)]
struct HashCli {
    #[command(flatten)]
    args: HashArgs,
}

pub fn parse_from_cli() -> HashArgs {
    HashCli::parse().args
}

pub fn run(args: HashArgs) -> Result<()> {
    let manifest_path = args.manifest;
    let manifest_text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let mut manifest: Value = serde_json::from_str(&manifest_text)
        .with_context(|| format!("invalid json: {}", manifest_path.display()))?;
    let wasm_path = resolve_wasm_path(&manifest, args.wasm.as_deref(), &manifest_path)?;
    let wasm_bytes = fs::read(&wasm_path)
        .with_context(|| format!("failed to read wasm at {}", wasm_path.display()))?;
    let digest = blake3::hash(&wasm_bytes).to_hex().to_string();
    manifest["hashes"]["component_wasm"] = Value::String(format!("blake3:{digest}"));
    let formatted = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, formatted + "\n")
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    println!(
        "Updated {} with hash of {}",
        manifest_path.display(),
        wasm_path.display()
    );
    Ok(())
}

fn resolve_wasm_path(
    manifest: &Value,
    override_path: Option<&Path>,
    manifest_path: &Path,
) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(path.to_path_buf());
    }
    let artifact = manifest
        .get("artifacts")
        .and_then(|art| art.get("component_wasm"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("manifest is missing artifacts.component_wasm"))?;
    let root = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(root.join(artifact))
}
