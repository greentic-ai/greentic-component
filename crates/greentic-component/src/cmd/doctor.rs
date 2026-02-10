use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Parser};
use toml::Value as TomlValue;
use wit_parser::{Resolve, WorldItem};

use super::component_world::{canonical_component_world, is_fallback_world};
use super::path::strip_file_scheme;
use crate::{
    ComponentError,
    abi::{self, AbiError},
    manifest::validate_manifest,
    prepare_component_with_manifest,
    schema_quality::{SchemaQualityMode, validate_operation_schemas},
};

#[derive(Args, Debug, Clone)]
#[command(about = "Run health checks against a Greentic component artifact")]
pub struct DoctorArgs {
    /// Path or identifier resolvable by the loader
    pub target: String,
    /// Explicit path to component.manifest.json when it is not adjacent to the wasm
    #[arg(long)]
    pub manifest: Option<PathBuf>,
    /// Do not fail on empty operation schemas; emit warnings instead
    #[arg(long)]
    pub permissive: bool,
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
    let target_path = strip_file_scheme(Path::new(&args.target));
    if args.manifest.is_none()
        && let Some(report) = WizardReport::from_dir(&target_path)
    {
        report.run()?;
        return Ok(());
    }
    if args.manifest.is_none()
        && let Some(report) = detect_scaffold(&target_path)
    {
        report.print();
        return Ok(());
    }
    let manifest_override = args.manifest.as_deref().map(strip_file_scheme);
    let prepared = prepare_component_with_manifest(&args.target, manifest_override.as_deref())?;

    let manifest_json = fs::read_to_string(&prepared.manifest_path)?;
    validate_manifest(&manifest_json)?;
    let mode = if args.permissive {
        SchemaQualityMode::Permissive
    } else {
        SchemaQualityMode::Strict
    };
    let schema_warnings = validate_operation_schemas(&prepared.manifest, mode)?;
    println!("manifest schema: ok");
    for warning in schema_warnings {
        eprintln!("warning[W_OP_SCHEMA_EMPTY]: {}", warning.message);
    }

    println!("hash verification: ok ({})", prepared.wasm_hash);
    let canonical_world = canonical_component_world();
    if std::env::var_os("GREENTIC_SKIP_NODE_EXPORT_CHECK").is_some() {
        println!("world export: skipped (GREENTIC_SKIP_NODE_EXPORT_CHECK=1)");
    } else {
        match abi::check_world_base(&prepared.wasm_path, canonical_world) {
            Ok(found) => {
                println!("world export: {canonical_world} (found {found})");
            }
            Err(err) => match err {
                AbiError::WorldMismatch { expected, found } if is_fallback_world(&found) => {
                    println!("world export: fallback {found} (expected {expected})");
                }
                err => return Err(err.into()),
            },
        }
    }
    println!(
        "manifest world: {} (compares with exported world)",
        prepared.manifest.world.as_str()
    );
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
    let caps = &prepared.manifest.capabilities;
    println!("supports: {:?}", prepared.manifest.supports);
    println!(
        "capabilities declared: wasi(fs={}, env={}, random={}, clocks={}) host(secrets={}, state={}, messaging={}, events={}, http={}, telemetry={}, iac={})",
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
        caps.host.iac.is_some()
    );
    println!("limits configured: {}", prepared.manifest.limits.is_some());
    Ok(())
}

fn detect_scaffold(path: &Path) -> Option<ScaffoldReport> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_dir() {
        return None;
    }
    ScaffoldReport::from_dir(path)
}

struct ScaffoldReport {
    root: PathBuf,
    manifest: bool,
    cargo: bool,
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
            schemas: root.join("schemas").is_dir(),
            src: root.join("src").is_dir(),
        })
    }

    fn print(&self) {
        println!("Detected Greentic scaffold at {}", self.root.display());
        self.print_line("component.manifest.json", self.manifest);
        self.print_line("Cargo.toml", self.cargo);
        self.print_line("src/", self.src);
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
        self.manifest && self.schemas
    }
}

struct WizardReport {
    root: PathBuf,
    cargo: PathBuf,
    makefile: PathBuf,
    wit: PathBuf,
    qa: PathBuf,
    i18n: PathBuf,
}

impl WizardReport {
    fn from_dir(root: &Path) -> Option<Self> {
        let cargo = root.join("Cargo.toml");
        let makefile = root.join("Makefile");
        let wit = root.join("wit").join("package.wit");
        let qa = root.join("src").join("qa.rs");
        let i18n = root.join("src").join("i18n.rs");
        if cargo.is_file() && wit.is_file() {
            Some(Self {
                root: root.to_path_buf(),
                cargo,
                makefile,
                wit,
                qa,
                i18n,
            })
        } else {
            None
        }
    }

