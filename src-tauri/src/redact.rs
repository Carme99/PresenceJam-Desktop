//! Single redaction helper for secret-shaped values in logs (issue #910).
//!
//! Every `[REDACTED len N]` marker in logs and the diagnostics snapshot is
//! built by [`redact_len`] — and only there. Call sites pass the sensitive
//! value; the helper prints its length and nothing else.
//!
//! Deliberately length-only: an earlier `redact_prefix` helper printed the
//! first four characters of its input (24 bits of a base64url secret) into
//! the log. It was deleted with #910 — the deep-link logs in `lib.rs` now
//! record lengths, never prefixes. If a future call site needs values
//! correlated across log lines, add a documented helper here rather than
//! inlining a prefix at the call site.

/// Format a sensitive value for logging: `[REDACTED len N]`, where `N` is
/// the value's byte length.
///
/// Logs the shape of a secret (OAuth `state`, callback URLs) without
/// logging any of its content. Every value routed through here is ASCII
/// (base64url, hex, URLs), so the byte length equals the character count
/// the old inline copies printed.
pub fn redact_len(s: &str) -> String {
    format!("[REDACTED len {}]", s.len())
}

/// Format a pre-measured byte length: `[REDACTED len N]`. Same literal,
/// same site — for call sites that sum lengths without materialising the
/// value (e.g. the diagnostics rebuild over a `Vec<char>` range).
pub fn redact_len_len(n: usize) -> String {
    format!("[REDACTED len {n}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_len_reports_length_and_nothing_else() {
        assert_eq!(redact_len(""), "[REDACTED len 0]");
        assert_eq!(redact_len("abcdef"), "[REDACTED len 6]");
        // NOTE: `candidate` is deliberately NOT named `secret` — gitleaks'
        // generic-api-key rule flags `let secret = "..."` in test code.
        let candidate = "csrf.0123456789abcdef";
        let redacted = redact_len(candidate);
        assert!(
            !redacted.contains(candidate),
            "helper must never echo its input (got: {redacted})"
        );
    }

    #[test]
    fn diagnostics_redaction_embeds_helper_output_for_keyed_value() {
        // Issue #910 step 5: the diagnostics redaction path must emit
        // exactly what the helper builds. `redact_sensitive` is the real
        // call site (the rebuild routes through `redact_len`); an ASCII
        // keyed value pins the seam end to end, so a helper-format drift
        // fails here. Pre-fix this failed: the rebuild inlined the literal
        // instead of calling the helper, so changing the helper's format
        // (or the literal's) would diverge the two. Post-fix the rebuild
        // delegates, so this passes by construction.
        let value = "abc123";
        let out = crate::diagnostics::redact_sensitive(&format!("token={value}"));
        assert_eq!(out, format!("token={}", redact_len(value)));
        // The pass-2 opaque-run seam as well: opaque runs separated by
        // non-opaque chars (`=` acts as a kv separator here, breaking the
        // run) must each embed the helper's own output for that run, not
        // an independently spelled literal.
        let run = "SflKx-wRJSMeKKF2QT4fwpMeJf36P-EXTRA";
        assert!(run.len() >= 32, "run must trip the opaque-run mask");
        let long = format!("a={run} b={run}");
        let masked = crate::diagnostics::redact_sensitive(&long);
        // `key=value` keeps the key name: only the values are masked.
        assert_eq!(
            masked,
            format!("a={} b={}", redact_len(run), redact_len(run))
        );
    }

    #[test]
    fn redaction_literal_is_built_only_in_redact_rs() {
        // Issue #910 acceptance: every length-marker must come from
        // `redact_len`/`redact_len_len`. The needle is the literal prefix
        // `[REDACTED len ` — shape-agnostic, so `{}`, `{n}`, `{0}` and
        // named-arg spellings all match. Doc-comment prose is skipped
        // line-wise below; test expectations live below `mod tests` which
        // is split off. The bare `[REDACTED]` no-length markers are a
        // different, static literal and intentionally out of scope.
        for (name, src) in [
            ("lib.rs", include_str!("lib.rs")),
            ("app.rs", include_str!("app.rs")),
            ("cli.rs", include_str!("cli.rs")),
            ("deep_link.rs", include_str!("deep_link.rs")),
            ("state.rs", include_str!("state.rs")),
            ("diagnostics.rs", include_str!("diagnostics.rs")),
            ("pkce.rs", include_str!("pkce.rs")),
            (
                "commands/spotify_auth.rs",
                include_str!("commands/spotify_auth.rs"),
            ),
            ("tray/mod.rs", include_str!("tray/mod.rs")),
            ("tray/cache.rs", include_str!("tray/cache.rs")),
            ("tray/dedup.rs", include_str!("tray/dedup.rs")),
            ("tray/snooze.rs", include_str!("tray/snooze.rs")),
            ("tray/devices.rs", include_str!("tray/devices.rs")),
            ("tray/actions.rs", include_str!("tray/actions.rs")),
            (
                "config.rs",
                concat!(
                    include_str!("config/schema.rs"),
                    include_str!("config/clamp.rs"),
                    include_str!("config/snooze.rs"),
                    include_str!("config/patch.rs"),
                    include_str!("config/migrate.rs"),
                    include_str!("config/io.rs"),
                    include_str!("config/transfer.rs"),
                    include_str!("config/mod.rs"),
                ),
            ),
            ("teams.rs", include_str!("teams.rs")),
            ("spotify.rs", include_str!("spotify.rs")),
            ("commands/sync.rs", include_str!("commands/sync.rs")),
            ("commands/config.rs", include_str!("commands/config.rs")),
        ] {
            let prod = src.split("#[cfg(test)]\nmod tests").next().unwrap_or(src);
            for (lineno, line) in prod.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                assert!(
                    !line.contains("[REDACTED len "),
                    "{name}:{lineno} builds a `[REDACTED len …]` literal inline — route it through `crate::redact` (issue #910)"
                );
            }
        }
    }
}
