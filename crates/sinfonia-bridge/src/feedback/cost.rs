//! Cost table loading + per-session cost computation (Phase 3 §7.1).
//!
//! The cost table maps `(provider, model)` to per-million-token rates in
//! USD. It ships embedded in the binary via `include_str!` so the
//! bridge always has a default; operators override at runtime via
//! `bridge.cost_table_path` in BRIDGE.md when provider pricing changes
//! between Sinfonia releases.
//!
//! Decimal arithmetic uses `rust_decimal::Decimal` for precision — per
//! STATUS §5.1, cost values are written to the tracker as
//! `CustomFieldValue::String("8.23")`, NEVER as an f64. This module is
//! the one place we cross from token counts (u64) to dollars; the
//! `Decimal` boundary is intentional.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Default cost table baked into the binary. Loaded eagerly at startup;
/// `BridgeConfig.cost_table_path` overrides via `CostTable::load_or_default`.
pub const EMBEDDED_COST_TABLE_YAML: &str = include_str!("../../../../config/cost_table.yaml");

/// Number of days after `verified_at` past which we warn at startup.
/// Cross-cutting concern C in the milestone overview.
pub const FRESHNESS_WARN_DAYS: i64 = 90;

/// Plan-checker M-2 fix: refuse cost caps (token caps stay accepted)
/// when the table is more than this many days stale. The asymmetric gate
/// is intentional: token caps degrade safely under stale pricing data;
/// dollar caps don't.
pub const COST_CAP_BLOCK_DAYS: i64 = 180;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCost {
    pub input_per_million_usd: Decimal,
    pub output_per_million_usd: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostTable {
    pub verified_at: NaiveDate,
    pub providers: HashMap<String, HashMap<String, ModelCost>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupOutcome {
    Found(ModelCost),
    UnknownProvider,
    UnknownModel,
}

#[derive(Debug)]
pub enum CostTableError {
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
}

impl std::fmt::Display for CostTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Yaml(e) => write!(f, "yaml: {e}"),
        }
    }
}

impl std::error::Error for CostTableError {}

impl CostTable {
    /// Parse the embedded default table. Panics on a malformed bundled
    /// table — that's a build-time bug, not a runtime config error.
    pub fn embedded_default() -> Self {
        serde_yaml::from_str(EMBEDDED_COST_TABLE_YAML)
            .expect("embedded cost_table.yaml must parse — build-time invariant")
    }

    /// Load from `path` if provided; otherwise fall back to the embedded
    /// default. Returned errors describe override-path failures only;
    /// the embedded path is infallible by construction.
    pub fn load_or_default(path: Option<&Path>) -> std::result::Result<Self, CostTableError> {
        match path {
            None => Ok(Self::embedded_default()),
            Some(p) => {
                let text = std::fs::read_to_string(p).map_err(CostTableError::Io)?;
                let parsed: Self = serde_yaml::from_str(&text).map_err(CostTableError::Yaml)?;
                Ok(parsed)
            }
        }
    }

    /// Days elapsed between `verified_at` and `today`. Negative if the
    /// table is dated in the future (used by tests).
    pub fn age_days(&self, today: NaiveDate) -> i64 {
        today.signed_duration_since(self.verified_at).num_days()
    }

    pub fn is_stale_warn(&self, today: NaiveDate) -> bool {
        self.age_days(today) > FRESHNESS_WARN_DAYS
    }

    /// M-2 fix: when the table is older than `COST_CAP_BLOCK_DAYS`, the
    /// bridge keeps applying TOKEN caps but stops applying COST caps —
    /// avoids charging operators against arbitrarily-stale pricing.
    pub fn accepts_cost_caps(&self, today: NaiveDate) -> bool {
        self.age_days(today) <= COST_CAP_BLOCK_DAYS
    }

    pub fn lookup(&self, provider: &str, model: &str) -> LookupOutcome {
        let provider_lc = provider.to_lowercase();
        // Provider name is what `AgentProvider::format!("{:?}", ...)`
        // produces — normalize a few known prefixes so the table's
        // lowercase keys ("anthropic", "openai") match.
        let normalized_provider = normalize_provider(&provider_lc);
        let Some(models) = self.providers.get(&normalized_provider) else {
            return LookupOutcome::UnknownProvider;
        };
        // OpenCode-style `provider/model` model names (e.g.
        // "anthropic/claude-sonnet-4-6") should look up by the part
        // after the slash since the provider prefix is already
        // captured separately.
        let model_key = match model.split_once('/') {
            Some((_, m)) => m,
            None => model,
        };
        match models.get(model_key) {
            Some(c) => LookupOutcome::Found(c.clone()),
            None => LookupOutcome::UnknownModel,
        }
    }

