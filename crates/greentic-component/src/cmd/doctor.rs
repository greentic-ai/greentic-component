use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, ValueEnum};
use serde::Serialize;
use serde_json::Value as JsonValue;
use wasmtime::component::{Component, Func, Linker, Val};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use super::path::strip_file_scheme;
use crate::cmd::component_world::is_fallback_world;
use crate::{ComponentError, abi, loader};

use greentic_types::cbor::canonical;
use greentic_types::schemas::common::schema_ir::{AdditionalProperties, SchemaIr};
use greentic_types::schemas::component::v0_6_0::{
    ComponentDescribe, ComponentInfo, ComponentQaSpec, QaMode, schema_hash,
};

const COMPONENT_WORLD_V0_6_0: &str = "greentic:component/component-v0-v6-v0@0.6.0";
const SELF_DESCRIBE_TAG: [u8; 3] = [0xd9, 0xd9, 0xf7];
const EMPTY_CBOR_MAP: [u8; 1] = [0xa0];

#[derive(Args, Debug, Clone)]
#[command(about = "Run health checks against a Greentic component artifact")]
pub struct DoctorArgs {
    /// Path or identifier resolvable by the loader
    pub target: String,
    /// Explicit path to component.manifest.json when it is not adjacent to the wasm
    #[arg(long)]
    pub manifest: Option<PathBuf>,
    /// Output format
    #[arg(long, value_enum, default_value = "human")]
    pub format: DoctorFormat,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorFormat {
    Human,
    Json,
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
    let wasm_path = resolve_wasm_path(&args.target, &target_path, args.manifest.as_deref())
        .map_err(ComponentError::Doctor)?;

    let report = DoctorReport::from_wasm(&wasm_path).map_err(ComponentError::Doctor)?;
    match args.format {
        DoctorFormat::Human => report.emit_human(),
        DoctorFormat::Json => report.emit_json()?,
    }

    if report.has_errors() {
        return Err(ComponentError::Doctor("doctor checks failed".to_string()));
    }
    Ok(())
}

fn resolve_wasm_path(
    raw_target: &str,
    target_path: &Path,
    manifest: Option<&Path>,
) -> Result<PathBuf, String> {
    if let Some(manifest_path) = manifest {
        let handle = loader::discover_with_manifest(raw_target, Some(manifest_path))
            .map_err(|err| format!("failed to load manifest: {err}"))?;
        return Ok(handle.wasm_path);
    }

    if target_path.is_file() {
        if target_path.extension().and_then(|ext| ext.to_str()) == Some("wasm") {
            return Ok(target_path.to_path_buf());
        }
        if target_path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            let handle = loader::discover_with_manifest(raw_target, Some(target_path))
                .map_err(|err| format!("failed to load manifest: {err}"))?;
            return Ok(handle.wasm_path);
        }
    }

    if target_path.is_dir()
        && let Some(found) = find_wasm_in_dir(target_path)?
    {
        return Ok(found);
    }

    Err(format!(
        "doctor: unable to resolve wasm for '{}'; pass a .wasm file or --manifest",
        raw_target
    ))
}

fn find_wasm_in_dir(dir: &Path) -> Result<Option<PathBuf>, String> {
    let mut candidates = Vec::new();
    let dist = dir.join("dist");
    if dist.is_dir() {
        collect_wasm_files(&dist, &mut candidates)?;
    }
    let target = dir.join("target").join("wasm32-wasip2");
    for profile in ["release", "debug"] {
        let profile_dir = target.join(profile);
        if profile_dir.is_dir() {
            collect_wasm_files(&profile_dir, &mut candidates)?;
        }
    }

    candidates.sort();
    candidates.dedup();
    match candidates.len() {
        0 => Ok(None),
        1 => Ok(Some(candidates.remove(0))),
        _ => Err(format!(
            "doctor: multiple wasm files found in {}; specify one explicitly",
            dir.display()
        )),
    }
}

fn collect_wasm_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(dir).map_err(|err| format!("failed to read {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read {}: {err}", dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("wasm") {
            out.push(path);
        }
    }
    Ok(())
}

#[derive(Default, Serialize)]
struct DoctorReport {
    diagnostics: Vec<DoctorDiagnostic>,
}

