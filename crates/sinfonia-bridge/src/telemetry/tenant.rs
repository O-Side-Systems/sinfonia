//! Tenant resolution (plan §3.2). Mirror of the daemon's
//! `sinfonia::telemetry::tenant`. Resolution precedence:
//!
//! 1. `telemetry.tenant_id` from BRIDGE.md (literal or `$ENV_VAR`).
//! 2. `SINFONIA_TENANT_ID` environment variable.
//! 3. The literal string `"default"`.
//!
//! Kept as a per-crate copy rather than sharing a utility crate because
//! the resolver is ten lines and a shared crate would have cost more to
//! maintain than it saved. Same name + behavior as the daemon's resolver
//! so an operator setting `SINFONIA_TENANT_ID` once gets both binaries.

use std::sync::Arc;

pub const TENANT_ENV_VAR: &str = "SINFONIA_TENANT_ID";
pub const DEFAULT_TENANT: &str = "default";

#[derive(Debug, Clone)]
pub struct TenantId(Arc<str>);

impl TenantId {
    pub fn resolve(from_config: Option<&str>) -> Self {
        if let Some(s) = from_config {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Self(Arc::<str>::from(trimmed));
            }
        }
        if let Ok(env) = std::env::var(TENANT_ENV_VAR) {
            let trimmed = env.trim();
            if !trimmed.is_empty() {
                return Self(Arc::<str>::from(trimmed));
            }
        }
        Self(Arc::<str>::from(DEFAULT_TENANT))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn config_value_wins() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(TENANT_ENV_VAR);
        let id = TenantId::resolve(Some("kyros-web-app"));
        assert_eq!(id.as_str(), "kyros-web-app");
    }

    #[test]
    fn env_var_used_when_config_empty() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var(TENANT_ENV_VAR, "tenant-from-env");
        let id = TenantId::resolve(None);
        assert_eq!(id.as_str(), "tenant-from-env");
        std::env::remove_var(TENANT_ENV_VAR);
    }

    #[test]
    fn default_when_nothing_set() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(TENANT_ENV_VAR);
        let id = TenantId::resolve(None);
        assert_eq!(id.as_str(), DEFAULT_TENANT);
    }
}
