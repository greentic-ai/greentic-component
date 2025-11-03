use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(not(feature = "oci"))]
use anyhow::bail;
use anyhow::{Result, anyhow};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use self::cache::Cache;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComponentId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComponentLocator {
    Fs { path: PathBuf },
    Oci { reference: String },
}

#[derive(Clone, Debug)]
pub struct ComponentBytes {
    pub id: ComponentId,
    pub bytes: Bytes,
    pub meta: MetaInfo,
}

pub type SourceId = String;

#[derive(Clone, Debug)]
pub struct ComponentStore {
    sources: HashMap<SourceId, ComponentLocator>,
    cache: Cache,
    compat: CompatPolicy,
}

impl Default for ComponentStore {
    fn default() -> Self {
        Self::with_cache_dir(None, CompatPolicy::default())
    }
}

impl ComponentStore {
    pub fn with_cache_dir(cache_dir: Option<PathBuf>, compat: CompatPolicy) -> Self {
        Self {
            sources: HashMap::new(),
            cache: Cache::new(cache_dir),
            compat,
        }
    }

    pub fn add_fs(&mut self, id: impl Into<SourceId>, path: impl Into<PathBuf>) -> &mut Self {
        self.sources
            .insert(id.into(), ComponentLocator::Fs { path: path.into() });
        self
    }

    pub fn add_oci(&mut self, id: impl Into<SourceId>, reference: impl Into<String>) -> &mut Self {
        self.sources.insert(
            id.into(),
            ComponentLocator::Oci {
                reference: reference.into(),
            },
        );
        self
    }

    #[instrument(level = "trace", skip_all, fields(source = %source_id))]
    pub async fn get(&self, source_id: &str) -> Result<ComponentBytes> {
        let loc = self
            .sources
            .get(source_id)
            .ok_or_else(|| anyhow!("unknown source id: {source_id}"))?;

        if let Some(hit) = self.cache.try_load(loc).await? {
            compat::check(&self.compat, &hit.meta).map_err(anyhow::Error::new)?;
            return Ok(hit);
        }

        let bytes = match loc {
            ComponentLocator::Fs { path } => fs_source::fetch(path).await?,
            ComponentLocator::Oci { reference } => {
                #[cfg(feature = "oci")]
                {
                    oci_source::fetch(reference).await?
                }
                #[cfg(not(feature = "oci"))]
                {
                    bail!("OCI support disabled: enable the `oci` feature to fetch {reference}");
                }
            }
        };

        let (id, meta) = meta::compute_id_and_meta(bytes.as_ref()).await?;
        let cb = ComponentBytes { id, bytes, meta };

        compat::check(&self.compat, &cb.meta).map_err(anyhow::Error::new)?;
        self.cache.store(loc, &cb).await?;
        Ok(cb)
    }
}

mod cache;
mod compat;
mod fs_source;
mod meta;
#[cfg(feature = "oci")]
mod oci_source;

pub use compat::{CompatError, CompatPolicy};
pub use meta::MetaInfo;
