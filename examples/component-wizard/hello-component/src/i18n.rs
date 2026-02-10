pub const I18N_KEYS: &[&str] = &[
    "component.title",
    "component.description",
    "qa.prompt.example",
];

pub fn all_keys() -> &'static [&'static str] {
    I18N_KEYS
}

pub fn contains(key: &str) -> bool {
    I18N_KEYS.iter().any(|value| value == &key)
}
