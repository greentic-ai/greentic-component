use once_cell::sync::Lazy;

use greentic_interfaces::component_v0_6;

static CANONICAL_COMPONENT_WORLD: Lazy<String> = Lazy::new(|| {
    let package_id = component_v0_6::PACKAGE_ID;
    let (base, version) = package_id
        .split_once('@')
        .expect("component package id must include version");
    format!("{base}/component@{version}")
});

const FALLBACK_WORLDS: &[&str] = &["root:component/root", "root:root/root"];

/// Returns true when the world string matches a fallback identifier we know about.
pub fn is_fallback_world(world: &str) -> bool {
    FALLBACK_WORLDS.contains(&world)
}

/// Returns the canonical component world reference emitted by the scaffolded runtime.
pub fn canonical_component_world() -> &'static str {
    &CANONICAL_COMPONENT_WORLD
}
