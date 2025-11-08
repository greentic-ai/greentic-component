#![cfg(feature = "cli")]

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;
use thiserror::Error;

static NAME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z0-9]+([_-][a-z0-9]+)*$").expect("valid name regex"));

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("component name may not be empty")]
    EmptyName,
    #[error("component name must be lowercase kebab-or-snake case (got `{0}`)")]
    InvalidName(String),
    #[error("unable to determine working directory: {0}")]
    WorkingDir(#[source] io::Error),
    #[error("target path points to an existing file: {0}")]
    TargetIsFile(PathBuf),
    #[error("target directory {0} already exists and is not empty")]
    TargetDirNotEmpty(PathBuf),
    #[error("failed to inspect path {0}: {1}")]
    Io(PathBuf, #[source] io::Error),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ComponentName(String);

impl ComponentName {
    pub fn parse(value: &str) -> Result<Self, ValidationError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(ValidationError::EmptyName);
        }
        if !NAME_RE.is_match(trimmed) {
            return Err(ValidationError::InvalidName(trimmed.to_owned()));
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

pub fn is_valid_name(value: &str) -> bool {
    ComponentName::parse(value).is_ok()
}

pub fn resolve_target_path(
    name: &ComponentName,
    provided: Option<&Path>,
) -> Result<PathBuf, ValidationError> {
    let relative = provided
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(name.as_str()));
    if relative.is_absolute() {
        return Ok(relative);
    }
    let cwd = env::current_dir().map_err(ValidationError::WorkingDir)?;
    Ok(cwd.join(relative))
}

pub fn ensure_path_available(path: &Path) -> Result<(), ValidationError> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_file() {
                return Err(ValidationError::TargetIsFile(path.to_path_buf()));
            }
            let mut entries =
                fs::read_dir(path).map_err(|err| ValidationError::Io(path.to_path_buf(), err))?;
            if entries.next().is_some() {
                return Err(ValidationError::TargetDirNotEmpty(path.to_path_buf()));
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(ValidationError::Io(path.to_path_buf(), err)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::TempDir;

    #[test]
    fn rejects_invalid_names() {
        let err = ComponentName::parse("HelloWorld").unwrap_err();
        assert!(matches!(err, ValidationError::InvalidName(_)));
    }

    #[test]
    fn resolves_default_path_relative_to_cwd() {
        let name = ComponentName::parse("demo-component").unwrap();
        let path = resolve_target_path(&name, None).unwrap();
        assert!(path.ends_with("demo-component"));
    }

    #[test]
    fn detects_non_empty_directories() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("demo");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("file.txt"), "data").unwrap();
        let err = ensure_path_available(&target).unwrap_err();
        assert!(matches!(err, ValidationError::TargetDirNotEmpty(_)));
    }
}
