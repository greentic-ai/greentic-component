use once_cell::sync::Lazy;

use greentic_interfaces::component_v0_5;

static CANONICAL_COMPONENT_WORLD: Lazy<String> = Lazy::new(|| {
    let package_id = component_v0_5::PACKAGE_ID;
    let (base, version) = package_id
        .split_once('@')
        .expect("component package id must include version");
    format!("{base}/component@{version}")
});

const FALLBACK_WORLD: &str = "root:component/root";

/// Returns true when the world string is the default `root:component/root` fallback.
pub fn is_fallback_world(world: &str) -> bool {
    world == FALLBACK_WORLD
}

/// Returns the canonical component world reference emitted by the scaffolded runtime.
pub fn canonical_component_world() -> &'static str {
    &CANONICAL_COMPONENT_WORLD
}
