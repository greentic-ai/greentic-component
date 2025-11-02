use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::ComponentId;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetaInfo {
    pub id: ComponentId,
    pub size: u64,
    pub abi_version: String,
    pub provider_name: Option<String>,
    pub provider_version: Option<String>,
    pub capabilities: Vec<String>,
}

pub async fn compute_id_and_meta(bytes: &[u8]) -> Result<(ComponentId, MetaInfo)> {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hex::encode(hasher.finalize());
    let id = ComponentId(format!("sha256:{digest}"));

    let size = bytes.len() as u64;

    // TODO: hook into greentic-interfaces once ABI metadata extraction is stabilised.
    let meta = MetaInfo {
        id: id.clone(),
        size,
        abi_version: "greentic-abi-0".to_string(),
        provider_name: None,
        provider_version: None,
        capabilities: Vec::new(),
    };

    Ok((id, meta))
}
