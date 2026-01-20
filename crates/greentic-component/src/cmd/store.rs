use std::fs;
use std::path::{Path, PathBuf};

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
    let source = resolve_source(&args.source)?;
    let mut opts = DistOptions::default();
    if let Some(cache_dir) = &args.cache_dir {
        opts.cache_dir = cache_dir.clone();
    }
    let client = DistClient::new(opts);
    let rt = tokio::runtime::Runtime::new().context("failed to create async runtime")?;
    let resolved = rt
        .block_on(async { client.ensure_cached(&source).await })
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
    let mut wasm_out_path = wasm_override
        .clone()
        .unwrap_or_else(|| out_dir.join("component.wasm"));
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
        source,
    );
    if manifest_out_path.exists() {
        println!("Wrote {}", manifest_out_path.display());
    }
    Ok(())
}

fn resolve_source(source: &str) -> Result<String> {
    let (prefix, path_str) = if let Some(rest) = source.strip_prefix("file://") {
        ("file://", rest)
    } else {
        ("", source)
    };
    let path = Path::new(path_str);
    if !path.is_dir() {
        return Ok(source.to_string());
    }

    let manifest_path = path.join("component.manifest.json");
    if manifest_path.exists() {
        let manifest_bytes = fs::read(&manifest_path).with_context(|| {
            format!(
                "failed to read component.manifest.json at {}",
                manifest_path.display()
            )
        })?;
        let manifest: Value = serde_json::from_slice(&manifest_bytes).with_context(|| {
            format!(
                "failed to parse component.manifest.json at {}",
                manifest_path.display()
            )
        })?;
        if let Some(component_wasm) = manifest
            .get("artifacts")
            .and_then(|artifacts| artifacts.get("component_wasm"))
            .and_then(|value| value.as_str())
        {
            let wasm_path = normalize_under_root(path, Path::new(component_wasm)).with_context(
                || format!("invalid artifacts.component_wasm path `{}`", component_wasm),
            )?;
            return Ok(format!("{prefix}{}", wasm_path.display()));
        }
    }

    let wasm_path = path.join("component.wasm");
    if wasm_path.exists() {
        return Ok(format!("{prefix}{}", wasm_path.display()));
    }

    Err(anyhow!(
        "source directory {} does not contain component.manifest.json or component.wasm",
        path.display()
    ))
}

fn resolve_output_paths(out: &std::path::Path) -> Result<(PathBuf, Option<PathBuf>)> {
    if out.exists() {
        if out.is_dir() {
            return Ok((out.to_path_buf(), None));
        }
        if let Some(parent) = out.parent() {
            return Ok((parent.to_path_buf(), Some(out.to_path_buf())));
        }
        return Ok((PathBuf::from("."), Some(out.to_path_buf())));
    }

    if out.extension().is_some() {
        let parent = out.parent().unwrap_or_else(|| std::path::Path::new("."));
        return Ok((parent.to_path_buf(), Some(out.to_path_buf())));
    }

    Ok((out.to_path_buf(), None))
}
