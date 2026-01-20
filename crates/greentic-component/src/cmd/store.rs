use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use clap::{Args, Subcommand};
use serde_json::Value;

use crate::path_safety::normalize_under_root;
use greentic_distributor_client::{DistClient, DistOptions};

#[derive(Subcommand, Debug, Clone)]
pub enum StoreCommand {
    /// Fetch a component from a source and write the wasm bytes to disk
    Fetch(StoreFetchArgs),
}

#[derive(Args, Debug, Clone)]
pub struct StoreFetchArgs {
    /// Destination directory for the fetched component bytes
    #[arg(long, value_name = "DIR")]
    pub out: PathBuf,
    /// Optional cache directory for fetched components
    #[arg(long, value_name = "DIR")]
    pub cache_dir: Option<PathBuf>,
    /// Source reference to resolve (file://, oci://, repo://, store://, etc.)
    #[arg(value_name = "SOURCE")]
    pub source: String,
}

pub fn run(command: StoreCommand) -> Result<()> {
    match command {
        StoreCommand::Fetch(args) => fetch(args),
    }
}

fn fetch(args: StoreFetchArgs) -> Result<()> {
    let mut opts = DistOptions::default();
    if let Some(cache_dir) = &args.cache_dir {
        opts.cache_dir = cache_dir.clone();
    }
    let client = DistClient::new(opts);
    let rt = tokio::runtime::Runtime::new().context("failed to create async runtime")?;
    let resolved = rt
        .block_on(async { client.ensure_cached(&args.source).await })
        .context("store fetch failed")?;
    let cache_path = resolved
        .cache_path
        .ok_or_else(|| anyhow!("resolved source has no cached component path"))?;
    let (out_dir, wasm_override) = resolve_output_paths(&args.out)?;
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create output dir {}", out_dir.display()))?;
    let manifest_cache_path = cache_path
        .parent()
        .map(|dir| dir.join("component.manifest.json"));
    let manifest_out_path = out_dir.join("component.manifest.json");
    let mut wasm_out_path = wasm_override.unwrap_or_else(|| out_dir.join("component.wasm"));
    if let Some(manifest_cache_path) = manifest_cache_path
        && manifest_cache_path.exists()
    {
        let manifest_bytes = fs::read(&manifest_cache_path).with_context(|| {
            format!(
                "failed to read cached manifest {}",
                manifest_cache_path.display()
            )
        })?;
        fs::write(&manifest_out_path, &manifest_bytes)
            .with_context(|| format!("failed to write manifest {}", manifest_out_path.display()))?;
        let manifest: Value = serde_json::from_slice(&manifest_bytes).with_context(|| {
            format!(
                "failed to parse component.manifest.json from {}",
                manifest_cache_path.display()
            )
        })?;
        if let Some(component_wasm) = manifest
            .get("artifacts")
            .and_then(|artifacts| artifacts.get("component_wasm"))
            .and_then(|value| value.as_str())
        {
            let candidate = PathBuf::from(component_wasm);
            if wasm_override.is_none() {
                wasm_out_path = normalize_under_root(&out_dir, &candidate).with_context(|| {
                    format!("invalid artifacts.component_wasm path `{}`", component_wasm)
                })?;
                if let Some(parent) = wasm_out_path.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create output dir {}", parent.display())
                    })?;
                }
            }
        }
    }
    fs::copy(&cache_path, &wasm_out_path).with_context(|| {
        format!(
            "failed to copy cached component {} to {}",
            cache_path.display(),
            wasm_out_path.display()
        )
    })?;
    println!(
        "Wrote {} (digest {}) for source {}",
        wasm_out_path.display(),
        resolved.digest,
        args.source,
    );
    if manifest_out_path.exists() {
        println!("Wrote {}", manifest_out_path.display());
    }
    Ok(())
}

fn resolve_output_paths(out: &PathBuf) -> Result<(PathBuf, Option<PathBuf>)> {
    if out.exists() {
        if out.is_dir() {
            return Ok((out.clone(), None));
        }
        if let Some(parent) = out.parent() {
            return Ok((parent.to_path_buf(), Some(out.clone())));
        }
        return Ok((PathBuf::from("."), Some(out.clone())));
    }

    if out.extension().is_some() {
        let parent = out.parent().unwrap_or_else(|| std::path::Path::new("."));
        return Ok((parent.to_path_buf(), Some(out.clone())));
    }

    Ok((out.clone(), None))
}
