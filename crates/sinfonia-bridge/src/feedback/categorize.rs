//! Pure failure categorization (plan §6).
//!
//! Given a list of failing check names and the bridge's configured
//! categories, pick the highest-priority category whose `check_pattern`
//! matches any of the failed checks. Falls back to the synthetic
//! `default` category — `config::parse_failure_categories` always
//! injects one (either user-supplied or auto-generated), so the
//! fallback is always present.

use crate::config::FailureCategory;

/// Select the best-matching category for the given failing check names.
///
/// Returns a reference into `categories` — the caller decides how to
/// clone or borrow from there.
///
/// Edge cases (per plan §6):
///
/// - **Empty `failed_checks`.** Indicates a caller bug (we shouldn't be
///   categorizing a green build). We assert in debug and fall back to
///   the default in release so we don't take the whole bridge down.
/// - **Multiple checks matching multiple categories.** The highest
///   `priority` wins. Tie-broken by config-validation (duplicate
///   priorities are rejected at parse time, except for the synthetic
///   `default` at priority 0).
/// - **No category's pattern matches any failing check.** Return the
///   `default` category.
/// - **`failure_categories:` absent from config.** Not visible at this
///   layer — `parse_failure_categories` injects a synthetic default
///   when the section is missing, so this function always receives at
///   least one category.
pub fn categorize<'a>(
    failed_checks: &[String],
    categories: &'a [FailureCategory],
) -> &'a FailureCategory {
    debug_assert!(
        !failed_checks.is_empty(),
        "categorize: failed_checks is empty (caller bug)"
    );
    debug_assert!(
        !categories.is_empty(),
        "categorize: categories slice is empty (config invariant violated)"
    );

    let mut best: Option<&FailureCategory> = None;
    for cat in categories {
        let Some(pat) = cat.check_pattern.as_ref() else {
            continue;
        };
        let hit = failed_checks.iter().any(|name| pat.is_match(name));
        if !hit {
            continue;
        }
        best = match best {
            Some(prev) if prev.priority >= cat.priority => Some(prev),
            _ => Some(cat),
        };
    }

    best.unwrap_or_else(|| default_category(categories))
}

/// Return the explicit `default` category if present, otherwise the
/// lowest-priority category (which by config invariant is the synthetic
/// default).
fn default_category(categories: &[FailureCategory]) -> &FailureCategory {
    categories
        .iter()
        .find(|c| c.name == "default")
        .unwrap_or_else(|| {
            categories
                .iter()
                .min_by_key(|c| c.priority)
                .expect("at least one category exists")
        })
}

// ---------------------------------------------------------------------------
// Tests (plan §9.1: `feedback::categorize::tests`)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    fn cat(name: &str, pattern: Option<&str>, target: &str, priority: i64) -> FailureCategory {
        FailureCategory {
            name: name.into(),
            check_pattern: pattern.map(|p| Regex::new(p).expect("test regex compiles")),
            target_state: target.into(),
            priority,
        }
    }

    /// The shape `parse_failure_categories` would produce for a config
    /// with explicit `lint`/`e2e` rules plus the auto-injected default.
    fn typical_categories() -> Vec<FailureCategory> {
        vec![
            cat("lint", Some("(?i)lint|prettier|clippy"), "Needs Fixes - Lint", 10),
            cat("e2e", Some("(?i)e2e|playwright"), "Needs Fixes - E2E", 20),
            cat("default", None, "Needs Fixes", 0),
        ]
    }

    #[test]
    fn higher_priority_wins_when_multiple_match() {
        let categories = typical_categories();
        // Both lint AND e2e fail in the same run. e2e has priority 20,
        // lint has 10 → e2e wins.
        let failed = vec!["unit-tests / lint".to_string(), "e2e / playwright".to_string()];
        let picked = categorize(&failed, &categories);
        assert_eq!(picked.name, "e2e");
        assert_eq!(picked.target_state, "Needs Fixes - E2E");
    }

    #[test]
    fn single_category_match_wins() {
        let categories = typical_categories();
        let failed = vec!["unit / clippy".to_string()];
        let picked = categorize(&failed, &categories);
        assert_eq!(picked.name, "lint");
    }

    #[test]
    fn no_match_returns_default() {
        let categories = typical_categories();
        // None of the configured patterns match this check name.
        let failed = vec!["weird-bespoke-check-no-one-configured".to_string()];
        let picked = categorize(&failed, &categories);
        assert_eq!(picked.name, "default");
        assert_eq!(picked.target_state, "Needs Fixes");
    }

    #[test]
    fn synthetic_only_default_returns_default() {
        // This mirrors what `parse_failure_categories` produces when the
        // user omits `failure_categories:` entirely.
        let categories = vec![cat("default", None, "Needs Fixes", 0)];
        let failed = vec!["any-check".to_string()];
        let picked = categorize(&failed, &categories);
        assert_eq!(picked.name, "default");
    }

    #[test]
    fn empty_failed_checks_returns_default_in_release() {
        // We're in `cfg(test)`, which compiles `debug_assertions` ON, so
        // this would panic if we exercised the canonical path. We can't
        // run the release-only branch from here — but assert structurally
        // that the function would return the default given the slice.
        // This is the documented release-mode behaviour from plan §6.
        let categories = typical_categories();
        // Use `default_category` directly to assert the fallback is
        // reachable and matches what release-mode `categorize` would
        // return.
        let picked = default_category(&categories);
        assert_eq!(picked.name, "default");
    }
}
