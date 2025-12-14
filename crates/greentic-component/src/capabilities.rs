use greentic_types::SecretKey;
pub use greentic_types::component::{
    ComponentCapabilities as Capabilities, ComponentConfigurators, ComponentProfiles,
    EnvCapabilities, EventsCapabilities, FilesystemCapabilities, FilesystemMode, FilesystemMount,
    HostCapabilities, HttpCapabilities, IaCCapabilities, MessagingCapabilities,
    SecretsCapabilities, StateCapabilities, TelemetryCapabilities, TelemetryScope,
    WasiCapabilities,
};
use std::collections::HashSet;

/// Validates a capability declaration, ensuring basic structural correctness.
pub fn validate_capabilities(caps: &Capabilities) -> Result<(), CapabilityError> {
    validate_wasi(&caps.wasi)?;
    validate_host(&caps.host)?;
    Ok(())
}

fn validate_wasi(wasi: &WasiCapabilities) -> Result<(), CapabilityError> {
    if let Some(fs) = &wasi.filesystem {
        validate_filesystem(fs)?;
    }
    if let Some(env) = &wasi.env {
        validate_env(env)?;
    }
    Ok(())
}

fn validate_filesystem(fs: &FilesystemCapabilities) -> Result<(), CapabilityError> {
    if fs.mode != FilesystemMode::None && fs.mounts.is_empty() {
        return Err(CapabilityError::invalid(
            "wasi.filesystem.mounts",
            "filesystem mounts must be declared when exposing the filesystem",
        ));
    }
    for mount in &fs.mounts {
        validate_mount(mount)?;
    }
    Ok(())
}

fn validate_mount(mount: &FilesystemMount) -> Result<(), CapabilityError> {
    if mount.name.trim().is_empty() {
        return Err(CapabilityError::invalid(
            "wasi.filesystem.mounts[].name",
            "mount name cannot be empty",
        ));
    }
    if mount.host_class.trim().is_empty() {
        return Err(CapabilityError::invalid(
            "wasi.filesystem.mounts[].host_class",
            "host_class must describe a storage class",
        ));
    }
    if mount.guest_path.trim().is_empty() {
        return Err(CapabilityError::invalid(
            "wasi.filesystem.mounts[].guest_path",
            "guest_path cannot be empty",
        ));
    }
    Ok(())
}

fn validate_env(env: &EnvCapabilities) -> Result<(), CapabilityError> {
    for var in &env.allow {
        if var.trim().is_empty() {
            return Err(CapabilityError::invalid(
                "wasi.env.allow[]",
                "environment variable names cannot be empty",
            ));
        }
    }
    Ok(())
}

fn validate_host(host: &HostCapabilities) -> Result<(), CapabilityError> {
    if let Some(secrets) = &host.secrets {
        validate_secrets(secrets)?;
    }
    if let Some(state) = &host.state
        && !state.read
        && !state.write
    {
        return Err(CapabilityError::invalid(
            "host.state",
            "state capability must enable read and/or write",
        ));
    }
    if let Some(telemetry) = &host.telemetry {
        validate_telemetry(telemetry)?;
    }
    if let Some(iac) = &host.iac {
        validate_iac(iac)?;
    }
    Ok(())
}

fn validate_secrets(secrets: &SecretsCapabilities) -> Result<(), CapabilityError> {
    let mut seen = HashSet::new();
    for requirement in &secrets.required {
        let key = requirement.key.as_str();
        if !seen.insert(key.to_string()) {
            return Err(CapabilityError::invalid(
                "host.secrets.required",
                format!("duplicate secret `{key}`"),
            ));
        }

        SecretKey::new(key)
            .map_err(|err| CapabilityError::invalid("host.secrets.required", err.to_string()))?;

        let scope = requirement.scope.as_ref().ok_or_else(|| {
            CapabilityError::invalid(
                "host.secrets.required.scope",
                "scope must include env and tenant",
            )
        })?;
        if scope.env.trim().is_empty() {
            return Err(CapabilityError::invalid(
                "host.secrets.required.scope.env",
                "scope.env must not be empty",
            ));
        }
        if scope.tenant.trim().is_empty() {
            return Err(CapabilityError::invalid(
                "host.secrets.required.scope.tenant",
                "scope.tenant must not be empty",
            ));
        }
        if let Some(team) = &scope.team
            && team.trim().is_empty()
        {
            return Err(CapabilityError::invalid(
                "host.secrets.required.scope.team",
                "scope.team must not be empty when provided",
            ));
        }

        if requirement.format.is_none() {
            return Err(CapabilityError::invalid(
                "host.secrets.required.format",
                "format must be specified",
            ));
        }
        if let Some(schema) = &requirement.schema
            && !schema.is_object()
        {
            return Err(CapabilityError::invalid(
                "host.secrets.required.schema",
                "schema must be an object when provided",
            ));
        }
    }
    Ok(())
}

fn validate_telemetry(telemetry: &TelemetryCapabilities) -> Result<(), CapabilityError> {
    // No structural validation beyond ensuring the enum is populated.
    let _ = telemetry.scope;
    Ok(())
}

fn validate_iac(iac: &IaCCapabilities) -> Result<(), CapabilityError> {
    if !iac.write_templates && !iac.execute_plans {
        return Err(CapabilityError::invalid(
            "host.iac",
            "iac capability must enable template writes and/or plan execution",
        ));
    }
    Ok(())
}

/// Error produced when capability declarations are malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityError {
    pub path: &'static str,
    pub message: String,
}

impl CapabilityError {
    pub fn invalid(path: &'static str, message: impl Into<String>) -> Self {
        Self {
            path,
            message: message.into(),
        }
    }
}

impl core::fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid capability `{}`: {}", self.path, self.message)
    }
}

impl std::error::Error for CapabilityError {}
