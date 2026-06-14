//! Harness-manifest ingestion: adversarial security suite (Proposal 0001
//! Task 7, §5 matrix).
//!
//! `bridge.json` may originate from an untrusted fork PR's CI run, so this
//! suite drives the *public* ingestion API (`try_fetch_manifest`,
//! `parse_manifest`, `build_failure_digest`) with hostile inputs and
//! asserts the bridge degrades safely — no panic, no resource blow-up, no
//! template evaluation, no server-side fetch of artifact references, and a
//! bounded digest. It is the exit criterion for flipping the default on.
//!
//! | Threat                       | Asserted control                                  |
//! |------------------------------|---------------------------------------------------|
//! | Oversized artifact           | declared-size pre-check AND actual-download cap    |
//! | Zip bomb                     | per-entry decompressed cap                         |
//! | Manifest flooding            | max_failures_parsed + digest byte bound            |
//! | Template injection           | scalar/text rendering, no Liquid evaluation        |
//! | artifact_urls server fetch   | exactly one download (the bundle), urls untouched  |
//! | Version drift                | old → fallback; newer-additive → forward-ingest    |
//! | Malformed / missing entry    | fallback, never a panic                            |

use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use serde_json::json;

use sinfonia_bridge::config::HarnessManifestSection;
use sinfonia_bridge::feedback::manifest::{
    build_failure_digest, parse_manifest, try_fetch_manifest, ArtifactUrls, Failure, Manifest,
    ManifestOutcome,
};
use sinfonia_bridge::github::{ArtifactMeta, CheckRunSummary, GhOps};
use sinfonia_bridge::{Error, Result};

// ---------------------------------------------------------------------------
// Fixtures + scriptable GhOps
// ---------------------------------------------------------------------------

/// Build an in-memory zip with Deflated compression (so a zip bomb stays
/// tiny on the wire while decompressing large).
fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }
    buf
}