impl DoctorReport {
    fn from_wasm(wasm_path: &Path) -> Result<Self, String> {
        let mut report = DoctorReport::default();
        report.validate_world(wasm_path);

        let mut caller = ComponentCaller::new(wasm_path)
            .map_err(|err| format!("doctor: failed to load component: {err}"))?;

        let info_bytes = report.require_export_bytes(
            &mut caller,
            "component-descriptor",
            "get-component-info",
            &[],
        );
        let describe_bytes =
            report.require_export_bytes(&mut caller, "component-descriptor", "describe", &[]);
        let i18n_keys =
            report.require_export_strings(&mut caller, "component-i18n", "i18n-keys", &[]);

        report.require_export_call(
            &mut caller,
            "component-runtime",
            "run",
            &[
                Val::List(bytes_to_vals(&EMPTY_CBOR_MAP)),
                Val::List(bytes_to_vals(&EMPTY_CBOR_MAP)),
            ],
        );

        let mut qa_specs = BTreeMap::new();
        for (mode, mode_name) in qa_modes() {
            let spec_bytes = report.require_export_bytes(
                &mut caller,
                "component-qa",
                "qa-spec",
                &[Val::Enum(mode_name.to_string())],
            );
            if let Some(bytes) = spec_bytes.as_deref() {
                match decode_cbor::<ComponentQaSpec>(bytes) {
                    Ok(spec) => {
                        if spec.mode != mode {
                            report.error(
                                "doctor.qa.mode_mismatch",
                                format!("qa-spec returned {:?} for mode {mode_name}", spec.mode),
                                "qa-spec",
                                None,
                            );
                        }
                        qa_specs.insert(mode_name.to_string(), spec);
                    }
                    Err(err) => {
                        report.error(
                            "doctor.qa.decode_failed",
                            format!("qa-spec({mode_name}) decode failed: {err}"),
                            "qa-spec",
                            None,
                        );
                    }
                }
            }
        }

        if let Some(bytes) = info_bytes {
            match decode_cbor::<ComponentInfo>(&bytes) {
                Ok(info) => report.validate_info(&info, "get-component-info"),
                Err(err) => report.error(
                    "doctor.describe.info_decode_failed",
                    format!("get-component-info decode failed: {err}"),
                    "get-component-info",
                    None,
                ),
            }
        }

        if let Some(bytes) = describe_bytes {
            match decode_cbor::<ComponentDescribe>(&bytes) {
                Ok(describe) => {
                    report.validate_info(&describe.info, "describe");
                    report.validate_describe(&describe, &bytes);
                    report.validate_i18n(&i18n_keys, &qa_specs);
                    report.validate_apply_answers(&mut caller, &describe, &bytes);
                }
                Err(err) => report.error(
                    "doctor.describe.decode_failed",
                    format!("describe decode failed: {err}"),
                    "describe",
                    None,
                ),
            }
        }

        report.finalize();
        Ok(report)
    }

    fn validate_world(&mut self, wasm_path: &Path) {
        if let Err(err) = abi::check_world_base(wasm_path, COMPONENT_WORLD_V0_6_0) {
            match err {
                abi::AbiError::WorldMismatch { found, .. } if is_fallback_world(&found) => {}
                other => self.error(
                    "doctor.world.mismatch",
                    format!("component world mismatch: {other}"),
                    "world",
                    Some("expected component@0.6.0 world".to_string()),
                ),
            }
        }
    }

    fn validate_info(&mut self, info: &ComponentInfo, source: &str) {
        if info.id.trim().is_empty() {
            self.error(
                "doctor.describe.info.id_empty",
                format!("{source} info.id must be non-empty"),
                "info.id",
                None,
            );
        }
        if info.version.trim().is_empty() {
            self.error(
                "doctor.describe.info.version_empty",
                format!("{source} info.version must be non-empty"),
                "info.version",
                None,
            );
        }
        if info.role.trim().is_empty() {
            self.error(
                "doctor.describe.info.role_empty",
                format!("{source} info.role must be non-empty"),
                "info.role",
                None,
            );
        }
    }

    fn validate_describe(&mut self, describe: &ComponentDescribe, raw_bytes: &[u8]) {
        if let Err(err) = ensure_canonical_allow_floats(raw_bytes) {
            self.error(
                "doctor.describe.non_canonical",
                format!("describe CBOR is not canonical: {err}"),
                "describe",
                None,
            );
        }

        if describe.operations.is_empty() {
            self.error(
                "doctor.describe.missing_operations",
                "describe.operations must be non-empty".to_string(),
                "operations",
                None,
            );
        }

        self.validate_schema_ir(&describe.config_schema, "config_schema");

        for (idx, op) in describe.operations.iter().enumerate() {
            if op.id.trim().is_empty() {
                self.error(
                    "doctor.describe.operation.id_empty",
                    "operation id must be non-empty".to_string(),
                    format!("operations[{idx}].id"),
                    None,
                );
            }
            self.validate_schema_ir(&op.input.schema, format!("operations[{idx}].input.schema"));
            self.validate_schema_ir(
                &op.output.schema,
                format!("operations[{idx}].output.schema"),
            );

            match schema_hash(&op.input.schema, &op.output.schema, &describe.config_schema) {
                Ok(expected) => {
                    if op.schema_hash.trim().is_empty() {
                        self.error(
                            "doctor.describe.schema_hash.empty",
                            "schema_hash must be non-empty".to_string(),
                            format!("operations[{idx}].schema_hash"),
                            None,
                        );
                    } else if op.schema_hash != expected {
                        self.error(
                            "doctor.describe.schema_hash.mismatch",
                            format!(
                                "schema_hash mismatch (expected {expected}, got {})",
                                op.schema_hash
                            ),
                            format!("operations[{idx}].schema_hash"),
                            None,
                        );
                    }
                }
                Err(err) => self.error(
                    "doctor.describe.schema_hash.failed",
                    format!("schema_hash computation failed: {err}"),
                    format!("operations[{idx}].schema_hash"),
                    None,
                ),
            }
        }
    }

    fn validate_i18n(
        &mut self,
        i18n_keys: &Option<BTreeSet<String>>,
        qa_specs: &BTreeMap<String, ComponentQaSpec>,
    ) {
        let Some(keys) = i18n_keys else {
            self.error(
                "doctor.i18n.missing_keys",
                "i18n-keys export missing or failed".to_string(),
                "component-i18n",
                None,
            );
            return;
        };

        for (mode, spec) in qa_specs {
            for key in spec.i18n_keys() {
                if !keys.contains(&key) {
                    self.error(
                        "doctor.i18n.key_missing",
                        format!("missing i18n key {key} referenced in qa-spec({mode})"),
                        "component-i18n",
                        None,
                    );
                }
            }
        }
    }

