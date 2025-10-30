use std::path::{Path, PathBuf};

use crate::StoreError;

pub fn list(root: &Path) -> Result<Vec<PathBuf>, StoreError> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            entries.push(path);
        }
    }
    entries.sort();
    Ok(entries)
}

pub fn fetch(path: &Path) -> Result<Vec<u8>, StoreError> {
    Ok(std::fs::read(path)?)
}