    fn run(&self) -> Result<(), ComponentError> {
        println!(
            "Detected Greentic wizard scaffold (component@0.6.0) at {}",
            self.root.display()
        );
        let mut errors = Vec::new();

        if let Err(err) = self.check_wit_exports() {
            errors.push(err);
        } else {
            println!("wit world exports: ok");
        }

        if let Err(err) = self.check_qa_modes() {
            errors.push(err);
        } else {
            println!("qa modes: ok (default/setup/upgrade/remove)");
        }

        if let Err(err) = self.check_i18n_keys() {
            errors.push(err);
        } else {
            println!("i18n keys: ok");
        }

        if let Err(err) = self.check_abi_metadata() {
            errors.push(err);
        } else {
            println!("abi metadata: ok");
        }

        if let Err(err) = self.check_makefile() {
            errors.push(err);
        } else {
            println!("makefile targets: ok");
        }

        println!("cbor validation: skipped (template scaffold)");

        if errors.is_empty() {
            return Ok(());
        }
        for error in &errors {
            eprintln!("error[WIZARD_CHECK]: {error}");
        }
        Err(ComponentError::Doctor(
            "wizard scaffold validation failed".to_string(),
        ))
    }

    fn check_wit_exports(&self) -> Result<(), String> {
        let contents = fs::read_to_string(&self.wit)
            .map_err(|err| format!("failed to read {}: {err}", self.wit.display()))?;
        let mut resolve = Resolve::default();
        let pkg = resolve
            .push_str(self.wit.display().to_string(), &contents)
            .map_err(|err| format!("failed to parse wit: {err}"))?;

        let world_name = resolve.packages[pkg]
            .worlds
            .keys()
            .next()
            .map(String::as_str)
            .unwrap_or("component");
        let world = resolve
            .select_world(&[pkg], Some(world_name))
            .map_err(|err| format!("failed to select world: {err}"))?;

        let pkg_meta = &resolve.packages[pkg].name;
        if pkg_meta.namespace != "greentic" || pkg_meta.name != "component" {
            return Err(format!(
                "unexpected wit package: {}:{}",
                pkg_meta.namespace, pkg_meta.name
            ));
        }
        if pkg_meta.version.as_ref().map(|v| v.to_string()).as_deref() != Some("0.6.0") {
            return Err("wit package version must be 0.6.0".to_string());
        }

        let mut exports = HashSet::new();
        let world = &resolve.worlds[world];
        for (_key, item) in &world.exports {
            match item {
                WorldItem::Function(func) => {
                    exports.insert(func.name.to_lowercase());
                }
                WorldItem::Interface { id, .. } => {
                    for (func, _) in resolve.interfaces[*id].functions.iter() {
                        exports.insert(func.to_lowercase());
                    }
                }
                WorldItem::Type(_) => {}
            }
        }

        let required = ["describe", "qa-spec", "apply-answers", "run"];
        let missing = required
            .iter()
            .filter(|name| !exports.iter().any(|export| export == *name))
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!("wit world missing exports: {}", missing.join(", ")));
        }
        Ok(())
    }

    fn check_qa_modes(&self) -> Result<(), String> {
        let contents = fs::read_to_string(&self.qa)
            .map_err(|err| format!("failed to read {}: {err}", self.qa.display()))?;
        let required = ["\"default\"", "\"setup\"", "\"upgrade\"", "\"remove\""];
        let missing = required
            .iter()
            .filter(|mode| !contents.contains(*mode))
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "qa modes missing in src/qa.rs: {}",
                missing.join(", ")
            ));
        }
        Ok(())
    }

    fn check_i18n_keys(&self) -> Result<(), String> {
        let contents = fs::read_to_string(&self.i18n)
            .map_err(|err| format!("failed to read {}: {err}", self.i18n.display()))?;
        if !contents.contains("I18N_KEYS") {
            return Err("src/i18n.rs missing I18N_KEYS registry".to_string());
        }
        Ok(())
    }

    fn check_abi_metadata(&self) -> Result<(), String> {
        let contents = fs::read_to_string(&self.cargo)
            .map_err(|err| format!("failed to read {}: {err}", self.cargo.display()))?;
        let doc: TomlValue =
            toml::from_str(&contents).map_err(|err| format!("invalid Cargo.toml: {err}"))?;
        let abi_version = doc
            .get("package")
            .and_then(|pkg| pkg.get("metadata"))
            .and_then(|meta| meta.get("greentic"))
            .and_then(|greentic| greentic.get("abi_version"))
            .and_then(|value| value.as_str());
        if abi_version.is_none() {
            return Err("missing [package.metadata.greentic] abi_version".to_string());
        }
        Ok(())
    }

    fn check_makefile(&self) -> Result<(), String> {
        if !self.makefile.is_file() {
            return Err("missing Makefile".to_string());
        }
        let contents = fs::read_to_string(&self.makefile)
            .map_err(|err| format!("failed to read {}: {err}", self.makefile.display()))?;
        let required = ["build:", "wasm:", "doctor:"];
        let missing = required
            .iter()
            .filter(|target| !contents.contains(*target))
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!("Makefile missing targets: {}", missing.join(", ")));
        }
        Ok(())
    }
}
