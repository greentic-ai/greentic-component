use std::collections::{HashMap, HashSet};

use greentic_interfaces_wasmtime::host_helpers::v1::secrets_store::SecretsError;

#[derive(Clone, Debug)]
pub struct InMemorySecretsStore {
    allow_secrets: bool,
    allowed: HashSet<String>,
    secrets: HashMap<String, Vec<u8>>,
}

impl InMemorySecretsStore {
    pub fn new(allow_secrets: bool, allowed: HashSet<String>) -> Self {
        Self {
            allow_secrets,
            allowed,
            secrets: HashMap::new(),
        }
    }

    pub fn with_secrets(mut self, secrets: HashMap<String, String>) -> Self {
        self.secrets = secrets
            .into_iter()
            .map(|(key, value)| (key, value.into_bytes()))
            .collect();
        self
    }

    pub fn get(
        &self,
        key: &str,
    ) -> Result<Option<wasmtime::component::__internal::Vec<u8>>, SecretsError> {
        if !self.allow_secrets {
            return Err(SecretsError::Denied);
        }
        if !self.allowed.contains(key) {
            return Err(SecretsError::InvalidKey);
        }
        match self.secrets.get(key) {
            Some(bytes) => Ok(Some(bytes.clone())),
            None => Err(SecretsError::NotFound),
        }
    }
}
