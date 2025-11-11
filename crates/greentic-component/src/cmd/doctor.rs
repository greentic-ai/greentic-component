use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Parser};

use crate::{ComponentError, manifest::validate_manifest, prepare_component};

#[derive(Args, Debug, Clone)]
#[command(about = "Run health checks against a Greentic component artifact")]
pub struct DoctorArgs {
    /// Path or identifier resolvable by the loader
    pub target: String,
}

#[derive(Parser, Debug)]
struct DoctorCli {
    #[command(flatten)]
    args: DoctorArgs,
}

pub fn parse_from_cli() -> DoctorArgs {
    DoctorCli::parse().args
}

pub fn run(args: DoctorArgs) -> Result<(), ComponentError> {
    if let Some(report) = detect_scaffold(&args.target) {
        report.print();
        return Ok(());
    }
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

fn detect_scaffold(target: &str) -> Option<ScaffoldReport> {
    let path = PathBuf::from(target);
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_dir() {
        return None;
    }
    ScaffoldReport::from_dir(&path)
}

struct ScaffoldReport {
    root: PathBuf,
    manifest: bool,
    cargo: bool,
    wit: bool,
    schemas: bool,
    src: bool,
}

impl ScaffoldReport {
    fn from_dir(root: &Path) -> Option<Self> {
        let manifest = root.join("component.manifest.json");
        if !manifest.exists() {
            return None;
        }
        Some(Self {
            root: root.to_path_buf(),
            manifest: manifest.is_file(),
            cargo: root.join("Cargo.toml").is_file(),
            wit: root.join("wit").is_dir(),
            schemas: root.join("schemas").is_dir(),
            src: root.join("src").is_dir(),
        })
    }

    fn print(&self) {
        println!("Detected Greentic scaffold at {}", self.root.display());
        self.print_line("component.manifest.json", self.manifest);
        self.print_line("Cargo.toml", self.cargo);
        self.print_line("src/", self.src);
        self.print_line("wit/", self.wit);
        self.print_line("schemas/", self.schemas);
        if self.is_complete() {
            println!(
                "Next steps: run `cargo check --target wasm32-wasip2` and `greentic-component doctor` once the wasm is built."
            );
        } else {
            println!(
                "Some scaffold pieces are missing. Re-run `greentic-component new` or restore the template files."
            );
        }
    }

    fn print_line(&self, label: &str, ok: bool) {
        if ok {
            println!("  [ok] {label}");
        } else {
            println!("  [missing] {label}");
        }
    }

    fn is_complete(&self) -> bool {
        self.manifest && self.wit && self.schemas
    }
}
