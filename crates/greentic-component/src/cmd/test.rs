use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use clap::{ArgAction, Args};
use serde_json::Value;
use uuid::Uuid;

use crate::capabilities::FilesystemMode;
use crate::manifest::ComponentManifest;
use crate::manifest::parse_manifest;
use crate::test_harness::{HarnessConfig, TestHarness, WasiPreopen};
use greentic_types::{EnvId, TeamId, TenantCtx, TenantId, UserId};

#[derive(Args, Debug)]
pub struct TestArgs {
    /// Path to the component wasm binary.
    #[arg(long, value_name = "PATH")]
    pub wasm: PathBuf,
    /// Optional manifest path (defaults to component.manifest.json next to the wasm).
    #[arg(long, value_name = "PATH")]
    pub manifest: Option<PathBuf>,
    /// Operation to invoke (repeat for multi-step runs).
    #[arg(long, value_name = "OP", action = ArgAction::Append)]
    pub op: Vec<String>,
    /// Input JSON file path (repeat for multi-step runs).
    #[arg(long, value_name = "PATH", action = ArgAction::Append, conflicts_with = "input_json")]
    pub input: Vec<PathBuf>,
    /// Inline input JSON string (repeat for multi-step runs).
    #[arg(long, value_name = "JSON", action = ArgAction::Append, conflicts_with = "input")]
    pub input_json: Vec<String>,
    /// Write output JSON to a file.
    #[arg(long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Pretty-print JSON output.
    #[arg(long)]
    pub pretty: bool,
    /// Dump in-memory state after invocation.
    #[arg(long)]
    pub state_dump: bool,
    /// Seed in-memory state as KEY=BASE64 (repeatable).
    #[arg(long = "state-set", value_name = "KEY=BASE64")]
    pub state_set: Vec<String>,
    /// Repeatable step marker for multi-step runs.
    #[arg(long, action = ArgAction::Count)]
    pub step: u8,
    /// Load secrets from a .env style file.
    #[arg(long, value_name = "PATH")]
    pub secrets: Option<PathBuf>,
    /// Load secrets from a JSON map file.
    #[arg(long, value_name = "PATH")]
    pub secrets_json: Option<PathBuf>,
    /// Provide a secret inline as KEY=VALUE (repeatable).
    #[arg(long = "secret", value_name = "KEY=VALUE")]
    pub secret: Vec<String>,
    /// Environment identifier for the exec context.
    #[arg(long, default_value = "dev")]
    pub env: String,
    /// Tenant identifier for the exec context.
    #[arg(long, default_value = "default")]
    pub tenant: String,
    /// Optional team identifier for the exec context.
    #[arg(long)]
    pub team: Option<String>,
    /// Optional user identifier for the exec context.
    #[arg(long)]
    pub user: Option<String>,
    /// Optional flow identifier for the exec context.
    #[arg(long)]
    pub flow: Option<String>,
    /// Optional node identifier for the exec context.
    #[arg(long)]
    pub node: Option<String>,
    /// Optional session identifier for the exec context.
    #[arg(long)]
    pub session: Option<String>,
    /// Emit extra diagnostic output (e.g. generated session id).
    #[arg(long)]
    pub verbose: bool,
}

