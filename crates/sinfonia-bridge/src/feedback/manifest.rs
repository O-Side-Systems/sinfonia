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

use crate::config::HarnessManifestSection;
use crate::github::GhOps;
use serde::Deserialize;
use serde_json::Value;
use std::io::{Cursor, Read};
use tracing::{debug, warn};

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

/// Match a name against a glob supporting `*` wildcards (each `*` matches
/// any run of characters). Anchored at both ends; no other metacharacters
/// are special. Sufficient for the `bridge-*` artifact convention without
/// pulling in a glob crate on this hostile-input path.
fn glob_match(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        // No wildcard — exact match.
        return pattern == name;
    }
    let mut rest = name;
    // First segment must be a literal prefix.
    let first = parts[0];
    let Some(stripped) = rest.strip_prefix(first) else {
        return false;
    };
    rest = stripped;
    // Interior segments must appear in order.
    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() {
            continue;
        }
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    // Final segment must be a literal suffix of what remains.
    let last = parts[parts.len() - 1];
    rest.ends_with(last)
}

/// Fetch, unzip, and parse the harness `bridge.json` for a red workflow
/// run. Returns `Some(Manifest)` only on a clean, version-accepted parse;
/// every miss (no artifact, oversize, zip-bomb, missing entry, malformed,
/// version-too-old) logs at `warn`/`debug` and returns `None` so the
/// caller degrades to the check-name path. Best-effort by contract
/// (Proposal 0001 §4.2 / §4.4).
pub async fn try_fetch_manifest(
    gh: &dyn GhOps,
    repo: &str,
    run_id: u64,
    cfg: &HarnessManifestSection,
) -> Option<Manifest> {
    // 1. List the run's artifacts and pick the first matching the glob.
    let artifacts = match gh.list_run_artifacts(repo, run_id).await {
        Ok(a) => a,
        Err(e) => {
            warn!(target: "feedback", error = %e, repo, run_id, "list_run_artifacts failed; falling back to check names");
            return None;
        }
    };
    let Some(artifact) = artifacts
        .into_iter()
        .find(|a| glob_match(&cfg.artifact_glob, &a.name))
    else {
        debug!(target: "feedback", repo, run_id, glob = %cfg.artifact_glob, "no run artifact matched the manifest glob; using check names");
        return None;
    };

    // Pre-download cap on the declared size (cheap rejection before any
    // bytes move). The download itself re-checks (§5, defense in depth).
    if artifact.size_in_bytes > cfg.max_artifact_bytes {
        warn!(
            target: "feedback",
            repo, run_id,
            artifact = %artifact.name,
            size = artifact.size_in_bytes,
            cap = cfg.max_artifact_bytes,
            "manifest artifact exceeds max_artifact_bytes; falling back"
        );
        return None;
    }

    // 2. Download the artifact zip, capped.
    let zip_bytes = match gh
        .download_artifact(repo, artifact.id, cfg.max_artifact_bytes)
        .await
    {
        Ok(b) => b,
        Err(e) => {
            warn!(target: "feedback", error = %e, repo, run_id, artifact = %artifact.name, "download_artifact failed/over cap; falling back");
            return None;
        }
    };

    // 3. Extract the manifest entry in memory, enforcing a per-entry
    //    decompressed cap (zip-bomb defense) reusing max_artifact_bytes:
    //    a decompressed manifest larger than the whole download budget is
    //    abusive by definition.
    let manifest_bytes = match extract_entry(&zip_bytes, &cfg.filename, cfg.max_artifact_bytes) {
        Ok(b) => b,
        Err(reason) => {
            warn!(target: "feedback", repo, run_id, artifact = %artifact.name, %reason, "manifest extraction failed; falling back");
            return None;
        }
    };

    // 4. Parse + version-gate.
    match parse_manifest(&manifest_bytes, cfg.max_failures_parsed) {
        ManifestOutcome::Ingest(m) => Some(m),
        ManifestOutcome::IngestForward(m) => {
            let max_supported = SUPPORTED_BRIDGE_MANIFEST_VERSIONS
                .iter()
                .copied()
                .max()
                .unwrap_or(0);
            warn!(
                target: "feedback",
                repo, run_id,
                schema_version = m.schema_version,
                max_supported,
                "bridge.json schema_version is newer than supported; ingesting known fields best-effort"
            );
            Some(m)
        }
        ManifestOutcome::Fallback(reason) => {
            warn!(target: "feedback", repo, run_id, %reason, "bridge.json rejected; falling back to check names");
            None
        }
    }
}