    fn validate_apply_answers(
        &mut self,
        caller: &mut ComponentCaller,
        describe: &ComponentDescribe,
        describe_bytes: &[u8],
    ) {
        let context = describe_hash_context(describe, describe_bytes);
        for (_mode, mode_name) in qa_modes() {
            let bytes = self.require_export_bytes(
                caller,
                "component-qa",
                "apply-answers",
                &[
                    Val::Enum(mode_name.to_string()),
                    Val::List(bytes_to_vals(&EMPTY_CBOR_MAP)),
                    Val::List(bytes_to_vals(&EMPTY_CBOR_MAP)),
                ],
            );
            let Some(bytes) = bytes else {
                continue;
            };
            if let Err(err) = ensure_canonical_allow_floats(&bytes) {
                self.error(
                    "doctor.qa.apply_answers.non_canonical",
                    format!(
                        "apply-answers({mode_name}) returned non-canonical CBOR: {err}; {context}"
                    ),
                    format!("apply-answers.{mode_name}"),
                    None,
                );
            }
            match decode_cbor::<JsonValue>(&bytes) {
                Ok(value) => {
                    let mut issues = Vec::new();
                    validate_json_value(&describe.config_schema, &value, "$", &mut issues);
                    if !issues.is_empty() {
                        self.error(
                            "doctor.qa.apply_answers.schema_invalid",
                            format!(
                                "apply-answers({mode_name}) violates config_schema: {}; {context}",
                                format_validation_issues(&issues)
                            ),
                            format!("apply-answers.{mode_name}"),
                            None,
                        );
                    }
                }
                Err(err) => {
                    self.error(
                        "doctor.qa.apply_answers.decode_failed",
                        format!("apply-answers({mode_name}) decode failed: {err}; {context}"),
                        "apply-answers",
                        None,
                    );
                }
            }
        }
    }

    fn validate_schema_ir<P: Into<String>>(&mut self, schema: &SchemaIr, path: P) {
        let path = path.into();
        let mut errors = Vec::new();
        collect_schema_issues(schema, &path, &mut errors);
        for error in errors {
            self.error(error.code, error.message, error.path, error.hint);
        }
    }

    fn require_export_bytes(
        &mut self,
        caller: &mut ComponentCaller,
        interface: &str,
        func: &str,
        params: &[Val],
    ) -> Option<Vec<u8>> {
        match caller.call(interface, func, params) {
            Ok(values) => {
                if let Some(val) = values.first() {
                    match val_to_bytes(val) {
                        Ok(bytes) => Some(bytes),
                        Err(err) => {
                            self.error(
                                "doctor.export.invalid_bytes",
                                format!("{interface}.{func} returned invalid bytes: {err}"),
                                format!("{interface}.{func}"),
                                None,
                            );
                            None
                        }
                    }
                } else {
                    self.error(
                        "doctor.export.missing_result",
                        format!("{interface}.{func} returned no value"),
                        format!("{interface}.{func}"),
                        None,
                    );
                    None
                }
            }
            Err(err) => {
                self.error(
                    "doctor.export.call_failed",
                    format!("{interface}.{func} failed: {err}"),
                    format!("{interface}.{func}"),
                    None,
                );
                None
            }
        }
    }

    fn require_export_strings(
        &mut self,
        caller: &mut ComponentCaller,
        interface: &str,
        func: &str,
        params: &[Val],
    ) -> Option<BTreeSet<String>> {
        match caller.call(interface, func, params) {
            Ok(values) => {
                if let Some(val) = values.first() {
                    match val_to_strings(val) {
                        Ok(values) => Some(values.into_iter().collect()),
                        Err(err) => {
                            self.error(
                                "doctor.export.invalid_strings",
                                format!("{interface}.{func} returned invalid strings: {err}"),
                                format!("{interface}.{func}"),
                                None,
                            );
                            None
                        }
                    }
                } else {
                    self.error(
                        "doctor.export.missing_result",
                        format!("{interface}.{func} returned no value"),
                        format!("{interface}.{func}"),
                        None,
                    );
                    None
                }
            }
            Err(err) => {
                self.error(
                    "doctor.export.call_failed",
                    format!("{interface}.{func} failed: {err}"),
                    format!("{interface}.{func}"),
                    None,
                );
                None
            }
        }
    }

    fn require_export_call(
        &mut self,
        caller: &mut ComponentCaller,
        interface: &str,
        func: &str,
        params: &[Val],
    ) {
        if let Err(err) = caller.call(interface, func, params) {
            self.error(
                "doctor.export.call_failed",
                format!("{interface}.{func} failed: {err}"),
                format!("{interface}.{func}"),
                None,
            );
        }
    }

    fn error(
        &mut self,
        code: impl Into<String>,
        message: impl Into<String>,
        path: impl Into<String>,
        hint: Option<String>,
    ) {
        self.diagnostics.push(DoctorDiagnostic {
            severity: Severity::Error,
            code: code.into(),
            message: message.into(),
            path: path.into(),
            hint,
        });
    }