pub fn run(args: TestArgs) -> Result<()> {
    let manifest_path = resolve_manifest_path(&args.wasm, args.manifest.as_deref())?;
    let manifest_raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read manifest {}", manifest_path.display()))?;
    let manifest_value: Value =
        serde_json::from_str(&manifest_raw).context("manifest must be valid JSON")?;
    let manifest = parse_manifest(&manifest_raw).context("parse manifest")?;

    let steps = collect_steps(&args)?;
    for (op, _) in &steps {
        if !manifest
            .operations
            .iter()
            .any(|operation| operation.name == *op)
        {
            bail!("operation `{op}` not declared in manifest");
        }
    }
    let wasm_bytes =
        fs::read(&args.wasm).with_context(|| format!("read wasm {}", args.wasm.display()))?;

    let (tenant_ctx, session_id, generated_session) = build_tenant_ctx(&args)?;
    if args.verbose && generated_session {
        eprintln!("generated session id: {session_id}");
    }

    let (allow_state_read, allow_state_write, allow_state_delete) =
        state_permissions(&manifest_value, &manifest);
    if !args.state_set.is_empty() && !allow_state_write {
        bail!("manifest does not declare host.state.write; add it to use --state-set");
    }
    let (allow_secrets, allowed_secrets) = secret_permissions(&manifest);

    let secrets = load_secrets(&args)?;
    if !allow_secrets && !secrets.is_empty() {
        bail!("manifest does not declare host.secrets; add host.secrets to enable secrets access");
    }

    let state_seeds = parse_state_seeds(&args)?;
    let wasi_preopens = resolve_wasi_preopens(&manifest)?;
    let prefix = state_prefix(args.flow.as_deref(), &session_id);
    let flow_id = args.flow.clone().unwrap_or_else(|| "test".to_string());
    let harness = TestHarness::new(HarnessConfig {
        wasm_bytes,
        tenant_ctx: tenant_ctx.clone(),
        flow_id,
        node_id: args.node.clone(),
        state_prefix: prefix,
        state_seeds,
        allow_state_read,
        allow_state_write,
        allow_state_delete,
        allow_secrets,
        allowed_secrets,
        secrets,
        wasi_preopens,
    })?;

    if steps.len() > 1 && args.output.is_some() {
        bail!("--output is only supported for single-step runs");
    }

    for (idx, (op, input)) in steps.iter().enumerate() {
        let output = harness.invoke(op, input)?;
        let output = format_output(&output, args.pretty)?;
        if let Some(path) = &args.output {
            fs::write(path, output.as_bytes())
                .with_context(|| format!("write output {}", path.display()))?;
        }
        if steps.len() > 1 {
            println!("step {} output:\n{output}", idx + 1);
        } else {
            println!("{output}");
        }
    }

    if args.state_dump {
        let dump = harness.state_dump();
        let dump_json = serde_json::to_string_pretty(&dump).unwrap_or_else(|_| "{}".into());
        eprintln!("state dump:\n{dump_json}");
    }

    Ok(())
}

fn resolve_manifest_path(wasm: &Path, manifest: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = manifest {
        return Ok(path.to_path_buf());
    }
    let dir = wasm
        .parent()
        .ok_or_else(|| anyhow::anyhow!("wasm path has no parent directory"))?;
    let candidate = dir.join("component.manifest.json");
    if candidate.exists() {
        Ok(candidate)
    } else {
        bail!(
            "manifest not found; pass --manifest or place component.manifest.json next to the wasm"
        );
    }
}

fn collect_steps(args: &TestArgs) -> Result<Vec<(String, Value)>> {
    if args.op.is_empty() {
        bail!("--op is required");
    }
    let inputs = if !args.input.is_empty() {
        let mut values = Vec::new();
        for path in &args.input {
            let raw = fs::read_to_string(path)
                .with_context(|| format!("read input {}", path.display()))?;
            values.push(serde_json::from_str(&raw).context("input file must be valid JSON")?);
        }
        values
    } else if !args.input_json.is_empty() {
        let mut values = Vec::new();
        for raw in &args.input_json {
            values.push(serde_json::from_str(raw).context("input-json must be valid JSON")?);
        }
        values
    } else {
        bail!("--input or --input-json is required");
    };

    if args.op.len() != inputs.len() {
        bail!("provide the same number of --op and --input/--input-json values");
    }
    if args.op.len() > 1 {
        let expected_steps = args.op.len().saturating_sub(1);
        if args.step == 0 {
            bail!("use --step to indicate a multi-step run");
        }
        if args.step as usize != expected_steps {
            bail!(
                "expected {expected_steps} --step flags for {} operations",
                args.op.len()
            );
        }
    }

    Ok(args.op.clone().into_iter().zip(inputs).collect())
}

fn build_tenant_ctx(args: &TestArgs) -> Result<(TenantCtx, String, bool)> {
    let env: EnvId = args.env.clone().try_into().context("invalid --env")?;
    let tenant: TenantId = args.tenant.clone().try_into().context("invalid --tenant")?;
    let mut ctx = TenantCtx::new(env, tenant);
    if let Some(team) = &args.team {
        let team: TeamId = team.clone().try_into().context("invalid --team")?;
        ctx = ctx.with_team(Some(team));
    }
    if let Some(user) = &args.user {
        let user: UserId = user.clone().try_into().context("invalid --user")?;
        ctx = ctx.with_user(Some(user));
    }

    let (session_id, generated) = match &args.session {
        Some(session) => (session.clone(), false),
        None => (Uuid::new_v4().to_string(), true),
    };
    ctx = ctx.with_session(session_id.clone());

    if let Some(flow) = &args.flow {
        ctx = ctx.with_flow(flow.clone());
    }
    if let Some(node) = &args.node {
        ctx = ctx.with_node(node.clone());
    }

    Ok((ctx, session_id, generated))
}

