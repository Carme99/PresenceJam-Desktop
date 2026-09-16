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
use parking_lot::Mutex;
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
    rand::rng().fill_bytes(&mut bytes);
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
/// Stored in `AppState::launch_binding` (`LaunchBinding`, in-memory only) at
/// startup and bound into the OAuth `state` param as `<csrf>.<launch_secret>`.
/// Spotify echoes `state` verbatim, so the callback can prove the `code` came
/// from our launch. On macOS the
/// `presencejam://` scheme is registered at build time (Tauri config) — runtime
/// re-registration is not supported, so a hostile app could still intercept the
/// redirect, but without the secret the intercepted `code` is useless (PKCE verifier
/// stays in our AppState). See issue #66.
pub fn generate_launch_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Constant-time string equality (manual XOR-fold).
///
/// Compares byte-by-byte, accumulating differences into a single accumulator
/// so the result never depends on *where* the first mismatch occurs. Lengths
/// are compared up front: state/secret lengths are not confidential (the
/// `state` value is echoed through browser URLs), but content positions must
/// not leak. Used for all OAuth `state` / launch-secret comparisons — see
/// `LaunchBinding::validate_and_consume`. `subtle` is deliberately not pulled
/// in as a dependency for this single use site.
pub fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Per-launch OAuth anti-hijack binding (issue #66; scope-3.3 §C1).
///
/// Held in `AppState::launch_binding` (`OnceLock`, in-memory only, never
/// persisted). Two layers of defense-in-depth:
///
/// - **`launch_secret`** is bound into the OAuth `state` param as
///   `<csrf>.<launch_secret>`. Spotify echoes `state` verbatim
///   (<https://developer.spotify.com/documentation/web-api/tutorials/code-flow>),
///   so the callback can prove the authorization response belongs to this app
///   launch.
/// - **`verifier_hash`** is the SHA-256 of the PKCE verifier for the flow
///   currently in flight (RFC 7636). It is bound at authorize time
///   (`bind_verifier`) and validated at callback time (`validate` for the
///   fail-closed pre-check, `validate_and_consume` once the token exchange
///   succeeded), tying the echoed state to the exact `code_verifier` that will
///   be presented at token exchange. The slot is **single-use**: it is taken
///   only after a successful exchange so a replayed callback fails closed
///   while a transient exchange failure stays retryable, mirroring client-side
///   what RFC 6749 §10.12 requires server-side for authorization codes
///   (<https://datatracker.ietf.org/doc/html/rfc6749#section-10.12>).
///
/// On macOS the `presencejam://` scheme is registered at build time
/// (Info.plist); runtime re-registration is unsupported, so a hostile app can
/// still intercept the redirect, but the intercepted `code` is useless without
/// the launch secret + PKCE verifier (both in memory here). See issue #66.
pub struct LaunchBinding {
    pub launch_secret: String,
    /// SHA-256 (base64url no-pad, 43 chars) of the in-flight flow's PKCE
    /// verifier. `None` when no flow is in flight or it was already consumed.
    pub verifier_hash: Mutex<Option<String>>,
}

impl LaunchBinding {
    pub fn new(launch_secret: String) -> Self {
        Self {
            launch_secret,
            verifier_hash: Mutex::new(None),
        }
    }

    /// Bind the hash of a freshly generated PKCE verifier at authorize time.
    pub fn bind_verifier(&self, verifier: &str) {
        *self.verifier_hash.lock() = Some(generate_challenge(verifier));
    }

    /// Validate the `state` secret component and the pending PKCE verifier
    /// against this binding using constant-time comparisons, **without**
    /// consuming the single-use verifier hash.
    ///
    /// The split from [`Self::validate_and_consume`] exists so a caller can
    /// prove a callback belongs to this app launch *before* it spends the
    /// authorization code, and consume the slot only after the token exchange
    /// actually succeeded — a transient exchange failure (offline, 5xx, locked
    /// keychain, code already redeemed) must leave the flow retryable instead
    /// of burning it (issue #555).
    pub fn validate(&self, state_secret: &str, verifier: &str) -> Result<(), &'static str> {
        let slot = self.verifier_hash.lock();
        self.check(state_secret, slot.as_deref(), verifier)
    }

    /// Validate the `state` secret component and the pending PKCE verifier
    /// against this binding using constant-time comparisons, consuming the
    /// single-use verifier hash on success. A replayed callback finds the
    /// slot empty and is rejected (fail closed).
    pub fn validate_and_consume(
        &self,
        state_secret: &str,
        verifier: &str,
    ) -> Result<(), &'static str> {
        let mut slot = self.verifier_hash.lock();
        self.check(state_secret, slot.as_deref(), verifier)?;
        // Single-use consumption: any subsequent callback with the same
        // state/secret now finds `None` in `check` and fails closed.
        *slot = None;
        Ok(())
    }

    /// Shared comparison core: constant-time `state` secret compare, then the
    /// PKCE verifier-hash linkage. Identical error taxonomy for both the
    /// consuming and non-consuming entry points.
    fn check(
        &self,
        state_secret: &str,
        bound: Option<&str>,
        verifier: &str,
    ) -> Result<(), &'static str> {
        if !ct_eq(state_secret, &self.launch_secret) {
            return Err("launch secret mismatch");
        }
        let bound = match bound {
            Some(h) => h,
            None => return Err("no in-flight verifier binding (replayed or stale callback)"),
        };
        if !ct_eq(bound, &generate_challenge(verifier)) {
            return Err("PKCE verifier does not match the bound challenge");
        }
        Ok(())
    }
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

    #[test]
    fn ct_eq_equal_strings() {
        assert!(ct_eq("", ""));
        assert!(ct_eq("abc", "abc"));
        let s = generate_launch_secret();
        assert!(ct_eq(&s, &s));
    }

    #[test]
    fn ct_eq_single_byte_difference_at_every_position() {
        // Flip one byte at each position; every flip must flip the result.
        // Catches an implementation that stops accumulating after the first
        // N bytes (early-exit leak).
        let a = "0123456789abcdef";
        for i in 0..a.len() {
            let mut b = a.as_bytes().to_vec();
            b[i] ^= 0x01;
            let b = String::from_utf8(b).unwrap();
            assert!(!ct_eq(a, &b), "ct_eq missed a difference at byte {}", i);
        }
    }

    #[test]
    fn ct_eq_length_mismatch_is_rejected() {
        // Truncation hardening: a truncated or extended secret never matches.
        let secret = generate_launch_secret();
        assert!(!ct_eq(&secret[..secret.len() - 1], &secret));
        assert!(!ct_eq(&secret, &format!("{}.{}", secret, "x")));
    }

    #[test]
    fn binding_happy_path_is_single_use() {
        // RFC 6749 §10.12 replay defense: the first valid callback consumes
        // the verifier-hash slot; a replayed callback with identical inputs
        // must fail closed.
        let verifier = generate_verifier();
        let binding = LaunchBinding::new(generate_launch_secret());
        binding.bind_verifier(&verifier);

        assert!(binding
            .validate_and_consume(&binding.launch_secret.clone(), &verifier)
            .is_ok());
        // Replay: same state secret + same verifier → rejected.
        assert!(binding
            .validate_and_consume(&binding.launch_secret.clone(), &verifier)
            .is_err());
    }

    // Issue #555: `validate` must be non-consuming, so a caller can run it as
    // a fail-closed pre-check before the token exchange and consume only after
    // the exchange succeeded. A transient failure therefore leaves the flow
    // retryable, while a genuine replay (consume already happened) still fails.
    #[test]
    fn binding_validate_does_not_consume_the_slot() {
        let verifier = generate_verifier();
        let binding = LaunchBinding::new(generate_launch_secret());
        binding.bind_verifier(&verifier);
        let secret = binding.launch_secret.clone();

        // Repeated non-consuming validation stays green…
        assert!(binding.validate(&secret, &verifier).is_ok());
        assert!(binding.validate(&secret, &verifier).is_ok());
        // …and reports the same taxonomy as the consuming entry point.
        assert_eq!(
            binding.validate(&secret, &generate_verifier()),
            Err("PKCE verifier does not match the bound challenge")
        );
        assert_eq!(
            binding.validate("wrong-secret", &verifier),
            Err("launch secret mismatch")
        );
        // The slot is still live: the success path consumes it exactly once.
        assert!(binding.validate_and_consume(&secret, &verifier).is_ok());
        assert_eq!(
            binding.validate(&secret, &verifier),
            Err("no in-flight verifier binding (replayed or stale callback)")
        );
    }

    #[test]
    fn binding_rejects_wrong_truncated_and_empty_secrets() {
        let verifier = generate_verifier();
        let binding = LaunchBinding::new(generate_launch_secret());
        binding.bind_verifier(&verifier);

        let wrong = LaunchBinding::new(generate_launch_secret());
        assert_eq!(
            binding.validate_and_consume(&wrong.launch_secret, &verifier),
            Err("launch secret mismatch")
        );
        // Truncated copy of the real secret must not match either.
        let truncated = &binding.launch_secret[..binding.launch_secret.len() - 1];
        assert_eq!(
            binding.validate_and_consume(truncated, &verifier),
            Err("launch secret mismatch")
        );
        assert_eq!(
            binding.validate_and_consume("", &verifier),
            Err("launch secret mismatch")
        );
    }

    #[test]
    fn binding_rejects_verifier_not_matching_bound_challenge() {
        // A callback carrying a different flow's code/verifier pairing is
        // rejected even when the launch secret matches.
        let binding = LaunchBinding::new(generate_launch_secret());
        binding.bind_verifier(&generate_verifier());
        let other = generate_verifier();
        assert_eq!(
            binding.validate_and_consume(&binding.launch_secret.clone(), &other),
            Err("PKCE verifier does not match the bound challenge")
        );
    }

    #[test]
    fn binding_fails_closed_without_in_flight_flow() {
        // No bind_verifier call → no binding → reject (stale callback).
        let binding = LaunchBinding::new(generate_launch_secret());
        assert_eq!(
            binding.validate_and_consume(&binding.launch_secret.clone(), &generate_verifier()),
            Err("no in-flight verifier binding (replayed or stale callback)")
        );
    }
}
