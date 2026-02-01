use std::collections::HashMap;
use std::sync::Mutex;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use greentic_types::TenantCtx;
use serde::Serialize;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct ScopedKey {
    env: String,
    tenant: String,
    team: Option<String>,
    user: Option<String>,
    prefix: String,
    key: String,
}

#[derive(Clone, Debug)]
pub struct StateScope {
    pub env: String,
    pub tenant: String,
    pub team: Option<String>,
    pub user: Option<String>,
    pub prefix: String,
}

impl StateScope {
    pub fn from_tenant_ctx(tenant: &TenantCtx, prefix: String) -> Self {
        Self {
            env: tenant.env.as_str().to_string(),
            tenant: tenant.tenant.as_str().to_string(),
            team: tenant.team.as_ref().map(|t| t.as_str().to_string()),
            user: tenant.user.as_ref().map(|u| u.as_str().to_string()),
            prefix,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct StateDumpEntry {
    pub env: String,
    pub tenant: String,
    pub team: Option<String>,
    pub user_present: bool,
    pub prefix: String,
    pub key: String,
    pub value_base64: String,
}

#[derive(Debug)]
pub struct InMemoryStateStore {
    entries: Mutex<HashMap<ScopedKey, Vec<u8>>>,
}

impl InMemoryStateStore {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn read(&self, scope: &StateScope, key: &str) -> Option<Vec<u8>> {
        let guard = self.entries.lock().expect("state store mutex poisoned");
        guard.get(&self.scoped_key(scope, key)).cloned()
    }

    pub fn write(&self, scope: &StateScope, key: &str, bytes: Vec<u8>) {
        let mut guard = self.entries.lock().expect("state store mutex poisoned");
        guard.insert(self.scoped_key(scope, key), bytes);
    }

    pub fn delete(&self, scope: &StateScope, key: &str) -> bool {
        let mut guard = self.entries.lock().expect("state store mutex poisoned");
        guard.remove(&self.scoped_key(scope, key)).is_some()
    }

    pub fn dump(&self) -> Vec<StateDumpEntry> {
        let guard = self.entries.lock().expect("state store mutex poisoned");
        guard
            .iter()
            .map(|(key, value)| StateDumpEntry {
                env: key.env.clone(),
                tenant: key.tenant.clone(),
                team: key.team.clone(),
                user_present: key.user.is_some(),
                prefix: key.prefix.clone(),
                key: key.key.clone(),
                value_base64: BASE64_STANDARD.encode(value),
            })
            .collect()
    }

    fn scoped_key(&self, scope: &StateScope, key: &str) -> ScopedKey {
        ScopedKey {
            env: scope.env.clone(),
            tenant: scope.tenant.clone(),
            team: scope.team.clone(),
            user: scope.user.clone(),
            prefix: scope.prefix.clone(),
            key: key.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_types::{EnvId, TeamId, TenantCtx, TenantId, UserId};

    fn tenant_ctx(env: &str, tenant: &str, team: Option<&str>, user: Option<&str>) -> TenantCtx {
        let env: EnvId = env.try_into().unwrap();
        let tenant: TenantId = tenant.try_into().unwrap();
        let mut ctx = TenantCtx::new(env, tenant);
        if let Some(team) = team {
            let team: TeamId = team.try_into().unwrap();
            ctx = ctx.with_team(Some(team));
        }
        if let Some(user) = user {
            let user: UserId = user.try_into().unwrap();
            ctx = ctx.with_user(Some(user));
        }
        ctx
    }

    #[test]
    fn roundtrip_read_write_delete() {
        let store = InMemoryStateStore::new();
        let scope =
            StateScope::from_tenant_ctx(&tenant_ctx("dev", "tenant", None, None), "test/1".into());

        assert!(store.read(&scope, "alpha").is_none());
        store.write(&scope, "alpha", b"data".to_vec());
        assert_eq!(store.read(&scope, "alpha").unwrap(), b"data");
        assert!(store.delete(&scope, "alpha"));
        assert!(store.read(&scope, "alpha").is_none());
    }

    #[test]
    fn tenant_isolation() {
        let store = InMemoryStateStore::new();
        let scope_a = StateScope::from_tenant_ctx(
            &tenant_ctx("dev", "tenant-a", None, None),
            "test/1".into(),
        );
        let scope_b = StateScope::from_tenant_ctx(
            &tenant_ctx("dev", "tenant-b", None, None),
            "test/1".into(),
        );

        store.write(&scope_a, "alpha", b"a".to_vec());
        store.write(&scope_b, "alpha", b"b".to_vec());

        assert_eq!(store.read(&scope_a, "alpha").unwrap(), b"a");
        assert_eq!(store.read(&scope_b, "alpha").unwrap(), b"b");
    }

    #[test]
    fn prefix_isolation() {
        let store = InMemoryStateStore::new();
        let ctx = tenant_ctx("dev", "tenant", Some("team"), Some("user"));
        let scope_a = StateScope::from_tenant_ctx(&ctx, "flow/a/1".into());
        let scope_b = StateScope::from_tenant_ctx(&ctx, "flow/a/2".into());

        store.write(&scope_a, "alpha", b"one".to_vec());
        store.write(&scope_b, "alpha", b"two".to_vec());

        assert_eq!(store.read(&scope_a, "alpha").unwrap(), b"one");
        assert_eq!(store.read(&scope_b, "alpha").unwrap(), b"two");
    }
}