    fn finalize(&mut self) {
        self.diagnostics
            .sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.code.cmp(&b.code)));
    }

    fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diag| diag.severity == Severity::Error)
    }

    fn emit_human(&self) {
        if self.diagnostics.is_empty() {
            println!("doctor: ok");
            return;
        }
        for diag in &self.diagnostics {
            let hint = diag
                .hint
                .as_deref()
                .map(|hint| format!(" (hint: {hint})"))
                .unwrap_or_default();
            println!(
                "{severity}[{code}] {path}: {message}{hint}",
                severity = diag.severity,
                code = diag.code,
                path = diag.path,
                message = diag.message,
                hint = hint
            );
        }
    }

    fn emit_json(&self) -> Result<(), ComponentError> {
        let payload = serde_json::to_string_pretty(&self)
            .map_err(|err| ComponentError::Doctor(format!("failed to encode json: {err}")))?;
        println!("{payload}");
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Severity {
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct DoctorDiagnostic {
    severity: Severity,
    code: String,
    message: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

struct ComponentCaller {
    store: Store<DoctorWasi>,
    instance: wasmtime::component::Instance,
}

impl ComponentCaller {
    fn new(wasm_path: &Path) -> Result<Self, anyhow::Error> {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        let engine = Engine::new(&config)?;

        let component = Component::from_file(&engine, wasm_path)?;
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;

        let wasi = DoctorWasi::new()?;
        let mut store = Store::new(&engine, wasi);
        let instance = linker.instantiate(&mut store, &component)?;
        Ok(Self { store, instance })
    }

    fn call(&mut self, interface: &str, func: &str, params: &[Val]) -> Result<Vec<Val>, String> {
        let instance_index = resolve_interface_index(&self.instance, &mut self.store, interface)
            .ok_or_else(|| format!("missing export interface {interface}"))?;
        let func_index = self
            .instance
            .get_export_index(&mut self.store, Some(&instance_index), func)
            .ok_or_else(|| format!("missing export {interface}.{func}"))?;
        let func = self
            .instance
            .get_func(&mut self.store, func_index)
            .ok_or_else(|| format!("export {interface}.{func} is not callable"))?;

        call_component_func(&mut self.store, &func, params)
    }
}

fn resolve_interface_index(
    instance: &wasmtime::component::Instance,
    store: &mut Store<DoctorWasi>,
    interface: &str,
) -> Option<wasmtime::component::ComponentExportIndex> {
    for candidate in interface_candidates(interface) {
        if let Some(index) = instance.get_export_index(&mut *store, None, &candidate) {
            return Some(index);
        }
    }
    None
}

fn interface_candidates(interface: &str) -> [String; 3] {
    [
        interface.to_string(),
        format!("greentic:component/{interface}@0.6.0"),
        format!("greentic:component/{interface}"),
    ]
}

fn call_component_func(
    store: &mut Store<DoctorWasi>,
    func: &Func,
    params: &[Val],
) -> Result<Vec<Val>, String> {
    let results_len = func.ty(&mut *store).results().len();
    let mut results = vec![Val::Bool(false); results_len];
    func.call(&mut *store, params, &mut results)
        .map_err(|err| format!("call failed: {err}"))?;
    func.post_return(&mut *store)
        .map_err(|err| format!("post-return failed: {err}"))?;
    Ok(results)
}

fn qa_modes() -> [(QaMode, &'static str); 4] {
    [
        (QaMode::Default, "default"),
        (QaMode::Setup, "setup"),
        (QaMode::Update, "update"),
        (QaMode::Remove, "remove"),
    ]
}

fn bytes_to_vals(bytes: &[u8]) -> Vec<Val> {
    bytes.iter().map(|b| Val::U8(*b)).collect()
}

fn val_to_bytes(val: &Val) -> Result<Vec<u8>, String> {
    match val {
        Val::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Val::U8(byte) => out.push(*byte),
                    _ => {
                        return Err("expected list<u8>".to_string());
                    }
                }
            }
            Ok(out)
        }
        _ => Err("expected list<u8>".to_string()),
    }
}

fn val_to_strings(val: &Val) -> Result<Vec<String>, String> {
    match val {
        Val::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Val::String(value) => out.push(value.clone()),
                    _ => return Err("expected list<string>".to_string()),
                }
            }
            Ok(out)
        }
        _ => Err("expected list<string>".to_string()),
    }
}

fn decode_cbor<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let payload = strip_self_describe_tag(bytes);
    canonical::from_cbor(payload).map_err(|err| format!("CBOR decode failed: {err}"))
}

fn strip_self_describe_tag(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(&SELF_DESCRIBE_TAG) {
        &bytes[SELF_DESCRIBE_TAG.len()..]
    } else {
        bytes
    }
}

