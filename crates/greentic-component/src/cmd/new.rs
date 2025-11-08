#![cfg(feature = "cli")]

use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Args;
use serde::Serialize;

use crate::scaffold::engine::{ScaffoldEngine, ScaffoldOutcome, ScaffoldRequest};
use crate::scaffold::validate::{self, ComponentName};

#[derive(Args, Debug, Clone)]
pub struct NewArgs {
    /// Name for the component (kebab-or-snake case)
    #[arg(long = "name", value_name = "kebab_or_snake", required = true)]
    pub name: String,
    /// Path to create the component (defaults to ./<name>)
    #[arg(long = "path", value_name = "dir")]
    pub path: Option<PathBuf>,
    /// Template identifier to scaffold from
    #[arg(
        long = "template",
        default_value = "rust-wasi-p2-min",
        value_name = "id"
    )]
    pub template: String,
    /// Reverse DNS-style organisation identifier
    #[arg(
        long = "org",
        default_value = "ai.greentic",
        value_name = "reverse.dns"
    )]
    pub org: String,
    /// Initial component version
    #[arg(long = "version", default_value = "0.1.0", value_name = "semver")]
    pub version: String,
    /// License to embed into generated sources
    #[arg(long = "license", default_value = "MIT", value_name = "id")]
    pub license: String,
    /// Exported WIT world name
    #[arg(long = "wit-world", default_value = "component", value_name = "name")]
    pub wit_world: String,
    /// Run without prompting for confirmation
    #[arg(long = "non-interactive")]
    pub non_interactive: bool,
    /// Skip the post-scaffold cargo check (hidden flag for testing/local dev)
    #[arg(long = "no-check", hide = true)]
    pub no_check: bool,
    /// Emit JSON instead of human-readable output
    #[arg(long = "json")]
    pub json: bool,
}

pub fn run(args: NewArgs, engine: &ScaffoldEngine) -> Result<()> {
    let request = build_request(&args)?;
    let outcome = engine.scaffold(request)?;
    let compile_check = run_compile_check(&outcome.path, args.no_check)?;
    if args.json {
        let payload = NewCliOutput {
            scaffold: &outcome,
            compile_check: &compile_check,
        };
        print_json(&payload)?;
    } else {
        print_human(&outcome, &compile_check);
    }
    if compile_check.ran && !compile_check.passed {
        anyhow::bail!("cargo check --target wasm32-wasip2 failed");
    }
    Ok(())
}

fn build_request(args: &NewArgs) -> Result<ScaffoldRequest> {
    let component_name = ComponentName::parse(&args.name)?;
    let target_path = resolve_path(&component_name, args.path.as_deref())?;
    Ok(ScaffoldRequest {
        name: component_name.into_string(),
        path: target_path,
        template_id: args.template.clone(),
        org: args.org.clone(),
        version: args.version.clone(),
        license: args.license.clone(),
        wit_world: args.wit_world.clone(),
        non_interactive: args.non_interactive,
        year_override: None,
    })
}

fn resolve_path(name: &ComponentName, provided: Option<&Path>) -> Result<PathBuf> {
    let path = validate::resolve_target_path(name, provided)?;
    Ok(path)
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let mut handle = std::io::stdout();
    serde_json::to_writer_pretty(&mut handle, value)?;
    handle.write_all(b"\n").ok();
    Ok(())
}

fn print_human(outcome: &ScaffoldOutcome, check: &CompileCheckReport) {
    println!("{}", outcome.human_summary());
    for path in &outcome.created {
        println!("  - {path}");
    }
    if !check.ran {
        println!("cargo check (wasm32-wasip2): skipped (--no-check)");
        return;
    }
    if check.passed {
        println!("cargo check (wasm32-wasip2): ok");
    } else {
        println!(
            "cargo check (wasm32-wasip2): FAILED (exit code {:?})",
            check.exit_code
        );
        if let Some(stderr) = &check.stderr {
            if !stderr.is_empty() {
                println!("{stderr}");
            }
        }
    }
}

fn run_compile_check(path: &Path, skip: bool) -> Result<CompileCheckReport> {
    const COMMAND_DISPLAY: &str = "cargo check --target wasm32-wasip2";
    if skip {
        return Ok(CompileCheckReport::skipped(COMMAND_DISPLAY));
    }
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = Command::new(cargo);
    cmd.arg("check").arg("--target").arg("wasm32-wasip2");
    cmd.current_dir(path);
    let start = Instant::now();
    let output = cmd
        .output()
        .with_context(|| format!("failed to run `{COMMAND_DISPLAY}`"))?;
    let duration_ms = start.elapsed().as_millis();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Ok(CompileCheckReport {
        command: COMMAND_DISPLAY.to_string(),
        ran: true,
        passed: output.status.success(),
        exit_code: output.status.code(),
        duration_ms: Some(duration_ms),
        stdout: if stdout.is_empty() {
            None
        } else {
            Some(stdout)
        },
        stderr: if stderr.is_empty() {
            None
        } else {
            Some(stderr)
        },
        reason: None,
    })
}

#[derive(Serialize)]
struct NewCliOutput<'a> {
    scaffold: &'a ScaffoldOutcome,
    compile_check: &'a CompileCheckReport,
}

#[derive(Debug, Serialize)]
struct CompileCheckReport {
    command: String,
    ran: bool,
    passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl CompileCheckReport {
    fn skipped(command: &str) -> Self {
        Self {
            command: command.to_string(),
            ran: false,
            passed: true,
            exit_code: None,
            duration_ms: None,
            stdout: None,
            stderr: None,
            reason: Some("skipped (--no-check)".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_uses_name() {
        let args = NewArgs {
            name: "demo-component".into(),
            path: None,
            template: "rust-wasi-p2-min".into(),
            org: "ai.greentic".into(),
            version: "0.1.0".into(),
            license: "MIT".into(),
            wit_world: "component".into(),
            non_interactive: false,
            no_check: false,
            json: false,
        };
        let request = build_request(&args).unwrap();
        assert!(request.path.ends_with("demo-component"));
    }
}
