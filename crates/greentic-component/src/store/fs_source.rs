use std::path::Path;

use anyhow::Result;
use bytes::Bytes;
use tokio::fs;

pub async fn fetch(path: &Path) -> Result<Bytes> {
    let data = fs::read(path).await?;
    Ok(Bytes::from(data))
}