fn ensure_canonical_allow_floats(bytes: &[u8]) -> Result<(), String> {
    let payload = strip_self_describe_tag(bytes);
    let canonicalized = canonical::canonicalize_allow_floats(payload)
        .map_err(|err| format!("canonicalization failed: {err}"))?;
    if canonicalized.as_slice() != payload {
        return Err("payload is not canonical".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct SchemaIssue {
    code: String,
    message: String,
    path: String,
    hint: Option<String>,
}

fn collect_schema_issues(schema: &SchemaIr, path: &str, issues: &mut Vec<SchemaIssue>) {
    match schema {
        SchemaIr::Object {
            properties,
            required: _,
            additional,
        } => {
            if properties.is_empty() && matches!(additional, AdditionalProperties::Allow) {
                issues.push(SchemaIssue {
                    code: "doctor.schema.object.unconstrained".to_string(),
                    message: "object schema allows arbitrary additional properties without defined fields"
                        .to_string(),
                    path: path.to_string(),
                    hint: None,
                });
            }
            for (name, subschema) in properties {
                collect_schema_issues(subschema, &format!("{path}.{name}"), issues);
            }
            if let AdditionalProperties::Schema(schema) = additional {
                collect_schema_issues(schema, &format!("{path}.additional"), issues);
            }
        }
        SchemaIr::Array {
            items,
            min_items,
            max_items,
        } => {
            if min_items.is_none() && max_items.is_none() && is_unconstrained(items) {
                issues.push(SchemaIssue {
                    code: "doctor.schema.array.unconstrained".to_string(),
                    message: "array schema has no constraints".to_string(),
                    path: path.to_string(),
                    hint: None,
                });
            }
            collect_schema_issues(items, &format!("{path}.items"), issues);
        }
        SchemaIr::String {
            min_len,
            max_len,
            regex,
            format,
        } => {
            if min_len.is_none() && max_len.is_none() && regex.is_none() && format.is_none() {
                issues.push(SchemaIssue {
                    code: "doctor.schema.string.unconstrained".to_string(),
                    message: "string schema has no constraints".to_string(),
                    path: path.to_string(),
                    hint: None,
                });
            }
        }
        SchemaIr::Int { min, max } => {
            if min.is_none() && max.is_none() {
                issues.push(SchemaIssue {
                    code: "doctor.schema.int.unconstrained".to_string(),
                    message: "int schema has no constraints".to_string(),
                    path: path.to_string(),
                    hint: None,
                });
            }
        }
        SchemaIr::Float { min, max } => {
            if min.is_none() && max.is_none() {
                issues.push(SchemaIssue {
                    code: "doctor.schema.float.unconstrained".to_string(),
                    message: "float schema has no constraints".to_string(),
                    path: path.to_string(),
                    hint: None,
                });
            }
        }
        SchemaIr::Enum { values } => {
            if values.is_empty() {
                issues.push(SchemaIssue {
                    code: "doctor.schema.enum.empty".to_string(),
                    message: "enum schema must define at least one value".to_string(),
                    path: path.to_string(),
                    hint: None,
                });
            }
        }
        SchemaIr::OneOf { variants } => {
            if variants.is_empty() {
                issues.push(SchemaIssue {
                    code: "doctor.schema.oneof.empty".to_string(),
                    message: "oneof schema must define at least one variant".to_string(),
                    path: path.to_string(),
                    hint: None,
                });
            }
            for (idx, variant) in variants.iter().enumerate() {
                collect_schema_issues(variant, &format!("{path}.variants[{idx}]"), issues);
            }
        }
        SchemaIr::Ref { .. } => {
            issues.push(SchemaIssue {
                code: "doctor.schema.ref.unsupported".to_string(),
                message: "schema ref is not supported in strict mode".to_string(),
                path: path.to_string(),
                hint: None,
            });
        }
        SchemaIr::Bool | SchemaIr::Null | SchemaIr::Bytes => {}
    }
}

fn is_unconstrained(schema: &SchemaIr) -> bool {
    match schema {
        SchemaIr::Object {
            properties,
            additional,
            ..
        } => properties.is_empty() && matches!(additional, AdditionalProperties::Allow),
        SchemaIr::Array {
            min_items,
            max_items,
            items,
        } => min_items.is_none() && max_items.is_none() && is_unconstrained(items),
        SchemaIr::String {
            min_len,
            max_len,
            regex,
            format,
        } => min_len.is_none() && max_len.is_none() && regex.is_none() && format.is_none(),
        SchemaIr::Int { min, max } => min.is_none() && max.is_none(),
        SchemaIr::Float { min, max } => min.is_none() && max.is_none(),
        SchemaIr::Enum { values } => values.is_empty(),
        SchemaIr::OneOf { variants } => variants.is_empty(),
        SchemaIr::Ref { .. } => true,
        SchemaIr::Bool | SchemaIr::Null | SchemaIr::Bytes => false,
    }
}

#[derive(Debug)]
struct ValueIssue {
    path: String,
    message: String,
}

fn describe_hash_context(describe: &ComponentDescribe, describe_bytes: &[u8]) -> String {
    let describe_hash =
        compute_describe_hash(describe_bytes).unwrap_or_else(|err| format!("unavailable ({err})"));
    let schema_hashes = describe
        .operations
        .iter()
        .map(|op| format!("{}={}", op.id, op.schema_hash))
        .collect::<Vec<_>>();
    if schema_hashes.is_empty() {
        format!("describe_hash={describe_hash}")
    } else {
        format!(
            "describe_hash={describe_hash}; schema_hashes=[{}]",
            schema_hashes.join(", ")
        )
    }
}

fn compute_describe_hash(raw_bytes: &[u8]) -> Result<String, String> {
    let payload = strip_self_describe_tag(raw_bytes);
    let canonicalized = canonical::canonicalize_allow_floats(payload)
        .map_err(|err| format!("canonicalization failed: {err}"))?;
    Ok(blake3::hash(&canonicalized).to_hex().to_string())
}

fn format_validation_issues(issues: &[ValueIssue]) -> String {
    issues
        .iter()
        .take(8)
        .map(|issue| format!("{}: {}", issue.path, issue.message))
        .collect::<Vec<_>>()
        .join("; ")
}

fn validate_json_value(
    schema: &SchemaIr,
    value: &JsonValue,
    path: &str,
    issues: &mut Vec<ValueIssue>,
) {
    match schema {
        SchemaIr::Object {
            properties,
            required,
            additional,
        } => {
            let Some(obj) = value.as_object() else {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: "expected object".to_string(),
                });
                return;
            };
            for key in required {
                if !obj.contains_key(key) {
                    issues.push(ValueIssue {
                        path: format!("{path}/{key}"),
                        message: "required field missing".to_string(),
                    });
                }
            }
            for (key, subschema) in properties {
                if let Some(subvalue) = obj.get(key) {
                    validate_json_value(subschema, subvalue, &format!("{path}/{key}"), issues);
                }
            }
            for (key, subvalue) in obj {
                if properties.contains_key(key) {
                    continue;
                }
                match additional {
                    AdditionalProperties::Allow => {}
                    AdditionalProperties::Forbid => issues.push(ValueIssue {
                        path: format!("{path}/{key}"),
                        message: "additional property not allowed".to_string(),
                    }),
                    AdditionalProperties::Schema(extra_schema) => {
                        validate_json_value(
                            extra_schema,
                            subvalue,
                            &format!("{path}/{key}"),
                            issues,
                        );
                    }
                }
            }
        }
        SchemaIr::Array {
            items,
            min_items,
            max_items,
        } => {
            let Some(arr) = value.as_array() else {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: "expected array".to_string(),
                });
                return;
            };
            if let Some(min) = min_items
                && arr.len() < *min as usize
            {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: format!("expected at least {min} items"),
                });
            }
            if let Some(max) = max_items
                && arr.len() > *max as usize
            {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: format!("expected at most {max} items"),
                });
            }
            for (idx, item) in arr.iter().enumerate() {
                validate_json_value(items, item, &format!("{path}/{idx}"), issues);
            }
        }
        SchemaIr::String {
            min_len,
            max_len,
            regex,
            ..
        } => {
            let Some(s) = value.as_str() else {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: "expected string".to_string(),
                });
                return;
            };
            if let Some(min) = min_len
                && s.chars().count() < *min as usize
            {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: format!("expected minimum length {min}"),
                });
            }
            if let Some(max) = max_len
                && s.chars().count() > *max as usize
            {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: format!("expected maximum length {max}"),
                });
            }
            if let Some(pattern) = regex {
                match regex::Regex::new(pattern) {
                    Ok(re) => {
                        if !re.is_match(s) {
                            issues.push(ValueIssue {
                                path: path.to_string(),
                                message: format!("string does not match regex `{pattern}`"),
                            });
                        }
                    }
                    Err(err) => issues.push(ValueIssue {
                        path: path.to_string(),
                        message: format!("invalid schema regex `{pattern}`: {err}"),
                    }),
                }
            }
        }
        SchemaIr::Int { min, max } => {
            let Some(i) = value.as_i64() else {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: "expected integer".to_string(),
                });
                return;
            };
            if let Some(min) = min
                && i < *min
            {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: format!("expected value >= {min}"),
                });
            }
            if let Some(max) = max
                && i > *max
            {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: format!("expected value <= {max}"),
                });
            }
        }
        SchemaIr::Float { min, max } => {
            let Some(f) = value.as_f64() else {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: "expected number".to_string(),
                });
                return;
            };
            if let Some(min) = min
                && f < *min
            {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: format!("expected value >= {min}"),
                });
            }
            if let Some(max) = max
                && f > *max
            {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: format!("expected value <= {max}"),
                });
            }
        }
        SchemaIr::Enum { values } => match json_to_cbor_value(value) {
            Ok(cbor_value) => {
                if !values.iter().any(|candidate| candidate == &cbor_value) {
                    issues.push(ValueIssue {
                        path: path.to_string(),
                        message: "value not present in enum".to_string(),
                    });
                }
            }
            Err(err) => {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: format!("failed to normalize enum value: {err}"),
                });
            }
        },
        SchemaIr::OneOf { variants } => {
            let any_match = variants.iter().any(|variant| {
                let mut inner = Vec::new();
                validate_json_value(variant, value, path, &mut inner);
                inner.is_empty()
            });
            if !any_match {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: "value does not match any oneOf variant".to_string(),
                });
            }
        }
        SchemaIr::Bool => {
            if !value.is_boolean() {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: "expected boolean".to_string(),
                });
            }
        }
        SchemaIr::Null => {
            if !value.is_null() {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: "expected null".to_string(),
                });
            }
        }
        SchemaIr::Bytes => {
            if !value.is_string() && !value.is_array() {
                issues.push(ValueIssue {
                    path: path.to_string(),
                    message: "expected bytes-like value".to_string(),
                });
            }
        }
        SchemaIr::Ref { id } => {
            issues.push(ValueIssue {
                path: path.to_string(),
                message: format!("schema ref `{id}` is unsupported for strict validation"),
            });
        }
    }
}

