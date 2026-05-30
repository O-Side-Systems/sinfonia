//! Harness `bridge.json` manifest model + version gate (Proposal 0001).
//!
//! A conforming test harness emits, per CI run, a `bridge.json` failure
//! manifest inside a run artifact. This module owns the *consumed* shape
//! at `schema_version 2` (the reference producer's current version) and
//! the version gate that decides whether a given manifest is ingested,
//! forward-read best-effort, or rejected in favor of the check-name
//! fallback.
//!
//! Everything here treats the manifest as **untrusted input** — it may
//! originate from a fork PR's CI run. Parsing is pure data
//! deserialization (no execution, no path use); the size/count caps that
//! bound it live in the caller (the fetch pipeline) and in
//! [`parse_manifest`]'s `max_failures` argument.

use serde::Deserialize;
use serde_json::Value;

/// `bridge.json` schema versions this bridge knows how to read.
///
/// A manifest whose `schema_version` is in this set is ingested; one
/// newer than the maximum is forward-read by its known fields (manifests
/// are additive by convention); one older than the minimum, absent, or
/// unparseable falls back to the check-name path. Mirrors the warn-then-
/// degrade precedent of the cost-table freshness gate (SPEC §11.6.12).
pub const SUPPORTED_BRIDGE_MANIFEST_VERSIONS: &[u32] = &[2];

/// The parsed, version-gated `bridge.json`.
///
/// `total_failures` records the count *before* `max_failures` truncation
/// so a digest can report the true scenario count even when capped; it is
/// not part of the wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    #[serde(default)]
    pub run_url: Option<String>,
    #[serde(default)]
    pub artifact_bundle_name: Option<String>,
    #[serde(default)]
    pub failures: Vec<Failure>,
    #[serde(skip)]
    pub total_failures: usize,
}

/// One failing scenario row in `failures[]`.
///
/// `scenario` is the only required field; the producer guarantees the
/// rest are present-or-`null`. `step` / `assertion` are emitted by the
/// consumer **verbatim as scalar values**, never re-interpreted as
/// templates (Proposal 0001 §5).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Failure {
    pub scenario: String,
    #[serde(default)]
    pub feature_file: Option<String>,
    #[serde(default)]
    pub step: Option<String>,
    #[serde(default)]
    pub assertion: Option<String>,
    #[serde(default)]
    pub artifact_urls: Option<ArtifactUrls>,
}

/// Bundle-relative references to a scenario's artifacts. These are opaque
/// reference strings; the bridge **never** resolves or fetches them
/// server-side (Proposal 0001 §5) — they are passed downstream by name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct ArtifactUrls {
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub trace: Option<String>,
    #[serde(default)]
    pub video: Option<String>,
    #[serde(default)]
    pub a11y: Option<String>,
}

/// Outcome of [`parse_manifest`]: the three branches of the §4.3 gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestOutcome {
    /// `schema_version` is in [`SUPPORTED_BRIDGE_MANIFEST_VERSIONS`].
    Ingest(Manifest),
    /// `schema_version` is newer than the maximum supported. The known
    /// fields are read best-effort; the caller should `warn`. Unknown
    /// fields are ignored (additive-by-convention).
    IngestForward(Manifest),
    /// Too old, absent, or unparseable. The caller `warn`s and degrades
    /// to the check-name path. Carries a human-readable reason for the log.
    Fallback(String),
}

