use std::fs as std_fs;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tracing::debug;
use percent_encoding::percent_decode_str;
use url::Url;

pub mod fs;
#[cfg(feature = "http")]
pub mod http;
pub mod oci;
pub mod verify;
pub mod warg;

pub use verify::{
    DigestAlgorithm, DigestPolicy, SignaturePolicy, VerificationError, VerificationPolicy,
    VerificationReport, VerifiedDigest, VerifiedSignature,
};

#[derive(Debug, Clone)]
pub struct ComponentStore {
    cache_root: PathBuf,
    #[cfg(feature = "http")]
    http_client: reqwest::blocking::Client,
}

impl ComponentStore {
    pub fn new(cache_root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let cache_root = cache_root.as_ref().to_path_buf();
        std_fs::create_dir_all(&cache_root)?;
        Ok(Self {
            cache_root,
            #[cfg(feature = "http")]
            http_client: http::build_client()?,
        })
    }

    pub fn with_default_cache() -> Result<Self, StoreError> {
        let default = default_cache_dir();
        Self::new(default)
    }

    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    pub fn fetch_from_str(
        &self,
        locator: &str,
        policy: &VerificationPolicy,
    ) -> Result<StoreArtifact, StoreError> {
        let locator = StoreLocator::parse(locator)?;
        self.fetch(&locator, policy)
    }

    pub fn fetch(
        &self,
        locator: &StoreLocator,
        policy: &VerificationPolicy,
    ) -> Result<StoreArtifact, StoreError> {
        if let Some(expected) = policy.digest.as_ref().and_then(|d| d.expected()) {
            let cache_path = self.cache_root.join(format!("{expected}.wasm"));
            if cache_path.exists() {
                debug!("cache hit for digest {expected}");
                let bytes = std_fs::read(&cache_path)?;
                let report = policy.verify(&bytes)?;
                return Ok(StoreArtifact {
                    locator: locator.clone(),
                    path: cache_path,
                    bytes,
                    verification: report,
                });
            }
        }

        if let Some(artifact) = self.try_fetch_cached(locator, policy)? {
            return Ok(artifact);
        }

        let bytes = self.fetch_bytes(locator)?;
        let report = policy.verify(&bytes)?;
        let digest = report
            .digest
            .clone()
            .unwrap_or_else(|| VerifiedDigest::compute(DigestAlgorithm::Sha256, &bytes));
        let cache_path = self.persist(locator, &bytes, &digest)?;
        Ok(StoreArtifact {
            locator: locator.clone(),
            path: cache_path,
            bytes,
            verification: VerificationReport {
                digest: Some(digest),
                signature: report.signature,
            },
        })
    }

    fn try_fetch_cached(
        &self,
        locator: &StoreLocator,
        policy: &VerificationPolicy,
    ) -> Result<Option<StoreArtifact>, StoreError> {
        let cache_key = self.compute_cache_key(locator);
        let cache_path = self.cache_root.join(format!("{cache_key}.wasm"));
        if !cache_path.exists() {
            return Ok(None);
        }

        let bytes = std_fs::read(&cache_path)?;
        let report = policy.verify(&bytes)?;
        let digest = report
            .digest
            .clone()
            .unwrap_or_else(|| VerifiedDigest::compute(DigestAlgorithm::Sha256, &bytes));
        let digest_path = self.persist(locator, &bytes, &digest)?;
        Ok(Some(StoreArtifact {
            locator: locator.clone(),
            path: digest_path,
            bytes,
            verification: VerificationReport {
                digest: Some(digest),
                signature: report.signature,
            },
        }))
    }

    fn fetch_bytes(&self, locator: &StoreLocator) -> Result<Vec<u8>, StoreError> {
        match locator {
            StoreLocator::Fs { path, .. } => crate::fs::fetch(path),
            StoreLocator::Http(url) => {
                #[cfg(feature = "http")]
                {
                    http::fetch(&self.http_client, url)
                }
                #[cfg(not(feature = "http"))]
                {
                    let _ = url;
                    Err(StoreError::UnsupportedScheme("http".into()))
                }
            }
            StoreLocator::Https(url) => {
                #[cfg(feature = "http")]
                {
                    http::fetch(&self.http_client, url)
                }
                #[cfg(not(feature = "http"))]
                {
                    let _ = url;
                    Err(StoreError::UnsupportedScheme("https".into()))
                }
            }
            StoreLocator::Oci(reference) => oci::fetch(reference),
            StoreLocator::Warg(reference) => warg::fetch(reference),
        }
    }

