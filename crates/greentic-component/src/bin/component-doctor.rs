#[cfg(feature = "cli")]
use std::fs;
#[cfg(feature = "cli")]
use std::path::{Path, PathBuf};
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

#[cfg(feature = "cli")]
fn detect_scaffold(target: &str) -> Option<ScaffoldReport> {
    let path = PathBuf::from(target);
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_dir() {
        return None;
    }
    ScaffoldReport::from_dir(&path)
}

#[cfg(feature = "cli")]
struct ScaffoldReport {
    root: PathBuf,
    manifest: bool,
    cargo: bool,
    wit: bool,
    schemas: bool,
    src: bool,
}

#[cfg(feature = "cli")]
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
