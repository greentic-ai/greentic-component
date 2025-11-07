#[cfg(feature = "cli")]
use std::fs;
use std::process;

#[cfg(feature = "cli")]
use clap::Parser;
#[cfg(feature = "cli")]
use greentic_component::{ComponentError, manifest::validate_manifest, prepare_component};

#[cfg(feature = "cli")]
#[derive(Parser, Debug)]
#[command(about = "Run health checks against a Greentic component artifact")]
struct Args {
    /// Path or identifier resolvable by the loader
    target: String,
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("component-doctor requires the `cli` feature");
    process::exit(1);
}

#[cfg(feature = "cli")]
fn main() {
    if let Err(err) = run() {
        eprintln!("component-doctor[{}]: {err}", err.code());
        process::exit(1);
    }
}

#[cfg(feature = "cli")]
fn run() -> Result<(), ComponentError> {
    let args = Args::parse();
    let prepared = prepare_component(&args.target)?;

    let manifest_json = fs::read_to_string(&prepared.manifest_path)?;
    validate_manifest(&manifest_json)?;
    println!("manifest schema: ok");

    println!("hash verification: ok ({})", prepared.wasm_hash);
    println!("world check: ok ({})", prepared.manifest.world.as_str());
    println!(
        "lifecycle exports: init={} health={} shutdown={}",
        prepared.lifecycle.init, prepared.lifecycle.health, prepared.lifecycle.shutdown
    );
    println!(
        "describe payload versions: {}",
        prepared.describe.versions.len()
    );
    if prepared.redaction_paths().is_empty() {
        println!("redaction hints: none (ensure secrets use x-redact)");
    } else {
        println!("redaction hints: {}", prepared.redaction_paths().len());
        for path in prepared.redaction_paths() {
            println!("  - {}", path.as_str());
        }
    }
    if prepared.defaults_applied().is_empty() {
        println!("defaults applied: none");
    } else {
        println!("defaults applied:");
        for entry in prepared.defaults_applied() {
            println!("  - {entry}");
        }
    }
    println!(
        "capabilities declared: http={} secrets={} kv={} fs={} net={} tools={}",
        prepared.manifest.capabilities.http.is_some(),
        prepared.manifest.capabilities.secrets.is_some(),
        prepared.manifest.capabilities.kv.is_some(),
        prepared.manifest.capabilities.fs.is_some(),
        prepared.manifest.capabilities.net.is_some(),
        prepared.manifest.capabilities.tools.is_some()
    );
    println!("limits configured: {}", prepared.manifest.limits.is_some());
    Ok(())
}