    fn persist(
        &self,
        locator: &StoreLocator,
        bytes: &[u8],
        digest: &VerifiedDigest,
    ) -> Result<PathBuf, StoreError> {
        let mut file_name = digest.value.clone();
        file_name.push_str(".wasm");
        let path = self.cache_root.join(&file_name);
        std_fs::write(&path, bytes)?;

        let locator_cache = self
            .cache_root
            .join(format!("{}.wasm", self.compute_cache_key(locator)));
        if locator_cache != path {
            if let Err(err) = std_fs::write(&locator_cache, bytes) {
                debug!("failed to update locator cache at {}: {}", locator_cache.display(), err);
            }
        }

        debug!("cached artifact {:?} at {}", locator, path.display());
        Ok(path)
    }
}

fn default_cache_dir() -> PathBuf {
    std::env::temp_dir().join("greentic-component-cache")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreLocator {
    Fs { path: PathBuf, locator: String },
    Http(Url),
    Https(Url),
    Oci(String),
    Warg(String),
}

impl StoreLocator {
    pub fn parse(raw: &str) -> Result<Self, StoreError> {
        if raw.contains("://") {
            let url = Url::parse(raw).map_err(|err| StoreError::InvalidLocator {
                locator: raw.to_string(),
                reason: err.to_string(),
            })?;
            match url.scheme() {
                "fs" => {
                    let path = decode_fs_path(&url)?;
                    Ok(StoreLocator::Fs {
                        path,
                        locator: raw.to_string(),
                    })
                }
                "file" => {
                    let path = url
                        .to_file_path()
                        .map_err(|_| StoreError::InvalidLocator {
                            locator: raw.to_string(),
                            reason: "unable to convert file URL to path".into(),
                        })?;
                    Ok(StoreLocator::Fs {
                        path,
                        locator: raw.to_string(),
                    })
                }
                "http" => Ok(StoreLocator::Http(url)),
                "https" => Ok(StoreLocator::Https(url)),
                "oci" => Ok(StoreLocator::Oci(url.to_string())),
                "warg" => Ok(StoreLocator::Warg(url.to_string())),
                other => Err(StoreError::UnsupportedScheme(other.to_string())),
            }
        } else {
            let path = PathBuf::from(raw);
            let path = canonicalize_or(path);
            Ok(StoreLocator::Fs {
                path,
                locator: raw.to_string(),
            })
        }
    }

    pub fn as_cache_key(&self) -> String {
        match self {
            StoreLocator::Fs { locator, .. } => locator.clone(),
            StoreLocator::Http(url) | StoreLocator::Https(url) => url.as_str().to_string(),
            StoreLocator::Oci(reference) | StoreLocator::Warg(reference) => reference.clone(),
        }
    }
}

#[derive(Debug)]
pub struct StoreArtifact {
    pub locator: StoreLocator,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub verification: VerificationReport,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("invalid locator `{locator}`: {reason}")]
    InvalidLocator { locator: String, reason: String },
    #[error("unsupported locator scheme `{0}`")]
    UnsupportedScheme(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[cfg(feature = "http")]
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Verification(#[from] VerificationError),
}

fn decode_fs_path(url: &Url) -> Result<PathBuf, StoreError> {
    let mut path = String::new();
    if let Some(host) = url.host_str() {
        if !host.is_empty() {
            path.push_str("//");
            path.push_str(host);
        }
    }
    path.push_str(url.path());
    let decoded = percent_decode_str(&path)
        .decode_utf8()
        .map_err(|err: std::str::Utf8Error| StoreError::InvalidLocator {
            locator: url.to_string(),
            reason: err.to_string(),
        })?;
    let buf = PathBuf::from(decoded.as_ref());
    Ok(canonicalize_or(buf))
}

fn canonicalize_or(path: PathBuf) -> PathBuf {
    std_fs::canonicalize(&path).unwrap_or(path)
}

fn hash_locator(locator: &StoreLocator) -> String {
    let mut hasher = Sha256::new();
    hasher.update(locator.as_cache_key().as_bytes());
    hex::encode(hasher.finalize())
}

impl ComponentStore {
    fn compute_cache_key(&self, locator: &StoreLocator) -> String {
        hash_locator(locator)
    }
}