    /// Per-session cost in USD given input/output token counts and the
    /// resolved model. Unknown provider/model returns `Decimal::ZERO`
    /// so the accumulator advances without writing a wrong value;
    /// callers log a WARN via `lookup()` separately.
    pub fn compute_cost(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Decimal {
        let Some(cost) = (match self.lookup(provider, model) {
            LookupOutcome::Found(c) => Some(c),
            _ => None,
        }) else {
            return Decimal::ZERO;
        };
        let per_million = Decimal::from(1_000_000u64);
        let input_cost = cost.input_per_million_usd * Decimal::from(input_tokens) / per_million;
        let output_cost = cost.output_per_million_usd * Decimal::from(output_tokens) / per_million;
        input_cost + output_cost
    }
}

/// Map a Rust-side provider identifier (whatever
/// `format!("{:?}", AgentProvider)` emits) to the cost-table key.
/// Lower-cased input. Mapping is intentionally narrow — unknown
/// provider strings fall through and `lookup()` returns
/// `UnknownProvider`.
fn normalize_provider(s: &str) -> String {
    match s {
        "claudecode" | "anthropic" => "anthropic".to_string(),
        "openai" | "openaicompatible" => "openai".to_string(),
        "google" | "gemini" => "google".to_string(),
        "ollama" | "local" => "ollama".to_string(),
        "codex" => "openai".to_string(),     // Codex routes via OpenAI
        "opencode" => "opencode".to_string(), // Look up sub-model after "/"
        other => other.to_string(),
    }
}

/// Convenience: Decimal → tracker-safe string with at most 4 decimal
/// places (STATUS §5.1: cost values write as `CustomFieldValue::String`
/// — NEVER f64). 4dp is enough precision for token-cost arithmetic at
/// current provider rates without burying the value in trailing zeros.
pub fn cost_to_string(cost: Decimal) -> String {
    let rounded = cost.round_dp(4);
    // Strip trailing zeros and the decimal point if integer-valued,
    // matching the format `8.23` rather than `8.2300`.
    let s = rounded.normalize().to_string();
    if s.contains('.') {
        s
    } else {
        // `Decimal::normalize` returns "8" for 8.0000 — keep that as-is.
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn embedded_table_parses() {
        let t = CostTable::embedded_default();
        assert!(!t.providers.is_empty());
        assert!(t.providers.contains_key("anthropic"));
    }

    #[test]
    fn lookup_finds_anthropic_models() {
        let t = CostTable::embedded_default();
        match t.lookup("Anthropic", "claude-opus-4-7") {
            LookupOutcome::Found(_) => {}
            other => panic!("expected Found, got {:?}", other),
        }
        match t.lookup("ClaudeCode", "claude-opus-4-7") {
            LookupOutcome::Found(_) => {}
            other => panic!("provider normalization broke: {:?}", other),
        }
    }

    #[test]
    fn lookup_handles_opencode_provider_slash_model() {
        // OpenCode wire format: `provider/model`. The cost table looks
        // up by the post-slash portion — the test pins this behavior
        // so a refactor doesn't silently break OpenCode cost tracking.
        let t = CostTable::embedded_default();
        // anthropic provider, opencode-style model name with slash:
        match t.lookup("anthropic", "anthropic/claude-sonnet-4-6") {
            LookupOutcome::Found(_) => {}
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn unknown_provider_and_model() {
        let t = CostTable::embedded_default();
        assert_eq!(
            t.lookup("nosuchprovider", "model"),
            LookupOutcome::UnknownProvider
        );
        assert_eq!(
            t.lookup("anthropic", "nosuchmodel"),
            LookupOutcome::UnknownModel
        );
    }

    #[test]
    fn compute_cost_matches_per_million_rate() {
        let t = CostTable::embedded_default();
        // claude-sonnet-4-6: 3 USD/Mtok input, 15 USD/Mtok output.
        // 100k input + 50k output = 0.3 + 0.75 = 1.05 USD
        let cost = t.compute_cost("anthropic", "claude-sonnet-4-6", 100_000, 50_000);
        assert_eq!(cost, dec("1.05"));
    }

    #[test]
    fn compute_cost_unknown_returns_zero() {
        let t = CostTable::embedded_default();
        let cost = t.compute_cost("nosuchprovider", "x", 100, 100);
        assert_eq!(cost, Decimal::ZERO);
    }

    #[test]
    fn local_ollama_cost_is_zero() {
        let t = CostTable::embedded_default();
        let cost = t.compute_cost("ollama", "llama3.1", 10_000, 10_000);
        assert_eq!(cost, Decimal::ZERO);
    }

    #[test]
    fn freshness_gate_warn_and_block() {
        let t = CostTable::embedded_default();
        let just_after_warn = t.verified_at + chrono::Duration::days(FRESHNESS_WARN_DAYS + 1);
        assert!(t.is_stale_warn(just_after_warn));
        assert!(t.accepts_cost_caps(just_after_warn));

        let just_after_block = t.verified_at + chrono::Duration::days(COST_CAP_BLOCK_DAYS + 1);
        assert!(!t.accepts_cost_caps(just_after_block));
        assert!(t.is_stale_warn(just_after_block));
    }

    #[test]
    fn cost_to_string_strips_trailing_zeros() {
        assert_eq!(cost_to_string(dec("8.2300")), "8.23");
        assert_eq!(cost_to_string(dec("8.0")), "8");
        assert_eq!(cost_to_string(dec("0.0001")), "0.0001");
    }
}
