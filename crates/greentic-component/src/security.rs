use std::collections::HashSet;

use crate::capabilities::{
    Capabilities, CapabilityError, FsCaps, HttpCaps, KvCaps, NetCaps, SecretsCaps, ToolsCaps,
};
use crate::manifest::ComponentManifest;

#[derive(Debug, Clone, Default)]
pub struct Profile {
    pub allowed: Capabilities,
}

impl Profile {
    pub fn new(allowed: Capabilities) -> Self {
        Self { allowed }
    }
}

pub fn enforce_capabilities(
    manifest: &ComponentManifest,
    profile: Profile,
) -> Result<(), CapabilityError> {
    let requested = &manifest.capabilities;
    let allowed = &profile.allowed;

    if let Some(http) = &requested.http {
        ensure_http(http, allowed.http.as_ref())?;
    }
    if let Some(secrets) = &requested.secrets {
        ensure_secrets(secrets, allowed.secrets.as_ref())?;
    }
    if let Some(kv) = &requested.kv {
        ensure_kv(kv, allowed.kv.as_ref())?;
    }
    if let Some(fs) = &requested.fs {
        ensure_fs(fs, allowed.fs.as_ref())?;
    }
    if let Some(net) = &requested.net {
        ensure_net(net, allowed.net.as_ref())?;
    }
    if let Some(tools) = &requested.tools {
        ensure_tools(tools, allowed.tools.as_ref())?;
    }

    Ok(())
}

fn ensure_http(requested: &HttpCaps, allowed: Option<&HttpCaps>) -> Result<(), CapabilityError> {
    let policy = allowed.ok_or_else(|| {
        CapabilityError::denied(
            "http",
            "capabilities.http",
            "profile does not permit outbound HTTP",
        )
    })?;

    let allowed_domains: HashSet<_> = policy.domains.iter().collect();
    for domain in &requested.domains {
        if !allowed_domains.contains(domain) {
            return Err(CapabilityError::denied(
                "http",
                format!("capabilities.http.domains[{domain}]"),
                format!("domain `{domain}` is not allowed"),
            ));
        }
    }

    if requested.allow_insecure && !policy.allow_insecure {
        return Err(CapabilityError::denied(
            "http",
            "capabilities.http.allow_insecure",
            "insecure HTTP is disabled for this profile",
        ));
    }

    Ok(())
}

fn ensure_secrets(
    requested: &SecretsCaps,
    allowed: Option<&SecretsCaps>,
) -> Result<(), CapabilityError> {
    let policy = allowed.ok_or_else(|| {
        CapabilityError::denied(
            "secrets",
            "capabilities.secrets",
            "profile denies access to secrets",
        )
    })?;

    let allowed_scopes: HashSet<_> = policy.scopes.iter().collect();
    for scope in &requested.scopes {
        if !allowed_scopes.contains(scope) {
            return Err(CapabilityError::denied(
                "secrets",
                format!("capabilities.secrets.scopes[{scope}]"),
                format!("scope `{scope}` is not part of the profile"),
            ));
        }
    }
    Ok(())
}

fn ensure_kv(requested: &KvCaps, allowed: Option<&KvCaps>) -> Result<(), CapabilityError> {
    let policy = allowed.ok_or_else(|| {
        CapabilityError::denied("kv", "capabilities.kv", "profile denies kv access")
    })?;

    let allowed_buckets: HashSet<_> = policy.buckets.iter().collect();
    for bucket in &requested.buckets {
        if !allowed_buckets.contains(bucket) {
            return Err(CapabilityError::denied(
                "kv",
                format!("capabilities.kv.buckets[{bucket}]"),
                format!("bucket `{bucket}` is unavailable"),
            ));
        }
    }

    if requested.read && !policy.read {
        return Err(CapabilityError::denied(
            "kv",
            "capabilities.kv.read",
            "read access denied by profile",
        ));
    }

    if requested.write && !policy.write {
        return Err(CapabilityError::denied(
            "kv",
            "capabilities.kv.write",
            "write access denied by profile",
        ));
    }

    Ok(())
}

fn ensure_fs(requested: &FsCaps, allowed: Option<&FsCaps>) -> Result<(), CapabilityError> {
    let policy = allowed.ok_or_else(|| {
        CapabilityError::denied("fs", "capabilities.fs", "profile denies filesystem mounts")
    })?;

    let allowed_paths: HashSet<_> = policy.paths.iter().collect();
    for path in &requested.paths {
        if !allowed_paths.contains(path) {
            return Err(CapabilityError::denied(
                "fs",
                format!("capabilities.fs.paths[{path}]"),
                format!("path `{path}` is not mounted in this profile"),
            ));
        }
    }

    if !requested.read_only && policy.read_only {
        return Err(CapabilityError::denied(
            "fs",
            "capabilities.fs.read_only",
            "profile exposes filesystem as read-only",
        ));
    }

    Ok(())
}

fn ensure_net(requested: &NetCaps, allowed: Option<&NetCaps>) -> Result<(), CapabilityError> {
    let policy = allowed.ok_or_else(|| {
        CapabilityError::denied(
            "net",
            "capabilities.net",
            "profile denies outbound network access",
        )
    })?;

    if !requested.hosts.is_empty() {
        if policy.hosts.is_empty() {
            return Err(CapabilityError::denied(
                "net",
                "capabilities.net.hosts",
                "profile did not pre-authorise hosts",
            ));
        }
        let allowed_hosts: HashSet<_> = policy.hosts.iter().collect();
        for host in &requested.hosts {
            if !allowed_hosts.contains(host) {
                return Err(CapabilityError::denied(
                    "net",
                    format!("capabilities.net.hosts[{host}]"),
                    format!("host `{host}` is blocked"),
                ));
            }
        }
    }

    if requested.allow_tcp && !policy.allow_tcp {
        return Err(CapabilityError::denied(
            "net",
            "capabilities.net.allow_tcp",
            "TCP access disabled",
        ));
    }

    if requested.allow_udp && !policy.allow_udp {
        return Err(CapabilityError::denied(
            "net",
            "capabilities.net.allow_udp",
            "UDP access disabled",
        ));
    }

    Ok(())
}

fn ensure_tools(requested: &ToolsCaps, allowed: Option<&ToolsCaps>) -> Result<(), CapabilityError> {
    let policy = allowed.ok_or_else(|| {
        CapabilityError::denied(
            "tools",
            "capabilities.tools",
            "no tools allowed for this profile",
        )
    })?;

    let allowed: HashSet<_> = policy.allow.iter().collect();
    for tool in &requested.allow {
        if !allowed.contains(tool) {
            return Err(CapabilityError::denied(
                "tools",
                format!("capabilities.tools.allow[{tool}]"),
                format!("tool `{tool}` cannot be invoked"),
            ));
        }
    }

    Ok(())
}