/// Read a single entry out of an in-memory zip, bounding the decompressed
/// size at `max_decompressed`. Returns `Err(reason)` for a malformed zip,
/// a missing entry, or an entry that exceeds the cap (zip bomb).
fn extract_entry(zip_bytes: &[u8], entry_name: &str, max_decompressed: u64) -> Result<Vec<u8>, String> {
    let reader = Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("not a valid zip: {e}"))?;
    let entry = match archive.by_name(entry_name) {
        Ok(f) => f,
        Err(_) => return Err(format!("zip has no '{entry_name}' entry")),
    };
    // Read at most cap+1 bytes so we can distinguish "exactly at cap" from
    // "over cap" without trusting the entry's advertised size.
    let mut limited = entry.take(max_decompressed.saturating_add(1));
    let mut buf = Vec::new();
    limited
        .read_to_end(&mut buf)
        .map_err(|e| format!("decompress failed: {e}"))?;
    if buf.len() as u64 > max_decompressed {
        return Err(format!(
            "'{entry_name}' decompresses beyond the {max_decompressed}-byte cap (possible zip bomb)"
        ));
    }
    Ok(buf)
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

    // -- glob_match -----------------------------------------------------

    #[test]
    fn glob_match_handles_prefix_wildcard() {
        assert!(glob_match("bridge-*", "bridge-1820934"));
        assert!(glob_match("bridge-*", "bridge-"));
        assert!(!glob_match("bridge-*", "coverage"));
        assert!(!glob_match("bridge-*", "x-bridge-1"));
    }

    #[test]
    fn glob_match_exact_and_interior() {
        assert!(glob_match("bridge.json", "bridge.json"));
        assert!(!glob_match("bridge.json", "bridge.json.bak"));
        assert!(glob_match("*-runs-*", "harness-runs-1"));
        assert!(glob_match("a*c", "abc"));
        assert!(!glob_match("a*c", "abd"));
    }

    // -- try_fetch_manifest pipeline ------------------------------------

    use crate::github::{ArtifactMeta, CheckRunSummary};
    use crate::Result;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::io::Write;

    /// Build an in-memory zip from `(entry_name, contents)` pairs using
    /// Deflated compression (so the zip-bomb fixture stays tiny on disk).
    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, data) in entries {
                w.start_file(*name, opts).expect("start_file");
                w.write_all(data).expect("write entry");
            }
            w.finish().expect("finish zip");
        }
        buf
    }

    /// Scriptable GhOps fake: returns a fixed artifact listing and serves
    /// per-id zip bytes, enforcing the download cap exactly as production.
    struct MockGh {
        artifacts: Vec<ArtifactMeta>,
        zips: HashMap<u64, Vec<u8>>,
    }

    #[async_trait]
    impl GhOps for MockGh {
        async fn ensure_label(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        async fn apply_label_to_pr(&self, _: &str, _: u64, _: &str) -> Result<()> {
            Ok(())
        }
        async fn remove_label_from_pr(&self, _: &str, _: u64, _: &str) -> Result<()> {
            Ok(())
        }
        async fn post_pr_comment(&self, _: &str, _: u64, _: &str) -> Result<()> {
            Ok(())
        }
        async fn list_check_run_summary(&self, _: &str, _: &str) -> Result<CheckRunSummary> {
            Ok(CheckRunSummary::default())
        }
        async fn whoami(&self) -> Result<String> {
            Ok("mock".into())
        }
        async fn list_run_artifacts(&self, _: &str, _: u64) -> Result<Vec<ArtifactMeta>> {
            Ok(self.artifacts.clone())
        }
        async fn download_artifact(
            &self,
            _: &str,
            artifact_id: u64,
            max_bytes: u64,
        ) -> Result<Vec<u8>> {
            let bytes = self
                .zips
                .get(&artifact_id)
                .cloned()
                .unwrap_or_default();
            if bytes.len() as u64 > max_bytes {
                return Err(crate::Error::GitHub("over cap".into()));
            }
            Ok(bytes)
        }
    }

    fn art(id: u64, name: &str, size: u64) -> ArtifactMeta {
        ArtifactMeta {
            id,
            name: name.to_string(),
            size_in_bytes: size,
        }
    }

    fn default_cfg() -> HarnessManifestSection {
        HarnessManifestSection {
            ingest: true,
            ..HarnessManifestSection::default()
        }
    }

    #[tokio::test]
    async fn fetch_happy_path_returns_manifest() {
        let manifest_json = v2_fixture(2);
        let zip = make_zip(&[("bridge.json", &manifest_json)]);
        let gh = MockGh {
            artifacts: vec![art(11, "bridge-1820934", zip.len() as u64)],
            zips: HashMap::from([(11, zip)]),
        };
        let m = try_fetch_manifest(&gh, "acme/widgets", 1820934, &default_cfg())
            .await
            .expect("expected Some(manifest)");
        assert_eq!(m.schema_version, 2);
        assert_eq!(m.failures.len(), 2);
    }

    #[tokio::test]
    async fn fetch_no_matching_artifact_is_none() {
        let gh = MockGh {
            artifacts: vec![art(11, "coverage", 100), art(12, "logs", 100)],
            zips: HashMap::new(),
        };
        assert!(try_fetch_manifest(&gh, "acme/widgets", 1, &default_cfg())
            .await
            .is_none());
    }

    #[tokio::test]
    async fn fetch_oversized_declared_size_is_none() {
        // Declared size over cap → rejected before any download.
        let cfg = HarnessManifestSection {
            max_artifact_bytes: 1024,
            ..default_cfg()
        };
        let gh = MockGh {
            artifacts: vec![art(11, "bridge-x", 5000)],
            zips: HashMap::new(),
        };
        assert!(try_fetch_manifest(&gh, "acme/widgets", 1, &cfg)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn fetch_zip_bomb_entry_is_none() {
        // Tiny compressed, huge decompressed: passes the download cap but
        // trips the per-entry decompressed cap.
        let cfg = HarnessManifestSection {
            max_artifact_bytes: 4096,
            ..default_cfg()
        };
        let bomb = vec![b'A'; 1_000_000]; // ~1 MB of one byte → deflates tiny
        let zip = make_zip(&[("bridge.json", &bomb)]);
        assert!(
            (zip.len() as u64) <= cfg.max_artifact_bytes,
            "compressed bomb must fit under the download cap to exercise the entry cap"
        );
        let gh = MockGh {
            artifacts: vec![art(11, "bridge-x", zip.len() as u64)],
            zips: HashMap::from([(11, zip)]),
        };
        assert!(try_fetch_manifest(&gh, "acme/widgets", 1, &cfg)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn fetch_missing_entry_is_none() {
        let zip = make_zip(&[("not-bridge.json", b"{}")]);
        let gh = MockGh {
            artifacts: vec![art(11, "bridge-x", zip.len() as u64)],
            zips: HashMap::from([(11, zip)]),
        };
        assert!(try_fetch_manifest(&gh, "acme/widgets", 1, &default_cfg())
            .await
            .is_none());
    }

    #[tokio::test]
    async fn fetch_malformed_manifest_is_none() {
        let zip = make_zip(&[("bridge.json", b"{ not json")]);
        let gh = MockGh {
            artifacts: vec![art(11, "bridge-x", zip.len() as u64)],
            zips: HashMap::from([(11, zip)]),
        };
        assert!(try_fetch_manifest(&gh, "acme/widgets", 1, &default_cfg())
            .await
            .is_none());
    }
}
