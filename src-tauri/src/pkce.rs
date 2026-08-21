//! PKCE (Proof Key for Code Exchange) verifier and challenge generation.
//!
//! Used by both the Spotify and Microsoft Teams OAuth flows (RFC 7636).
//! Previously these helpers were duplicated byte-for-byte in `spotify.rs`
//! and `teams.rs` — see issue #75.
//!
//! - `generate_verifier()` returns 64 random bytes encoded as URL-safe
//!   base64 without padding (86 chars), per RFC 7636 §4.1.
//! - `generate_challenge(verifier)` returns the SHA-256 digest of the
//!   verifier, URL-safe base64 no-pad (43 chars), per RFC 7636 §4.2.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Generate a 64-byte PKCE code verifier, base64-url-no-pad encoded.
///
/// Per RFC 7636 §4.1, the verifier must be 43–128 chars from the
/// unreserved character set. 64 bytes → 86 base64 chars → comfortably
/// inside the allowed range. `URL_SAFE_NO_PAD` keeps the alphabet in
/// `[A-Za-z0-9_-]`, which matches the OAuth code_challenge_method=S256
/// contract used by Spotify and Microsoft.
pub fn generate_verifier() -> String {
    let mut bytes = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Generate a PKCE code challenge from a verifier.
///
/// `challenge = BASE64URL-NO-PAD(SHA256(verifier))` (RFC 7636 §4.2).
/// Always 43 chars (32 bytes SHA-256 output, base64-url-no-pad).
pub fn generate_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

/// Generate a per-launch anti-hijack secret (32 random bytes, base64url no-pad, 43 chars).
///
/// Stored in `AppState::launch_secret` (OnceCell) at startup and bound into the
/// OAuth `state` param as `<csrf>.<launch_secret>`. Spotify echoes `state` verbatim,
/// so the callback can prove the `code` came from our launch. On macOS the
/// `presencejam://` scheme is registered at build time (Tauri config) — runtime
/// re-registration is not supported, so a hostile app could still intercept the
/// redirect, but without the secret the intercepted `code` is useless (PKCE verifier
/// stays in our AppState). See issue #66.
pub fn generate_launch_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Redact a sensitive value for logging: `[REDACTED len N]`.
#[allow(dead_code)]
pub fn redact_len(s: &str) -> String {
    format!("[REDACTED len {}]", s.len())
}

/// Redact showing only a 4-char prefix: `abcd…[REDACTED len N]`.
#[allow(dead_code)]
pub fn redact_prefix(s: &str) -> String {
    let prefix: String = s.chars().take(4).collect();
    format!("{}…[REDACTED len {}]", prefix, s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_length() {
        // 64 random bytes → 86 base64-url-no-pad chars (no `=` padding).
        let v = generate_verifier();
        assert_eq!(v.len(), 86, "verifier must be 86 chars, got {}", v.len());
    }

    #[test]
    fn verifier_characters() {
        // RFC 7636 §4.1: unreserved characters only. URL_SAFE_NO_PAD uses
        // [A-Za-z0-9_-] which is a subset of the unreserved set, so this
        // also guarantees the verifier has no `=` padding.
        let v = generate_verifier();
        assert!(
            v.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "verifier contains non-URL-safe chars: {:?}",
            v
        );
    }

    #[test]
    fn challenge_round_trip() {
        // Reproduce the RFC 7636 §4.2 transform locally and confirm it
        // matches `generate_challenge` byte-for-byte. Catches drift if
        // either function is refactored away from the spec.
        let v = generate_verifier();
        let c = generate_challenge(&v);

        let mut hasher = Sha256::new();
        hasher.update(v.as_bytes());
        let expected = URL_SAFE_NO_PAD.encode(hasher.finalize());

        assert_eq!(c, expected);
        assert_eq!(c.len(), 43, "SHA-256 + base64-url-no-pad = 43 chars");
    }

    #[test]
    fn challenge_deterministic() {
        // Same verifier must always produce the same challenge. SHA-256 is
        // deterministic; this guards against accidental non-determinism
        // (e.g. clock-based salt) creeping into generate_challenge.
        let v = "fixed-test-verifier-do-not-perturb-1234567890";
        assert_eq!(generate_challenge(v), generate_challenge(v));
    }
}
