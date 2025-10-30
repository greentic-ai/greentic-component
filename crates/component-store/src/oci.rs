use crate::StoreError;

pub fn fetch(_reference: &str) -> Result<Vec<u8>, StoreError> {
    Err(StoreError::UnsupportedScheme("oci".into()))
}
