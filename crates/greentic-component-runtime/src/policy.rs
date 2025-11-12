use std::sync::Arc;

use greentic_component_store::ComponentStore;
use greentic_component_store::VerificationPolicy;

#[derive(Debug, Clone)]
pub struct HostPolicy {
    pub allow_http_fetch: bool,
    pub allow_telemetry: bool,
}

impl Default for HostPolicy {
    fn default() -> Self {
        Self {
            allow_http_fetch: false,
            allow_telemetry: true,
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
