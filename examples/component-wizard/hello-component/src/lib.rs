mod i18n;
mod qa;
mod schemas;

pub fn describe() -> Vec<u8> {
    schemas::describe_cbor().to_vec()
}

pub fn qa_spec(mode: &str) -> Vec<u8> {
    qa::qa_spec_cbor(mode).to_vec()
}

pub fn apply_answers(mode: &str, answers: Vec<u8>) -> Vec<u8> {
    qa::apply_answers(mode, answers)
}

pub fn run() -> Vec<u8> {
    Vec::new()
}
