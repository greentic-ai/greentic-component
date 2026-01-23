use std::path::{Path, PathBuf};

/// Strip an optional `file://` URI scheme from a CLI-provided path argument.
pub fn strip_file_scheme(original: &Path) -> PathBuf {
    if let Some(path_str) = original.to_str()
        && let Some(stripped) = path_str.strip_prefix("file://")
    {
        return PathBuf::from(stripped);
    }
    original.to_path_buf()
}
