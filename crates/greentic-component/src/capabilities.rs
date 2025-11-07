use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Capabilities {
    #[serde(default)]
    pub http: Option<HttpCaps>,
    #[serde(default)]
    pub secrets: Option<SecretsCaps>,
    #[serde(default)]
    pub kv: Option<KvCaps>,
    #[serde(default)]
    pub fs: Option<FsCaps>,
    #[serde(default)]
    pub net: Option<NetCaps>,
    #[serde(default)]
    pub tools: Option<ToolsCaps>,
}

impl Capabilities {
    pub fn is_empty(&self) -> bool {
        self.http.is_none()
            && self.secrets.is_none()
            && self.kv.is_none()
            && self.fs.is_none()
            && self.net.is_none()
            && self.tools.is_none()
    }

    pub fn validate(&self) -> Result<(), CapabilityError> {
        if let Some(http) = &self.http {
            http.validate("capabilities.http")?;
        }
        if let Some(secrets) = &self.secrets {
            secrets.validate("capabilities.secrets")?;
        }
        if let Some(kv) = &self.kv {
            kv.validate("capabilities.kv")?;
        }
        if let Some(fs) = &self.fs {
            fs.validate("capabilities.fs")?;
        }
        if let Some(net) = &self.net {
            net.validate("capabilities.net")?;
        }
        if let Some(tools) = &self.tools {
            tools.validate("capabilities.tools")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpCaps {
    pub domains: Vec<String>,
    #[serde(default)]
    pub allow_insecure: bool,
}

impl HttpCaps {
    fn validate(&self, path: &str) -> Result<(), CapabilityError> {
        if self.domains.is_empty() {
            return Err(CapabilityError::invalid(
                "http",
                format!("{path}.domains"),
                "domains cannot be empty",
            ));
        }
        for domain in &self.domains {
            if !DOMAIN_RE.is_match(domain) {
                return Err(CapabilityError::invalid(
                    "http",
                    format!("{path}.domains[{domain}]"),
                    "domain must be alphanumeric with dots/dashes",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretsCaps {
    pub scopes: Vec<String>,
}

impl SecretsCaps {
    fn validate(&self, path: &str) -> Result<(), CapabilityError> {
        if self.scopes.is_empty() {
            return Err(CapabilityError::invalid(
                "secrets",
                format!("{path}.scopes"),
                "at least one scope is required",
            ));
        }
        for scope in &self.scopes {
            if scope.trim().is_empty() {
                return Err(CapabilityError::invalid(
                    "secrets",
                    format!("{path}.scopes"),
                    "scopes cannot be blank",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KvCaps {
    pub buckets: Vec<String>,
    #[serde(default = "default_true")]
    pub read: bool,
    #[serde(default)]
    pub write: bool,
}

impl KvCaps {
    fn validate(&self, path: &str) -> Result<(), CapabilityError> {
        if self.buckets.is_empty() {
            return Err(CapabilityError::invalid(
                "kv",
                format!("{path}.buckets"),
                "at least one bucket is required",
            ));
        }
        for bucket in &self.buckets {
            if !BUCKET_RE.is_match(bucket) {
                return Err(CapabilityError::invalid(
                    "kv",
                    format!("{path}.buckets[{bucket}]"),
                    "bucket names must be lowercase alphanumeric or dash",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FsCaps {
    pub paths: Vec<String>,
    #[serde(default = "default_true")]
    pub read_only: bool,
}

impl FsCaps {
    fn validate(&self, path: &str) -> Result<(), CapabilityError> {
        if self.paths.is_empty() {
            return Err(CapabilityError::invalid(
                "fs",
                format!("{path}.paths"),
                "at least one path is required",
            ));
        }
        for p in &self.paths {
            if p.trim().is_empty() {
                return Err(CapabilityError::invalid(
                    "fs",
                    format!("{path}.paths"),
                    "paths cannot be blank",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetCaps {
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default = "default_true")]
    pub allow_tcp: bool,
    #[serde(default)]
    pub allow_udp: bool,
}

impl NetCaps {
    fn validate(&self, path: &str) -> Result<(), CapabilityError> {
        for host in &self.hosts {
            if host.trim().is_empty() {
                return Err(CapabilityError::invalid(
                    "net",
                    format!("{path}.hosts"),
                    "hosts cannot be blank",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolsCaps {
    #[serde(default)]
    pub allow: Vec<String>,
}

impl ToolsCaps {
    fn validate(&self, path: &str) -> Result<(), CapabilityError> {
        for tool in &self.allow {
            if tool.trim().is_empty() {
                return Err(CapabilityError::invalid(
                    "tools",
                    format!("{path}.allow"),
                    "tool names cannot be blank",
                ));
            }
        }
        Ok(())
    }
}

fn default_true() -> bool {
    true
}

static DOMAIN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[A-Za-z0-9.-]+$").expect("http domain regex compile should never fail")
});

static BUCKET_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z0-9-]+$").expect("bucket regex compile should never fail"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityErrorKind {
    Invalid,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityError {
    pub capability: &'static str,
    pub path: String,
    pub kind: CapabilityErrorKind,
    pub message: String,
}

impl CapabilityError {
    pub fn invalid(
        capability: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            capability,
            path: path.into(),
            kind: CapabilityErrorKind::Invalid,
            message: message.into(),
        }
    }

    pub fn denied(
        capability: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            capability,
            path: path.into(),
            kind: CapabilityErrorKind::Denied,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} capability `{}` at `{}`: {}",
            self.kind, self.capability, self.path, self.message
        )
    }
}

impl std::error::Error for CapabilityError {}
