//! GitHub webhook HMAC-SHA256 signature verification.
//!
//! GitHub signs every webhook with the secret configured on the webhook
//! itself (matching `BridgeConfig.github.webhook_secret`). The signature
//! arrives as a hex string in the `X-Hub-Signature-256` header with the
//! `sha256=` prefix.
//!
//! Two correctness rules drive the implementation:
//!
//! 1. **Compare in constant time.** Naïve `==` on the digest leaks the
//!    common-prefix length to an attacker who can measure response
//!    timing. `subtle::ConstantTimeEq` is the standard tool.
//! 2. **Verify over the raw request body**, not a parsed JSON
//!    re-serialization. axum's [`bytes::Bytes`] body extractor preserves
//!    the original payload exactly; the handler passes it straight to
//!    [`verify_signature`] before calling `serde_json::from_slice`.
//!
//! See `01-bridge-mvp.md` §5.1 step 2 + §9.1 (`webhook::verify::tests`).

use crate::{Error, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// GitHub's standard header name (case-insensitive on the wire).
pub const SIGNATURE_HEADER: &str = "X-Hub-Signature-256";

/// Required prefix on the header value.
const SIGNATURE_PREFIX: &str = "sha256=";

/// Verify that `signature_header` is the HMAC-SHA256 of `body` keyed by
/// `secret`.
///
/// Returns `Ok(())` on a match. Every failure mode (missing or
/// malformed header, decoding error, digest mismatch) maps to
/// [`Error::Webhook`] with a short reason string — the handler turns
/// any such error into a 401 response.
pub fn verify_signature(
    body: &[u8],
    signature_header: Option<&str>,
    secret: &str,
) -> Result<()> {
    let header = signature_header.ok_or_else(|| {
        Error::Webhook(format!("missing {SIGNATURE_HEADER} header"))
    })?;
    let hex = header.strip_prefix(SIGNATURE_PREFIX).ok_or_else(|| {
        Error::Webhook(format!(
            "{SIGNATURE_HEADER} missing '{SIGNATURE_PREFIX}' prefix"
        ))
    })?;
    let supplied = decode_hex(hex)
        .map_err(|e| Error::Webhook(format!("{SIGNATURE_HEADER} not valid hex: {e}")))?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|e| {
        // Hmac::new_from_slice only errors on key-length issues, which
        // SHA-256 doesn't have. Treat as a configuration problem.
        Error::Webhook(format!("hmac init: {e}"))
    })?;
    mac.update(body);
    let expected = mac.finalize().into_bytes();

    if expected.len() != supplied.len() {
        return Err(Error::Webhook("signature length mismatch".into()));
    }
    if expected.as_slice().ct_eq(supplied.as_slice()).into() {
        Ok(())
    } else {
        Err(Error::Webhook("signature mismatch".into()))
    }
}

/// Minimal hex decoder. We accept lowercase only (matching GitHub's
/// emitter) but tolerate uppercase as well — some intermediate proxies
/// have been known to normalize header casing. A dedicated `hex` crate
/// would do the same; rolling it ourselves keeps the dep tree small.
fn decode_hex(s: &str) -> std::result::Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err(format!("odd length ({})", s.len()));
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for chunk in bytes.chunks(2) {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> std::result::Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        other => Err(format!("non-hex byte: 0x{other:02x}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::Mac;

    /// Compute the canonical header value for a given (secret, body)
    /// pair — what GitHub would send.
    fn signature_header_for(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let digest = mac.finalize().into_bytes();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        format!("{SIGNATURE_PREFIX}{hex}")
    }

    #[test]
    fn happy_path_verifies() {
        let body = br#"{"action":"opened"}"#;
        let secret = "topsecret";
        let header = signature_header_for(secret, body);
        verify_signature(body, Some(&header), secret).expect("valid signature should verify");
    }

    #[test]
    fn wrong_secret_rejected() {
        let body = br#"{"action":"opened"}"#;
        let header = signature_header_for("wrong-secret", body);
        let err = verify_signature(body, Some(&header), "real-secret")
            .expect_err("mismatched secret must reject");
        match err {
            Error::Webhook(s) => assert!(s.contains("signature mismatch"), "got {s}"),
            other => panic!("expected Webhook, got {other:?}"),
        }
    }

    #[test]
    fn missing_header_rejected() {
        let err = verify_signature(b"any body", None, "secret")
            .expect_err("missing header must reject");
        match err {
            Error::Webhook(s) => assert!(s.contains("missing"), "got {s}"),
            other => panic!("expected Webhook, got {other:?}"),
        }
    }

    #[test]
    fn tampered_body_rejected() {
        let body = br#"{"action":"opened"}"#;
        let secret = "k";
        let header = signature_header_for(secret, body);
        // Same header, different body — must reject.
        let tampered = br#"{"action":"closed"}"#;
        let err = verify_signature(tampered, Some(&header), secret)
            .expect_err("tampered body must reject");
        assert!(matches!(err, Error::Webhook(_)));
    }

    #[test]
    fn missing_sha256_prefix_rejected() {
        let body = b"x";
        let err =
            verify_signature(body, Some("abcdef0123456789"), "k").expect_err("no prefix must reject");
        match err {
            Error::Webhook(s) => assert!(s.contains("prefix"), "got {s}"),
            other => panic!("expected Webhook, got {other:?}"),
        }
    }

    #[test]
    fn non_hex_header_rejected() {
        let body = b"x";
        let header = format!("{SIGNATURE_PREFIX}not-hex-data!!!!");
        let err = verify_signature(body, Some(&header), "k").expect_err("non-hex must reject");
        assert!(matches!(err, Error::Webhook(_)));
    }
}
