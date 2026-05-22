//! Tenant resolution (plan §3.2).
//!
//! Resolution precedence:
//!
//! 1. `telemetry.tenant_id` from the config file (literal or `$ENV_VAR`).
//! 2. `SINFONIA_TENANT_ID` environment variable.
//! 3. The literal string `"default"`.
//!
//! The resolved value is process-wide and shared between the resource-level
//! `service.namespace` attribute and the per-span `tenant_id` attribute. Both
//! Sinfonia and the bridge ship their own copy of this resolver — the
//! resolution rule is the same; sharing a crate to host ten lines would have
//! cost more than it saved.

use std::sync::Arc;

/// Environment variable consulted when `telemetry.tenant_id` is unset.
pub const TENANT_ENV_VAR: &str = "SINFONIA_TENANT_ID";

/// Literal fallback when neither config nor env supplies a value.
pub const DEFAULT_TENANT: &str = "default";

/// Process-wide tenant identifier. Cheaply cloneable; held by the OTel
/// resource builder and surfaced on every span attribute so a Collector
/// `routing_processor` can split per-tenant exporters without touching
/// emission code.
#[derive(Debug, Clone)]
pub struct TenantId(Arc<str>);

impl TenantId {
    /// Resolve the tenant id from a config value, falling back through the
    /// precedence chain. Non-empty strings win; empty strings are treated as
    /// "not configured" so callers can pass through the YAML literal directly.
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

    // The tests below race on a global env var, so they coordinate through
    // a process-local mutex. Without it `cargo test` can interleave the
    // env-mutating tests and flake. The TenantId resolver itself is
    // thread-safe; only the env-var setup needs serializing.
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
    fn whitespace_only_config_falls_through() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var(TENANT_ENV_VAR, "fallback-env");
        let id = TenantId::resolve(Some("   "));
        assert_eq!(id.as_str(), "fallback-env");
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