fn v2_manifest_bytes(n_failures: usize) -> Vec<u8> {
    let failures: Vec<_> = (0..n_failures)
        .map(|i| {
            json!({
                "scenario": format!("Scenario {i}"),
                "feature_file": "requirements/features/x.feature",
                "step": format!("Then step {i}"),
                "assertion": format!("Expected {i}"),
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
        "run_url": "https://github.com/acme/widgets/actions/runs/1820934",
        "artifact_bundle_name": "harness-runs-1820934",
        "failures": failures,
    }))
    .unwrap()
}

/// Records every `download_artifact` call so the suite can prove the
/// bridge never reaches out for the per-scenario `artifact_urls`.
struct CountingGh {
    artifacts: Vec<ArtifactMeta>,
    zips: HashMap<u64, Vec<u8>>,
    downloads: AtomicUsize,
}

impl CountingGh {
    fn new(artifacts: Vec<ArtifactMeta>, zips: HashMap<u64, Vec<u8>>) -> Self {
        Self {
            artifacts,
            zips,
            downloads: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl GhOps for CountingGh {
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
        Ok("counting".into())
    }
    async fn list_run_artifacts(&self, _: &str, _: u64) -> Result<Vec<ArtifactMeta>> {
        Ok(self.artifacts.clone())
    }
    async fn download_artifact(&self, _: &str, id: u64, max_bytes: u64) -> Result<Vec<u8>> {
        self.downloads.fetch_add(1, Ordering::SeqCst);
        let bytes = self.zips.get(&id).cloned().unwrap_or_default();
        if bytes.len() as u64 > max_bytes {
            return Err(Error::GitHub(format!(
                "artifact {id} over the {max_bytes}-byte cap"
            )));
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

fn cfg_with(max_artifact_bytes: u64, max_failures: usize, max_digest: usize) -> HarnessManifestSection {
    HarnessManifestSection {
        ingest: true,
        artifact_glob: "bridge-*".into(),
        filename: "bridge.json".into(),
        max_artifact_bytes,
        max_failures_parsed: max_failures,
        max_failure_digest_bytes: max_digest,
    }
}

// ---------------------------------------------------------------------------
// Oversized artifact — both the declared-size pre-check and the actual
// post-download cap must reject.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn oversized_actual_download_is_rejected() {
    // Declared size LIES (under cap) but the real zip is over cap: the
    // download_artifact cap is the backstop. Pipeline → None.
    let big = make_zip(&[("bridge.json", &v2_manifest_bytes(1))]);
    let real_len = big.len() as u64;
    let cfg = cfg_with(real_len - 1, 20, 8192); // cap one byte under the real size
    let gh = CountingGh::new(
        vec![art(11, "bridge-x", real_len - 1)], // declared = cap, passes pre-check
        HashMap::from([(11, big)]),
    );
    let out = try_fetch_manifest(&gh, "acme/widgets", 1, &cfg).await;
    assert!(out.is_none(), "actual-download cap must reject the oversize zip");
}

#[tokio::test]
async fn oversized_declared_size_skips_download_entirely() {
    let cfg = cfg_with(1024, 20, 8192);
    let gh = CountingGh::new(vec![art(11, "bridge-x", 5000)], HashMap::new());
    let out = try_fetch_manifest(&gh, "acme/widgets", 1, &cfg).await;
    assert!(out.is_none());
    assert_eq!(
        gh.downloads.load(Ordering::SeqCst),
        0,
        "declared-oversize must be rejected before any download"
    );
}

// ---------------------------------------------------------------------------
// Zip bomb — small compressed, huge decompressed.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn zip_bomb_entry_is_rejected() {
    let cfg = cfg_with(4096, 20, 8192);
    let bomb = vec![b'A'; 2_000_000];
    let zip = make_zip(&[("bridge.json", &bomb)]);
    assert!(
        (zip.len() as u64) <= cfg.max_artifact_bytes,
        "compressed bomb fits under the download cap; the entry cap is what must fire"
    );
    let gh = CountingGh::new(
        vec![art(11, "bridge-x", zip.len() as u64)],
        HashMap::from([(11, zip)]),
    );
    assert!(try_fetch_manifest(&gh, "acme/widgets", 1, &cfg)
        .await
        .is_none());
}

// ---------------------------------------------------------------------------
// Manifest flooding — thousands of failures must bound both the parsed
// count and the rendered digest length.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn manifest_flooding_is_bounded() {
    let cfg = cfg_with(5_242_880, 20, 8_192);
    let zip = make_zip(&[("bridge.json", &v2_manifest_bytes(5000))]);
    let gh = CountingGh::new(
        vec![art(11, "bridge-x", zip.len() as u64)],
        HashMap::from([(11, zip)]),
    );
    let m = try_fetch_manifest(&gh, "acme/widgets", 1, &cfg)
        .await
        .expect("a flooded-but-valid manifest still ingests, just capped");
    assert_eq!(m.failures.len(), 20, "parsed failures capped at max_failures_parsed");
    assert_eq!(m.total_failures, 5000, "true count preserved for the marker");

    let digest = build_failure_digest(&m, "https://example/run", &cfg);
    assert!(
        digest.len() <= cfg.max_failure_digest_bytes,
        "digest length {} must stay within the {}-byte budget",
        digest.len(),
        cfg.max_failure_digest_bytes
    );
    assert!(digest.contains("more scenario(s) truncated)"));
}

// ---------------------------------------------------------------------------
// Template injection — Liquid + Markdown in fork-controlled fields render
// verbatim through the digest (the comment template binds it as a scalar).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn injection_renders_literally_in_digest() {
    let cfg = cfg_with(5_242_880, 20, 8_192);
    let hostile = serde_json::to_vec(&json!({
        "schema_version": 2,
        "failures": [{
            "scenario": "Injection",
            "step": "{% if true %}danger{% endif %}",
            "assertion": "{{ 7*7 }} and `code` and ```fence```",
        }],
    }))
    .unwrap();
    let zip = make_zip(&[("bridge.json", &hostile)]);
    let gh = CountingGh::new(
        vec![art(11, "bridge-x", zip.len() as u64)],
        HashMap::from([(11, zip)]),
    );
    let m = try_fetch_manifest(&gh, "acme/widgets", 1, &cfg)
        .await
        .expect("hostile-but-valid v2 manifest still parses");
    let digest = build_failure_digest(&m, "", &cfg);
    // The Liquid is carried verbatim (and not evaluated) and the fence is
    // broken so it cannot escape a Markdown code block downstream.
    assert!(digest.contains("{{ 7*7 }}"));
    assert!(!digest.contains("49"));
    assert!(digest.contains("{% if true %}danger{% endif %}"));
    assert!(!digest.contains("```"), "triple-backtick fence neutralized");
}

// ---------------------------------------------------------------------------
// artifact_urls are opaque references — the bridge MUST NOT fetch them
// server-side. Exactly one download happens: the manifest bundle.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn artifact_urls_are_never_fetched_server_side() {
    let cfg = cfg_with(5_242_880, 20, 8_192);
    let zip = make_zip(&[("bridge.json", &v2_manifest_bytes(3))]);
    let gh = CountingGh::new(
        vec![art(11, "bridge-1820934", zip.len() as u64)],
        HashMap::from([(11, zip)]),
    );
    let m = try_fetch_manifest(&gh, "acme/widgets", 1, &cfg)
        .await
        .expect("ingests");
    // The references appear in the digest as text...
    let digest = build_failure_digest(&m, "https://example/run", &cfg);
    assert!(digest.contains("trace.zip"));
    // ...but they were never resolved: only the single bundle was fetched.
    assert_eq!(
        gh.downloads.load(Ordering::SeqCst),
        1,
        "exactly one download (the bundle); artifact_urls must not be fetched"
    );
}

// ---------------------------------------------------------------------------
// Version drift.
// ---------------------------------------------------------------------------

#[test]
fn version_too_old_falls_back() {
    let bytes = serde_json::to_vec(&json!({"schema_version": 1, "failures": []})).unwrap();
    assert!(matches!(
        parse_manifest(&bytes, 20),
        ManifestOutcome::Fallback(_)
    ));
}

#[test]
fn version_newer_forward_ingests() {
    let bytes = serde_json::to_vec(&json!({
        "schema_version": 99,
        "future_field": true,
        "failures": [{"scenario": "S"}],
    }))
    .unwrap();
    assert!(matches!(
        parse_manifest(&bytes, 20),
        ManifestOutcome::IngestForward(_)
    ));
}

// ---------------------------------------------------------------------------
// Malformed inputs never panic.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn malformed_and_missing_entry_fall_back() {
    let cfg = cfg_with(5_242_880, 20, 8_192);

    // Garbage entry contents.
    let bad = make_zip(&[("bridge.json", b"\x00\x01 not json")]);
    let gh = CountingGh::new(
        vec![art(11, "bridge-x", bad.len() as u64)],
        HashMap::from([(11, bad)]),
    );
    assert!(try_fetch_manifest(&gh, "acme/widgets", 1, &cfg)
        .await
        .is_none());

    // Right artifact, wrong entry name.
    let wrong = make_zip(&[("other.json", b"{}")]);
    let gh2 = CountingGh::new(
        vec![art(12, "bridge-y", wrong.len() as u64)],
        HashMap::from([(12, wrong)]),
    );
    assert!(try_fetch_manifest(&gh2, "acme/widgets", 1, &cfg)
        .await
        .is_none());

    // Not a zip at all.
    let gh3 = CountingGh::new(
        vec![art(13, "bridge-z", 4)],
        HashMap::from([(13, b"junk".to_vec())]),
    );
    assert!(try_fetch_manifest(&gh3, "acme/widgets", 1, &cfg)
        .await
        .is_none());
}

