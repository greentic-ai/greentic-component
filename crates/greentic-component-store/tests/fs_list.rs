use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use greentic_component_store::fs as store_fs;
use greentic_component_store::{ComponentStore, DigestPolicy, VerificationPolicy};

fn write_file(dir: &tempfile::TempDir, name: &str, contents: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    let mut file = File::create(&path).expect("unable to create test file");
    file.write_all(contents).expect("write should succeed");
    path
}

#[test]
fn list_returns_files() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let file_a = write_file(&temp_dir, "a.wasm", b"a");
    let file_b = write_file(&temp_dir, "b.wasm", b"b");

    let mut files = store_fs::list(temp_dir.path()).expect("list succeeds");
    files.sort();
    assert_eq!(files, vec![file_a, file_b]);
}

#[test]
fn fetch_fs_component_and_cache() {
    let temp_dir = tempfile::tempdir().expect("component dir");
    let cache_dir = tempfile::tempdir().expect("cache dir");
    let file_path = write_file(&temp_dir, "component.wasm", b"hello world");

    let store = ComponentStore::new(cache_dir.path()).expect("store");
    let locator = format!("fs://{}", file_path.display());
    let policy = VerificationPolicy {
        digest: Some(DigestPolicy::sha256(None, false)),
        signature: None,
    };

    let artifact = store
        .fetch_from_str(&locator, &policy)
        .expect("fetch should succeed");
    assert_eq!(artifact.bytes, b"hello world");
    assert!(artifact.verification.digest.is_some());
    assert!(artifact.path.exists());

    // second fetch should hit cache without touching source file
    fs::remove_file(&file_path).expect("remove source file");
    let artifact_cached = store
        .fetch_from_str(&locator, &policy)
        .expect("cached fetch should succeed");
    assert_eq!(artifact_cached.bytes, b"hello world");
    assert!(artifact_cached.path.exists());
}