fn json_to_cbor_value(value: &JsonValue) -> Result<ciborium::Value, String> {
    let bytes = canonical::to_canonical_cbor_allow_floats(value)
        .map_err(|err| format!("CBOR encode failed: {err}"))?;
    canonical::from_cbor(&bytes).map_err(|err| format!("CBOR decode failed: {err}"))
}

struct DoctorWasi {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl DoctorWasi {
    fn new() -> Result<Self, anyhow::Error> {
        let ctx = WasiCtxBuilder::new().build();
        Ok(Self {
            ctx,
            table: ResourceTable::new(),
        })
    }
}

impl WasiView for DoctorWasi {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_types::i18n_text::I18nText;
    use greentic_types::schemas::component::v0_6_0::{
        ChoiceOption, ComponentDescribe, ComponentInfo, ComponentOperation, ComponentQaSpec,
        ComponentRunInput, ComponentRunOutput, QaMode, Question, QuestionKind, RedactionKind,
        RedactionRule,
    };
    use serde_json::json;

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("doctor")
            .join(name)
    }

    fn load_or_update_fixture(name: &str, expected: &[u8]) -> Vec<u8> {
        let path = fixture_path(name);
        if std::env::var("UPDATE_DOCTOR_FIXTURES").is_ok() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create fixture dir");
            }
            fs::write(&path, expected).expect("write fixture");
        }
        fs::read(&path).expect("fixture exists")
    }

    fn object_schema(props: Vec<(&str, SchemaIr)>) -> SchemaIr {
        let mut properties = BTreeMap::new();
        let mut required = Vec::new();
        for (name, schema) in props {
            properties.insert(name.to_string(), schema);
            required.push(name.to_string());
        }
        SchemaIr::Object {
            properties,
            required,
            additional: AdditionalProperties::Forbid,
        }
    }

    fn good_describe() -> ComponentDescribe {
        let info = ComponentInfo {
            id: "com.greentic.demo".to_string(),
            version: "0.1.0".to_string(),
            role: "tool".to_string(),
            display_name: None,
        };
        let input_schema = object_schema(vec![(
            "name",
            SchemaIr::String {
                min_len: Some(1),
                max_len: None,
                regex: None,
                format: None,
            },
        )]);
        let output_schema = object_schema(vec![("ok", SchemaIr::Bool)]);
        let config_schema = object_schema(vec![("enabled", SchemaIr::Bool)]);
        let schema_hash =
            schema_hash(&input_schema, &output_schema, &config_schema).expect("schema hash");
        let operation = ComponentOperation {
            id: "run".to_string(),
            display_name: None,
            input: ComponentRunInput {
                schema: input_schema,
            },
            output: ComponentRunOutput {
                schema: output_schema,
            },
            defaults: BTreeMap::new(),
            redactions: Vec::new(),
            constraints: BTreeMap::new(),
            schema_hash,
        };
        ComponentDescribe {
            info,
            provided_capabilities: Vec::new(),
            required_capabilities: Vec::new(),
            metadata: BTreeMap::new(),
            operations: vec![operation],
            config_schema,
        }
    }

    fn bad_missing_ops_describe() -> ComponentDescribe {
        let mut describe = good_describe();
        describe.operations.clear();
        describe
    }

    fn bad_unconstrained_describe() -> ComponentDescribe {
        let info = ComponentInfo {
            id: "com.greentic.demo".to_string(),
            version: "0.1.0".to_string(),
            role: "tool".to_string(),
            display_name: None,
        };
        let input_schema = SchemaIr::String {
            min_len: None,
            max_len: None,
            regex: None,
            format: None,
        };
        let output_schema = SchemaIr::Bool;
        let config_schema = SchemaIr::Object {
            properties: BTreeMap::new(),
            required: Vec::new(),
            additional: AdditionalProperties::Allow,
        };
        let schema_hash =
            schema_hash(&input_schema, &output_schema, &config_schema).expect("schema hash");
        let operation = ComponentOperation {
            id: "run".to_string(),
            display_name: None,
            input: ComponentRunInput {
                schema: input_schema,
            },
            output: ComponentRunOutput {
                schema: output_schema,
            },
            defaults: BTreeMap::new(),
            redactions: vec![RedactionRule {
                json_pointer: "/secret".to_string(),
                kind: RedactionKind::Secret,
            }],
            constraints: BTreeMap::new(),
            schema_hash,
        };
        ComponentDescribe {
            info,
            provided_capabilities: Vec::new(),
            required_capabilities: Vec::new(),
            metadata: BTreeMap::new(),
            operations: vec![operation],
            config_schema,
        }
    }

    fn bad_hash_describe() -> ComponentDescribe {
        let mut describe = good_describe();
        if let Some(op) = describe.operations.first_mut() {
            op.schema_hash = "deadbeef".to_string();
        }
        describe
    }

    fn encode_describe(describe: &ComponentDescribe) -> Vec<u8> {
        canonical::to_canonical_cbor_allow_floats(describe).expect("encode cbor")
    }

    fn has_code(report: &DoctorReport, code: &str) -> bool {
        report.diagnostics.iter().any(|diag| diag.code == code)
    }

    #[test]
    fn fixtures_match_expected_payloads() {
        let good_bytes = encode_describe(&good_describe());
        let fixture = load_or_update_fixture("good_component_describe.cbor", &good_bytes);
        assert_eq!(fixture, good_bytes);

        let missing_ops_bytes = encode_describe(&bad_missing_ops_describe());
        let fixture = load_or_update_fixture(
            "bad_component_describe_missing_ops.cbor",
            &missing_ops_bytes,
        );
        assert_eq!(fixture, missing_ops_bytes);

        let unconstrained_bytes = encode_describe(&bad_unconstrained_describe());
        let fixture = load_or_update_fixture(
            "bad_component_describe_unconstrained_schema.cbor",
            &unconstrained_bytes,
        );
        assert_eq!(fixture, unconstrained_bytes);

        let hash_bytes = encode_describe(&bad_hash_describe());
        let fixture =
            load_or_update_fixture("bad_component_describe_hash_mismatch.cbor", &hash_bytes);
        assert_eq!(fixture, hash_bytes);
    }

    #[test]
    fn doctor_accepts_good_describe_fixture() {
        let bytes = load_or_update_fixture(
            "good_component_describe.cbor",
            &encode_describe(&good_describe()),
        );
        let describe: ComponentDescribe = decode_cbor(&bytes).expect("decode describe");
        let mut report = DoctorReport::default();
        report.validate_info(&describe.info, "describe");
        report.validate_describe(&describe, &bytes);
        report.finalize();
        assert!(
            !report.has_errors(),
            "expected no diagnostics, got {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn doctor_rejects_missing_ops_fixture() {
        let bytes = load_or_update_fixture(
            "bad_component_describe_missing_ops.cbor",
            &encode_describe(&bad_missing_ops_describe()),
        );
        let describe: ComponentDescribe = decode_cbor(&bytes).expect("decode describe");
        let mut report = DoctorReport::default();
        report.validate_describe(&describe, &bytes);
        report.finalize();
        assert!(has_code(&report, "doctor.describe.missing_operations"));
    }

    #[test]
    fn doctor_rejects_unconstrained_schema_fixture() {
        let bytes = load_or_update_fixture(
            "bad_component_describe_unconstrained_schema.cbor",
            &encode_describe(&bad_unconstrained_describe()),
        );
        let describe: ComponentDescribe = decode_cbor(&bytes).expect("decode describe");
        let mut report = DoctorReport::default();
        report.validate_describe(&describe, &bytes);
        report.finalize();
        assert!(
            has_code(&report, "doctor.schema.object.unconstrained")
                || has_code(&report, "doctor.schema.string.unconstrained"),
            "expected unconstrained schema diagnostics, got {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn doctor_rejects_hash_mismatch_fixture() {
        let bytes = load_or_update_fixture(
            "bad_component_describe_hash_mismatch.cbor",
            &encode_describe(&bad_hash_describe()),
        );
        let describe: ComponentDescribe = decode_cbor(&bytes).expect("decode describe");
        let mut report = DoctorReport::default();
        report.validate_describe(&describe, &bytes);
        report.finalize();
        assert!(has_code(&report, "doctor.describe.schema_hash.mismatch"));
    }

    #[test]
    fn doctor_flags_missing_i18n_keys() {
        let qa_spec = ComponentQaSpec {
            mode: QaMode::Default,
            title: I18nText::new("qa.title", None),
            description: Some(I18nText::new("qa.desc", None)),
            questions: vec![Question {
                id: "name".to_string(),
                label: I18nText::new("qa.question.name", None),
                help: None,
                error: None,
                kind: QuestionKind::Choice {
                    options: vec![ChoiceOption {
                        value: "one".to_string(),
                        label: I18nText::new("qa.option.one", None),
                    }],
                },
                required: true,
                default: None,
            }],
            defaults: BTreeMap::new(),
        };
        let mut qa_specs = BTreeMap::new();
        qa_specs.insert("default".to_string(), qa_spec);

        let keys = BTreeSet::from_iter(["qa.title".to_string()]);
        let mut report = DoctorReport::default();
        report.validate_i18n(&Some(keys), &qa_specs);
        report.finalize();
        assert!(has_code(&report, "doctor.i18n.key_missing"));
    }

    #[test]
    fn validation_issues_include_field_paths_and_hash_context() {
        let describe = good_describe();
        let describe_bytes = encode_describe(&describe);
        let context = describe_hash_context(&describe, &describe_bytes);

        let mut issues = Vec::new();
        let invalid_config = json!({ "enabled": "true" });
        validate_json_value(&describe.config_schema, &invalid_config, "$", &mut issues);
        assert!(
            !issues.is_empty(),
            "expected at least one schema validation issue"
        );

        let rendered = format_validation_issues(&issues);
        assert!(
            rendered.contains("$/enabled"),
            "issues should include field path"
        );
        assert!(
            rendered.contains("expected boolean"),
            "issues should include type mismatch message"
        );
        assert!(
            context.contains("describe_hash="),
            "context should include describe hash"
        );
        assert!(
            context.contains("schema_hashes=[run="),
            "context should include operation schema hash"
        );
    }

    #[test]
    fn non_map_config_reports_object_error_with_hash_context() {
        let describe = good_describe();
        let describe_bytes = encode_describe(&describe);
        let context = describe_hash_context(&describe, &describe_bytes);

        let mut issues = Vec::new();
        let non_map = json!(42);
        validate_json_value(&describe.config_schema, &non_map, "$", &mut issues);

        let rendered = format_validation_issues(&issues);
        assert!(
            rendered.contains("$: expected object"),
            "non-map config should be rejected with object error"
        );
        let combined = format!(
            "apply-answers(update) violates config_schema: {}; {}",
            rendered, context
        );
        assert!(combined.contains("describe_hash="));
        assert!(combined.contains("schema_hashes=[run="));
    }
}