// ---------------------------------------------------------------------------
// Golden snapshot — exact byte-for-byte digest rendering (D-03 / GAP-2).
//
// Pins the precise rendered string for all four diagnostic fields
// (scenario, feature_file, step, assertion) plus artifact references and
// the bundle suffix in the footer. A durable regression guard: any
// field-label typo, spacing change, or newline drift will cause this test
// to fail immediately (HARNESS-01 / Criterion C-1).
// ---------------------------------------------------------------------------

#[test]
fn golden_snapshot_exact_field_rendering() {
    let m = Manifest {
        schema_version: 2,
        run_url: Some("https://github.com/acme/widgets/actions/runs/1820934".into()),
        artifact_bundle_name: Some("harness-runs-1820934".into()),
        failures: vec![Failure {
            scenario: "Create tenant persists across reload".into(),
            feature_file: Some(
                "requirements/features/tenant/create-tenant.feature".into(),
            ),
            step: Some("Then the tenant list shows \"Acme\"".into()),
            assertion: Some(
                "Expected element [data-testid='tenant-row-acme'] to be visible; was not present in DOM"
                    .into(),
            ),
            artifact_urls: Some(ArtifactUrls {
                result: Some("<dir>/result.json".into()),
                trace: Some("<dir>/trace.zip".into()),
                video: Some("<dir>/video.webm".into()),
                a11y: Some("<dir>/a11y.json".into()),
            }),
        }],
        total_failures: 1,
    };
    let cfg = cfg_with(5_242_880, 20, 8_192);
    let digest = build_failure_digest(
        &m,
        "https://github.com/acme/widgets/actions/runs/1820934",
        &cfg,
    );
    // Golden snapshot: assert the EXACT rendered string — not just contains-checks.
    // Derived from manifest.rs:build_failure_digest + render_scenario_block + artifact_refs.
    let expected = concat!(
        "harness reported 1 failing scenario(s):\n",
        "\n",
        "1. \"Create tenant persists across reload\"\n",
        "   feature:   requirements/features/tenant/create-tenant.feature\n",
        "   step:      Then the tenant list shows \"Acme\"\n",
        "   assertion: Expected element [data-testid='tenant-row-acme'] to be visible; was not present in DOM\n",
        "   artifacts: <dir>/result.json · <dir>/trace.zip · <dir>/video.webm · <dir>/a11y.json\n",
        "\n",
        "(diagnostics from bridge.json schema_version=2; full artifacts at \
https://github.com/acme/widgets/actions/runs/1820934 (bundle 'harness-runs-1820934'))",
    );
    assert_eq!(digest, expected, "golden snapshot mismatch");
}