fn state_prefix(flow: Option<&str>, session: &str) -> String {
    if let Some(flow) = flow {
        format!("flow/{flow}/{session}")
    } else {
        format!("test/{session}")
    }
}

fn resolve_wasi_preopens(manifest: &ComponentManifest) -> Result<Vec<WasiPreopen>> {
    let Some(fs) = manifest.capabilities.wasi.filesystem.as_ref() else {
        return Ok(Vec::new());
    };
    if fs.mode == FilesystemMode::None {
        return Ok(Vec::new());
    }
    let host_root =
        std::env::current_dir().context("resolve current working directory for mounts")?;
    let meta = fs::metadata(&host_root)
        .with_context(|| format!("failed to stat preopen {}", host_root.display()))?;
    if !meta.is_dir() {
        bail!("preopen {} must be a directory", host_root.display());
    }
    let read_only = matches!(fs.mode, FilesystemMode::ReadOnly);
    let mut preopens = Vec::new();
    for mount in &fs.mounts {
        preopens.push(WasiPreopen::new(&host_root, mount.guest_path.clone()).read_only(read_only));
    }
    Ok(preopens)
}

fn state_permissions(
    manifest_value: &Value,
    manifest: &crate::manifest::ComponentManifest,
) -> (bool, bool, bool) {
    let mut allow_state_read = false;
    let mut allow_state_write = false;
    if let Some(state) = manifest.capabilities.host.state.as_ref() {
        allow_state_read = state.read;
        allow_state_write = state.write;
    }
    let allow_state_delete = manifest_value
        .get("capabilities")
        .and_then(|caps| caps.get("host"))
        .and_then(|host| host.get("state"))
        .and_then(|state| state.get("delete"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if allow_state_delete && !allow_state_write {
        allow_state_write = true;
    }
    (allow_state_read, allow_state_write, allow_state_delete)
}

fn secret_permissions(manifest: &crate::manifest::ComponentManifest) -> (bool, HashSet<String>) {
    let Some(secrets) = manifest.capabilities.host.secrets.as_ref() else {
        return (false, HashSet::new());
    };
    let allowed = secrets
        .required
        .iter()
        .map(|req| req.key.as_str().to_string())
        .collect::<HashSet<_>>();
    (true, allowed)
}

fn load_secrets(args: &TestArgs) -> Result<HashMap<String, String>> {
    let mut secrets = HashMap::new();
    if let Some(path) = &args.secrets {
        let entries = parse_env_file(path)?;
        secrets.extend(entries);
    }
    if let Some(path) = &args.secrets_json {
        let entries = parse_json_secrets(path)?;
        secrets.extend(entries);
    }
    for entry in &args.secret {
        let (key, value) = entry
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid --secret `{entry}`; use KEY=VALUE"))?;
        secrets.insert(key.to_string(), value.to_string());
    }
    Ok(secrets)
}

fn parse_state_seeds(args: &TestArgs) -> Result<Vec<(String, Vec<u8>)>> {
    let mut seeds = Vec::new();
    for entry in &args.state_set {
        let (key, value) = entry
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid --state-set `{entry}`; use KEY=BASE64"))?;
        let bytes = BASE64_STANDARD
            .decode(value)
            .with_context(|| format!("invalid base64 for state key `{key}`"))?;
        seeds.push((key.to_string(), bytes));
    }
    Ok(seeds)
}

fn parse_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("read secrets {}", path.display()))?;
    let mut secrets = HashMap::new();
    for (idx, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| {
            anyhow::anyhow!(
                "invalid secrets line {} in {} (expected KEY=VALUE)",
                idx + 1,
                path.display()
            )
        })?;
        secrets.insert(key.trim().to_string(), value.trim().to_string());
    }
    Ok(secrets)
}

fn parse_json_secrets(path: &Path) -> Result<HashMap<String, String>> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("read secrets {}", path.display()))?;
    let value: Value = serde_json::from_str(&contents).context("secrets JSON must be valid")?;
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("secrets JSON must be an object map"))?;
    let mut secrets = HashMap::new();
    for (key, value) in obj {
        let value = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("secret `{key}` must be a string value"))?;
        secrets.insert(key.clone(), value.to_string());
    }
    Ok(secrets)
}

fn format_output(raw: &str, pretty: bool) -> Result<String> {
    if !pretty {
        return Ok(raw.to_string());
    }
    let value: Value = serde_json::from_str(raw).context("output is not valid JSON")?;
    Ok(serde_json::to_string_pretty(&value)?)
}
