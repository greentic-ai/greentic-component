pub const QA_MODES: &[&str] = &["default", "setup", "upgrade", "remove"];

const DEFAULT_PREFILLED_ANSWERS_CBOR: &[u8] = &[];
const SETUP_PREFILLED_ANSWERS_CBOR: &[u8] = &[];
const UPGRADE_PREFILLED_ANSWERS_CBOR: &[u8] = &[];
const REMOVE_PREFILLED_ANSWERS_CBOR: &[u8] = &[];

pub fn qa_spec_cbor(mode: &str) -> &'static [u8] {
    match mode {
        "default" => &[],
        "setup" => &[],
        "upgrade" => &[],
        "remove" => &[],
        _ => &[],
    }
}

pub fn prefilled_answers_cbor(mode: &str) -> &'static [u8] {
    match mode {
        "default" => DEFAULT_PREFILLED_ANSWERS_CBOR,
        "setup" => SETUP_PREFILLED_ANSWERS_CBOR,
        "upgrade" => UPGRADE_PREFILLED_ANSWERS_CBOR,
        "remove" => REMOVE_PREFILLED_ANSWERS_CBOR,
        _ => &[],
    }
}

pub fn apply_answers(_mode: &str, _answers: Vec<u8>) -> Vec<u8> {
    // TODO: merge provided answers with defaults and return the resolved config.
    Vec::new()
}
