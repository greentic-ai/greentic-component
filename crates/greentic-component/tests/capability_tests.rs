use std::fs;
use std::path::Path;

use greentic_component::capabilities::CapabilityErrorKind;
use greentic_component::manifest::parse_manifest;
use greentic_component::security::{Profile, enforce_capabilities};

fn manifest() -> greentic_component::manifest::ComponentManifest {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/manifests/valid.component.json");
    let raw = fs::read_to_string(path).unwrap();
    parse_manifest(&raw).unwrap()
}

#[test]
fn profile_denies_missing_capability() {
    let manifest = manifest();
    let profile = Profile::default();
    let err = enforce_capabilities(&manifest, profile).expect_err("profile must deny http");
    assert_eq!(err.capability, "http");
    assert_eq!(err.path, "capabilities.http");
    assert_eq!(err.kind, CapabilityErrorKind::Denied);
}

#[test]
fn profile_allows_whitelisted_capabilities() {
    let manifest = manifest();
    let profile = Profile::new(manifest.capabilities.clone());
    enforce_capabilities(&manifest, profile).expect("profile should allow matching capabilities");
}
