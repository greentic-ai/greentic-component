use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use greentic_component_store::ComponentStore;
use greentic_component_store::VerificationPolicy;

#[derive(Debug, Clone)]
pub struct HostPolicy {
    pub allow_http_fetch: bool,
    pub allow_telemetry: bool,
    pub allow_state_read: bool,
    pub allow_state_write: bool,
    pub allow_state_delete: bool,
    pub state_store: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl Default for HostPolicy {
    fn default() -> Self {
        Self {
            allow_http_fetch: false,
            allow_telemetry: true,
            allow_state_read: false,
            allow_state_write: false,
            allow_state_delete: false,
            state_store: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadPolicy {
    pub store: Arc<ComponentStore>,
    pub verification: VerificationPolicy,
    pub host: HostPolicy,
}

impl LoadPolicy {
    pub fn new(store: Arc<ComponentStore>) -> Self {
        Self {
            store,
            verification: VerificationPolicy::default(),
            host: HostPolicy::default(),
        }
    }

    pub fn with_verification(mut self, policy: VerificationPolicy) -> Self {
        self.verification = policy;
        self
    }

    pub fn with_host_policy(mut self, host: HostPolicy) -> Self {
        self.host = host;
        self
    }
}