// ---------------------------------------------------------------------------
// No-disk-write proof — HARNESS-05 "in-memory parse only" (GAP-1 / C-3).
//
// Drives the full try_fetch_manifest path (download → unzip → parse) and
// asserts that no files appear in the system temp dir during ingestion.
// This is a belt-and-suspenders regression guard: the property is
// structurally true today (manifest.rs uses only Cursor<Vec<u8>>; no
// std::fs import), but this test will catch any future accidental
// introduction of a tempfile or std::fs::write call.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ingestion_writes_no_files_to_disk() {
    let cfg = cfg_with(5_242_880, 20, 8_192);
    let zip = make_zip(&[("bridge.json", &v2_manifest_bytes(1))]);
    let gh = CountingGh::new(
        vec![art(42, "bridge-noDisk", zip.len() as u64)],
        HashMap::from([(42u64, zip)]),
    );

    // Snapshot temp dir entry count BEFORE ingestion.
    let tmp = std::env::temp_dir();
    let count_entries = || {
        std::fs::read_dir(&tmp)
            .map(|rd| rd.filter_map(|e| e.ok()).count())
            .unwrap_or(0)
    };
    let before = count_entries();

    // Drive the full in-memory ingest path.
    let result = try_fetch_manifest(&gh, "acme/widgets", 42, &cfg).await;
    assert!(
        result.is_some(),
        "valid v2 zip must parse to Some(Manifest)"
    );

    // Assert no new temp files were created during ingestion.
    let after = count_entries();
    assert_eq!(
        before, after,
        "manifest ingestion must not write temp files (HARNESS-05 in-memory invariant)"
    );
}

// ---------------------------------------------------------------------------
// A fork-PR-shaped manifest (the real producer shape) ingests cleanly and
// the digest is well-formed.
// ---------------------------------------------------------------------------

#[test]
fn fork_pr_shaped_manifest_ingests_and_digests() {
    let m = Manifest {
        schema_version: 2,
        run_url: Some("https://github.com/fork/widgets/actions/runs/9".into()),
        artifact_bundle_name: Some("harness-runs-9".into()),
        failures: vec![Failure {
            scenario: "Create tenant persists across reload".into(),
            feature_file: Some("requirements/features/tenant/create-tenant.feature".into()),
            step: Some("Then the tenant list shows \"Acme\"".into()),
            assertion: Some("Expected element to be visible; was not present in DOM".into()),
            artifact_urls: Some(ArtifactUrls {
                result: Some("<dir>/result.json".into()),
                trace: Some("<dir>/trace.zip".into()),
                video: Some("<dir>/video.webm".into()),
                a11y: Some("<dir>/a11y.json".into()),
            }),
        }],
        total_failures: 1,
    };
    let digest = build_failure_digest(&m, "https://github.com/fork/widgets/actions/runs/9", &cfg_with(5_242_880, 20, 8_192));
    assert!(digest.contains("Create tenant persists across reload"));
    assert!(digest.contains("schema_version=2"));
    assert!(digest.contains("bundle 'harness-runs-9'"));
}