/// Parse and version-gate a `bridge.json` byte buffer.
///
/// `max_failures` caps how many scenarios are retained (the
/// manifest-flooding control, Proposal 0001 §5); `total_failures` still
/// reflects the pre-truncation count. Never panics on hostile input —
/// every malformed/old/absent case maps to [`ManifestOutcome::Fallback`].
pub fn parse_manifest(bytes: &[u8], max_failures: usize) -> ManifestOutcome {
    // Parse to a generic value first so a missing/invalid `schema_version`
    // produces a precise reason rather than a generic serde error.
    let value: Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(e) => return ManifestOutcome::Fallback(format!("malformed bridge.json: {e}")),
    };
    let version = match value.get("schema_version").and_then(Value::as_u64) {
        Some(v) => v as u32,
        None => {
            return ManifestOutcome::Fallback(
                "bridge.json missing or non-integer schema_version".into(),
            )
        }
    };

    let mut manifest: Manifest = match serde_json::from_value(value) {
        Ok(m) => m,
        Err(e) => return ManifestOutcome::Fallback(format!("bridge.json shape mismatch: {e}")),
    };
    manifest.total_failures = manifest.failures.len();
    if manifest.failures.len() > max_failures {
        manifest.failures.truncate(max_failures);
    }

    let max_supported = SUPPORTED_BRIDGE_MANIFEST_VERSIONS
        .iter()
        .copied()
        .max()
        .unwrap_or(0);
    let min_supported = SUPPORTED_BRIDGE_MANIFEST_VERSIONS
        .iter()
        .copied()
        .min()
        .unwrap_or(0);

    if SUPPORTED_BRIDGE_MANIFEST_VERSIONS.contains(&version) {
        ManifestOutcome::Ingest(manifest)
    } else if version > max_supported {
        ManifestOutcome::IngestForward(manifest)
    } else {
        ManifestOutcome::Fallback(format!(
            "bridge.json schema_version {version} is older than the minimum supported {min_supported}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn v2_fixture(n_failures: usize) -> Vec<u8> {
        let failures: Vec<Value> = (0..n_failures)
            .map(|i| {
                json!({
                    "scenario": format!("Scenario {i}"),
                    "feature_file": "requirements/features/x.feature",
                    "step": format!("Then step {i}"),
                    "assertion": format!("Expected {i}; got something else"),
                    "artifact_urls": {
                        "result": "<dir>/result.json",
                        "trace": "<dir>/trace.zip",
                        "video": "<dir>/video.webm",
                        "a11y": "<dir>/a11y.json"
                    }
                })
            })
            .collect();
        serde_json::to_vec(&json!({
            "schema_version": 2,
            "pr_number": 42,
            "branch": "sinfonia/eng-42",
            "commit_sha": "deadbeef",
            "linear_story_id": null,
            "run_url": "https://github.com/acme/widgets/actions/runs/1820934",
            "artifact_bundle_name": "harness-runs-1820934",
            "failures": failures,
        }))
        .unwrap()
    }

    #[test]
    fn v2_parses_and_ingests() {
        let outcome = parse_manifest(&v2_fixture(2), 20);
        let m = match outcome {
            ManifestOutcome::Ingest(m) => m,
            other => panic!("expected Ingest, got {other:?}"),
        };
        assert_eq!(m.schema_version, 2);
        assert_eq!(
            m.run_url.as_deref(),
            Some("https://github.com/acme/widgets/actions/runs/1820934")
        );
        assert_eq!(m.artifact_bundle_name.as_deref(), Some("harness-runs-1820934"));
        assert_eq!(m.failures.len(), 2);
        assert_eq!(m.total_failures, 2);
        assert_eq!(m.failures[0].scenario, "Scenario 0");
        assert_eq!(
            m.failures[0].artifact_urls.as_ref().unwrap().trace.as_deref(),
            Some("<dir>/trace.zip")
        );
    }

    #[test]
    fn version_1_falls_back() {
        let bytes = serde_json::to_vec(&json!({
            "schema_version": 1,
            "failures": [],
        }))
        .unwrap();
        assert!(matches!(
            parse_manifest(&bytes, 20),
            ManifestOutcome::Fallback(ref s) if s.contains("older than the minimum supported")
        ));
    }

    #[test]
    fn version_3_forward_ingests_known_fields() {
        // A hypothetical v3 that adds an unknown top-level field and an
        // unknown per-failure field: known fields are read, unknown ignored.
        let bytes = serde_json::to_vec(&json!({
            "schema_version": 3,
            "run_url": "https://example/run",
            "some_future_field": {"nested": true},
            "failures": [
                {"scenario": "S", "step": "T", "future_per_failure": 99}
            ],
        }))
        .unwrap();
        let m = match parse_manifest(&bytes, 20) {
            ManifestOutcome::IngestForward(m) => m,
            other => panic!("expected IngestForward, got {other:?}"),
        };
        assert_eq!(m.schema_version, 3);
        assert_eq!(m.run_url.as_deref(), Some("https://example/run"));
        assert_eq!(m.failures.len(), 1);
        assert_eq!(m.failures[0].scenario, "S");
        assert_eq!(m.failures[0].step.as_deref(), Some("T"));
    }

    #[test]
    fn malformed_json_falls_back() {
        assert!(matches!(
            parse_manifest(b"{ this is not json ", 20),
            ManifestOutcome::Fallback(ref s) if s.contains("malformed bridge.json")
        ));
    }

    #[test]
    fn absent_version_falls_back() {
        let bytes = serde_json::to_vec(&json!({ "failures": [] })).unwrap();
        assert!(matches!(
            parse_manifest(&bytes, 20),
            ManifestOutcome::Fallback(ref s) if s.contains("schema_version")
        ));
    }

    #[test]
    fn failures_truncated_at_cap() {
        let outcome = parse_manifest(&v2_fixture(50), 20);
        let m = match outcome {
            ManifestOutcome::Ingest(m) => m,
            other => panic!("expected Ingest, got {other:?}"),
        };
        assert_eq!(m.failures.len(), 20, "failures capped at max_failures");
        assert_eq!(m.total_failures, 50, "total reflects pre-truncation count");
    }

    #[test]
    fn missing_scenario_field_falls_back() {
        // `scenario` is required; a failure entry without it is a shape
        // mismatch and degrades rather than panicking.
        let bytes = serde_json::to_vec(&json!({
            "schema_version": 2,
            "failures": [{"step": "no scenario key"}],
        }))
        .unwrap();
        assert!(matches!(
            parse_manifest(&bytes, 20),
            ManifestOutcome::Fallback(ref s) if s.contains("shape mismatch")
        ));
    }
}
