use super::state::{deep_link_callback_key, AppState};
use crate::token_io;
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

pub(crate) async fn handle_spotify_callback<R: tauri::Runtime>(
    code: &str,
    state_param: Option<&str>,
    app: &AppHandle<R>,
) -> Result<(), String> {
    log::debug!(
        "[CALLBACK] handle_spotify_callback: ENTRY - code.len={}",
        code.len()
    );

    let app_state = app.state::<Arc<AppState>>();
    log::info!("[CALLBACK] handle_spotify_callback: got app state");

    // Issue #555: peek, do not take. A callback that fails validation (expired
    // pending, replayed state, transient exchange failure) must leave the
    // pending in place so the same flow stays retryable instead of destroying
    // it on the way to an error the user cannot act on. The pending is claimed
    // below, once the code has actually been redeemed.
    let pending = {
        let guard = app_state.pending.spotify();
        guard.as_ref().cloned().ok_or_else(|| {
            log::error!("[CALLBACK] handle_spotify_callback: No pending Spotify auth found");
            "No pending Spotify auth".to_string()
        })?
    };
    log::info!(
        "[CALLBACK] handle_spotify_callback: pending auth found - verifier.len={}",
        pending.verifier.len()
    );

    // Re-check expiry at submit time. The expiry was set on creation
    // (lib.rs setup, or `commands::spotify_auth::start_spotify_auth`)
    // but only consulted on disk-load. If the OS suspended the process for
    // >10 minutes, the auth code may now be rejected by Spotify as
    // expired. See issue #34.
    if pending.expires_at < chrono::Utc::now() {
        log::error!("[CALLBACK] handle_spotify_callback: auth state expired at submit time");
        return Err("Auth state expired — please try signing in again.".to_string());
    }

    // Verify state matches to prevent CSRF attacks
    if let Some(state_str) = state_param {
        if !crate::pkce::ct_eq(state_str, &pending.state) {
            log::error!(
                "[CALLBACK] handle_spotify_callback: state mismatch - CSRF attack detected"
            );
            return Err("State mismatch - possible CSRF attack".to_string());
        }
        log::info!("[CALLBACK] handle_spotify_callback: state verified successfully");
    } else {
        log::error!("[CALLBACK] handle_spotify_callback: missing state parameter in callback URL");
        return Err("Missing state parameter - possible CSRF attack".to_string());
    }

    // Note on #66: deep-link interception by another app is mitigated by
    // the verifier being in AppState only (#65) and, at the OS level, by
    // re-claiming the scheme at every launch — `macos_deeplink` on macOS,
    // `tauri-plugin-deep-link`'s `register_all()` on Windows/Linux.
    // Fetch the client_secret from the OS keychain. It was placed there by
    // `start_spotify_auth` and is never persisted to disk. See issue #9.
    log::info!("[CALLBACK] handle_spotify_callback: reading client_secret from keychain");
    let client_secret = crate::keychain::get_spotify_client_secret()?;

    log::info!(
        "[CALLBACK] handle_spotify_callback: calling complete_spotify_auth (on blocking pool)"
    );
    // The HTTP round-trip uses reqwest::blocking internally; offload it to
    // Tauri's blocking pool so we don't pin an async worker for the full call.
    let code = code.to_string();
    let verifier = pending.verifier.clone();
    let client_id = pending.client_id.clone();
    let client_secret = client_secret.clone();
    let redirect_uri = pending.redirect_uri.clone();
    let exchange = tauri::async_runtime::spawn_blocking(move || {
        crate::spotify::complete_spotify_auth(
            &code,
            &verifier,
            &client_id,
            &client_secret,
            &redirect_uri,
        )
    })
    .await;
    // #555: a failed exchange must not burn the flow — the authorization code
    // was never redeemed, so put the pending back and let the user retry the
    // same callback instead of starting over from a fresh consent screen.
    let tokens = match exchange {
        Ok(Ok(tokens)) => tokens,
        Ok(Err(e)) => {
            crate::commands::spotify_auth::restore_pending_after_failed_exchange(
                &app_state, pending,
            );
            log::error!(
                "[CALLBACK] handle_spotify_callback: token exchange failed - {}",
                e
            );
            return Err(e);
        }
        Err(e) => {
            crate::commands::spotify_auth::restore_pending_after_failed_exchange(
                &app_state, pending,
            );
            let msg = format!("Spotify OAuth callback task failed: {}", e);
            log::error!("[CALLBACK] handle_spotify_callback: {}", msg);
            return Err(msg);
        }
    };
    log::info!(
        "[CALLBACK] handle_spotify_callback: token exchange successful - access_token.len={}",
        tokens.access_token.len()
    );

    // The code is redeemed, so claim the pending for this flow and consume the
    // single-use launch binding — and only now (#555). A mismatch means a
    // concurrent flow replaced the pending while the exchange was in flight:
    // restore what we took and fail closed rather than claiming another flow's
    // pending (#351).
    {
        let mut guard = app_state.pending.spotify_mut();
        match guard.take() {
            Some(taken) if crate::pkce::ct_eq(&taken.state, &pending.state) => {}
            Some(taken) => {
                *guard = Some(taken);
                log::error!(
                    "[CALLBACK] handle_spotify_callback: pending changed during the exchange"
                );
                return Err("No pending Spotify auth".to_string());
            }
            None => {
                log::error!("[CALLBACK] handle_spotify_callback: pending consumed concurrently");
                return Err("No pending Spotify auth".to_string());
            }
        }
    }
    if let Some(binding) = app_state.launch_binding.get() {
        let secret_component = pending.state.rsplit('.').next().unwrap_or("");
        if let Err(reason) = binding.validate_and_consume(secret_component, &pending.verifier) {
            log::warn!(
                "[CALLBACK] handle_spotify_callback: launch binding not consumed after a successful exchange ({}) [REDACTED]",
                reason
            );
        }
    }

    // Issue #932: the post-exchange commit block lives in the single shared
    // `commit_spotify_session` seam so both Spotify commit paths share the
    // same non-fatal persist policy (matching the Teams precedent from
    // #562). A locked keychain, full disk or failed AES-key write leaves
    // the live session in `AppState` and surfaces the gap on its own
    // `spotify-auth-persist-warning` event instead of propagating an IPC
    // error the UI would render as a sign-in failure. There is intentionally
    // NO per-path wrapper between this call and the seam: a regression that
    // re-introduced the pre-#932 `token_io::persist_tokens(...)?` short-circuit
    // here would make the seam unused-but-computable, which is exactly the
    // mutation the reviewer disproved on commit `b11b254`. The source guard
    // `deep_link_handler_uses_commit_spotify_session_seam` (in `commands/spotify_auth.rs`)
    // pins this call site so any revert to `?`-propagation breaks the suite.
    crate::commands::spotify_auth::commit_spotify_session(
        &app_state,
        tokens,
        "[CALLBACK] handle_spotify_callback",
        |s| token_io::persist_tokens(s, app),
        |event| crate::commands::spotify_auth::emit_spotify_auth_event(app, event),
    );

    log::info!("[CALLBACK] handle_spotify_callback: SUCCESS");
    Ok(())
}

/// Issue #1122: a `presencejam://` callback that arrives before
/// `app.manage(state.clone())` has run would be dropped by the
/// unmanaged-state guard in `handle_deep_link`. The guard buffers the URL
/// here (at most one; a second early callback overwrites) and the setup
/// closure drains it right after `manage()` via `take_pending_deep_link()`
/// and re-dispatches through `handle_deep_link_from_app`, so the replay is
/// the first delivery the dedup gate sees (the early arm returns before the
/// `deep_link_seen` claim, never claiming the key).
///
/// `parking_lot::Mutex` has no poisoning, so `lock()` is infallible — no
/// `unwrap()` on a fallible path (AGENTS.md §4). The buffered URL carries
/// the OAuth code, so it is never logged; only its presence is.
static PENDING_DEEP_LINK: std::sync::LazyLock<Mutex<Option<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));

/// The `LazyLock` slot behind the pending deep-link buffer.
pub(crate) fn pending_deep_link_slot() -> &'static Mutex<Option<String>> {
    &PENDING_DEEP_LINK
}

/// Buffer one pre-manage deep-link URL, overwriting any earlier one (issue
/// #1122). Logs nothing here — the caller logs presence only, never the URL,
/// code, verifier, or state contents (AGENTS.md §7).
pub(crate) fn buffer_pending_deep_link(url: &str) {
    *pending_deep_link_slot().lock() = Some(url.to_string());
}

/// Drain the buffered pre-manage deep-link URL, if any (issue #1122). Single
/// replay: the slot is empty after the take, so a second call is a no-op.
pub(crate) fn take_pending_deep_link() -> Option<String> {
    pending_deep_link_slot().lock().take()
}

/// Production entry point for [`handle_deep_link`]: binds the managed-state
/// lookup and the token-exchange spawn to a live `AppHandle`.
///
/// `try_state` — not `state` — is the lookup: `state()` panics with "state()
/// called before manage()", so a lookup that can run before `app.manage()`
/// must not use it. See the seam's doc comment for whether this function's
/// current callers can (they cannot — this is defence-in-depth).
pub(crate) fn handle_deep_link_from_app<R: tauri::Runtime>(url: &str, app: AppHandle<R>) {
    handle_deep_link(
        url,
        || {
            app.try_state::<Arc<AppState>>()
                .map(|state| state.inner().clone())
        },
        |code_str, state_param| {
            let app_clone = app.clone();

            log::info!("[DEEP_LINK] handle_deep_link: routing to Spotify callback");
            tauri::async_runtime::spawn(async move {
                log::info!("[DEEP_LINK] handle_deep_link: spawning Spotify callback handler");
                if let Err(e) =
                    handle_spotify_callback(&code_str, state_param.as_deref(), &app_clone).await
                {
                    log::error!("[DEEP_LINK] handle_spotify_callback: FAILED - {}", e);
                    log::info!("[DEEP_LINK] handle_deep_link: EMIT spotify-auth-failed event");
                    let _ = app_clone.emit("spotify-auth-failed", e);
                }
            });
        },
    );
}

/// Validates one `presencejam://` callback and, if it survives every gate,
/// hands the `(code, state)` pair to `dispatch`.
///
/// The two effects the production path needs from a live app — reading the
/// managed `AppState` and spawning the token exchange — are parameters, so
/// the gates are driven directly by the unit tests and no GUI runtime has to
/// exist.
///
/// Issue #937 asked this to tolerate a forwarded callback arriving before
/// `app.manage(state.clone())` has run. Tracing the call graph says no
/// caller can do that today, so this is defence-in-depth rather than a fix
/// for an observed crash: both call sites live in the setup closure *after*
/// `manage()` — the `get_current()` drain and the `on_open_url` listener
/// registration. `tauri-plugin-deep-link`'s `handle_cli_arguments` does run
/// during `Builder::build`, but its `deep-link://new-url` emit has no
/// listener yet at that point, so the URL survives only in the plugin's
/// `current` slot and is drained later by `get_current()` — after
/// `manage()`.
///
/// A lookup returning `None` is therefore the arm the test drives: it is
/// what a pre-`manage()` caller would see, and the callback is buffered for
/// replay after `manage()` (issue #1122) instead of panicking the thread
/// meant to redeem it.
/// [`handle_deep_link_from_app`] supplies `try_state` and the spawn; its
/// binder is deliberately UNPINNED by tests (no hermetic `AppHandle` exists
/// outside Tauri's `test` feature, which #937's rework removed because it
/// breaks the Windows test binary's loader), so a revert of the binder to
/// `state()` would fail no test. That is tolerable only while every caller
/// is post-`manage()`.
pub(crate) fn handle_deep_link<S, D>(url: &str, managed_state: S, dispatch: D)
where
    S: FnOnce() -> Option<Arc<AppState>>,
    D: FnOnce(String, Option<String>),
{
    // #228: never log raw callback URL (contains code + state). Log length
    // only (#910 deleted the 4-char prefix: 24 bits of secret in the log).
    log::debug!(
        "[DEEP_LINK] handle_deep_link: ENTRY - url {}",
        crate::redact::redact_len(url)
    );

    if let Ok(parsed) = url::Url::parse(url) {
        log::info!("[DEEP_LINK] handle_deep_link: URL parsed successfully");
        let scheme = parsed.scheme();
        log::info!("[DEEP_LINK] handle_deep_link: scheme={}", scheme);

        if scheme == "presencejam" {
            log::info!("[DEEP_LINK] handle_deep_link: recognized as presencejam scheme");

            let code = parsed
                .query_pairs()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.to_string());
            let state_param = parsed
                .query_pairs()
                .find(|(k, _)| k == "state")
                .map(|(_, v)| v.to_string());

            if let Some(code_str) = code {
                log::info!(
                    "[DEEP_LINK] handle_deep_link: code found - code.len={}",
                    code_str.len()
                );
                // Issue #799: single-flight per (code, state). A Win/Linux
                // second-instance launch reaches the running instance through
                // `handle_cli_arguments` → `deep-link://new-url` →
                // `on_open_url` → `handle_deep_link`; the argv scan that used
                // to re-dispatch it from `forward_launch_to_running_instance`
                // is gone, and this gate drops any identical repeat that still
                // arrives inside the dedup window (e.g. a `get_current` start
                // URL followed by the same `on_open_url` event) before a
                // second token exchange can be spawned. Checked before
                // validation on purpose: a repeat is byte-identical, so
                // whatever the first delivery decides applies to it too.
                let callback_key = deep_link_callback_key(&code_str, state_param.as_deref());
                // Issue #937: `state()` panics with "state() called before
                // manage()", so this lookup must not use it. The lookup is
                // injected and reports absence as `None`, which turns that
                // panic into a logged no-op. No current caller reaches this
                // before `manage()` (both call sites are in the setup
                // closure, after it), so this is defence-in-depth.
                let Some(app_state) = managed_state() else {
                    // Issue #1122: buffer the URL for replay after `manage()`.
                    // This arm sits before the `deep_link_seen` claim, so the
                    // key is never claimed here and the replay is the first
                    // delivery the dedup gate sees. Presence-only logging — the
                    // URL carries the OAuth code (AGENTS.md §7).
                    buffer_pending_deep_link(url);
                    log::warn!(
                        "[DEEP_LINK] handle_deep_link: AppState not registered yet — \
                         buffering forwarded callback for replay (issues #937/#1122)"
                    );
                    return;
                };
                if !app_state.deep_link_seen.claim(callback_key) {
                    log::info!(
                        "[DEEP_LINK] handle_deep_link: duplicate callback inside the dedup window — dropping before any token exchange"
                    );
                    return;
                }
                // #66 option b + scope-3.3 §C1: per-launch secret bound into
                // state as `<csrf>.<launch_secret>`. Spotify echoes state
                // verbatim (https://developer.spotify.com/documentation/web-api/tutorials/code-flow),
                // so we can validate the callback against the binding stored in
                // AppState (OnceLock, in-memory only). Defense-in-depth layers,
                // all fail-closed before any token exchange:
                //   1. Strict structure: exactly `<csrf>.<secret>`, both
                //      non-empty base64url components — rejects truncated or
                //      malformed states.
                //   2. Constant-time compare of the full echoed state against
                //      the stored pending state (`pkce::ct_eq`, no early-exit
                //      content leak).
                //   3. PKCE linkage: the launch binding's verifier hash (bound
                //      at authorize time) must match the pending auth's
                //      verifier.
                //   4. Single-use consumption (RFC 6749 §10.12
                //      https://datatracker.ietf.org/doc/html/rfc6749#section-10.12):
                //      the check here is the NON-consuming
                //      `LaunchBinding::validate` — it must reject a foreign or
                //      replayed callback before any exchange, yet leave the
                //      slot intact so a *transient* exchange failure stays
                //      retryable (#555). The consumption happens in
                //      `handle_spotify_callback`, after the code was redeemed,
                //      and only there. The full state compare + `take()` of the
                //      pending auth also stay in `handle_spotify_callback`.
                // Scheme stays `presencejam://`
                // because macOS bundle scheme registration is config-time only (Info.plist) —
                // runtime re-registration is not supported, so hijack remains possible on macOS
                // but the intercepted `code` is useless without secret + PKCE verifier.
                // #228: redact state in logs — never log raw values.
                if let Some(st) = &state_param {
                    let secret_ok = {
                        // Issue #937: this used to be a second `app.state::<>()`,
                        // i.e. a second panic site inside the launch-binding
                        // validation. The pre-manage guard above already
                        // holds the one lookup this path may use — and once
                        // `manage()` has run the state cannot go back — so
                        // the same handle is reused here.
                        match app_state.launch_binding.get() {
                            Some(binding) => {
                                let parts: Vec<&str> = st.splitn(2, '.').collect();
                                if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty()
                                {
                                    // Peek (do not take) the pending auth: consumption of the
                                    // pending itself stays in handle_spotify_callback.
                                    let pending_peek = app_state.pending.spotify_mut();
                                    match pending_peek.as_ref() {
                                        Some(pending) if crate::pkce::ct_eq(st, &pending.state) => {
                                            match binding.validate(parts[1], &pending.verifier) {
                                                Ok(()) => true,
                                                Err(reason) => {
                                                    log::warn!(
                                                        "[DEEP_LINK] handle_deep_link: launch binding rejected ({}) — state {} — ignoring callback (possible hijack/replay)",
                                                        reason,
                                                        crate::redact::redact_len(st)
                                                    );
                                                    false
                                                }
                                            }
                                        }
                                        Some(_) => {
                                            log::warn!(
                                                "[DEEP_LINK] handle_deep_link: state mismatch vs pending auth — state {} — ignoring callback (possible hijack)",
                                                crate::redact::redact_len(st)
                                            );
                                            false
                                        }
                                        None => {
                                            log::warn!(
                                                "[DEEP_LINK] handle_deep_link: no pending Spotify auth in AppState — ignoring callback (stale or replayed) {}",
                                                crate::redact::redact_len(st)
                                            );
                                            false
                                        }
                                    }
                                } else {
                                    log::warn!(
                                        "[DEEP_LINK] handle_deep_link: malformed state (expected <csrf>.<secret>) — state {} — ignoring callback (possible truncation/hijack)",
                                        crate::redact::redact_len(st)
                                    );
                                    false
                                }
                            }
                            None => {
                                log::warn!(
                                    "[DEEP_LINK] handle_deep_link: no launch binding in AppState — ignoring callback {}",
                                    crate::redact::redact_len(st)
                                );
                                false
                            }
                        }
                    };
                    if !secret_ok {
                        return;
                    }
                } else {
                    log::warn!(
                        "[DEEP_LINK] handle_deep_link: missing state in callback — ignoring"
                    );
                    return;
                }
                dispatch(code_str, state_param);
            } else {
                log::warn!("[DEEP_LINK] handle_deep_link: no code found in URL");
            }
        } else {
            log::warn!("[DEEP_LINK] handle_deep_link: unknown scheme - {}", scheme);
        }
    } else {
        log::error!("[DEEP_LINK] handle_deep_link: failed to parse URL");
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::{DeepLinkDedup, PendingSpotifyAuth, DEEP_LINK_DEDUP_WINDOW};
    use super::*;
    use std::time::Instant;

    /// Serialises the tests that touch the process-wide `PENDING_DEEP_LINK`
    /// slot (issue #1122): the #937 unmanaged-state test arms it as a side
    /// effect while the #1122 tests assert exact buffer contents, so two of
    /// them landing on parallel test threads could flake. Poison-tolerant
    /// acquire, mirroring the `SERIAL` pattern in `platform/focus.rs`.
    static PENDING_DEEP_LINK_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Issue #799: one Win/Linux second-instance launch must redeem the
    /// authorization `code` exactly once. The single-instance plugin (built
    /// with the `deep-link` feature) already routes the argv URL through
    /// `handle_cli_arguments` → `deep-link://new-url` → `on_open_url` →
    /// `handle_deep_link`, so the argv scan that used to live in
    /// `forward_launch_to_running_instance` double-dispatched every callback
    /// (the second exchange failed → spurious `spotify-auth-failed`). This
    /// guard fails pre-fix (the body calls `handle_deep_link` on an argv
    /// `presencejam://` URL) and passes post-fix. Brace-counted body
    /// isolation via the shared `body_of` helper (order-independent, never
    /// anchored on the following fn).
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is the ABSENCE of `handle_deep_link` and argv URL scans in the forward-launch fn; the fn runs inside the single-instance plugin wiring, so the absence is pinned at the source.
    #[test]
    fn test_forward_launch_does_not_redispatch_deep_links() {
        let source = include_str!("app.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("app.rs has no #[cfg(test)] mod tests block");
        let body = body_of(prod_source, "fn forward_launch_to_running_instance(");
        assert!(
            !body.contains("handle_deep_link"),
            "forward_launch_to_running_instance must not call handle_deep_link: \
                 the single-instance plugin's deep-link feature already routes the \
                 argv URL via on_open_url, and a second dispatch redeems the same \
                 Spotify code twice (issue #799)"
        );
        assert!(
            !body.contains("presencejam://"),
            "forward_launch_to_running_instance must not scan argv for deep-link \
                 URLs (issue #799)"
        );
    }

    /// Issue #799: `handle_deep_link` must pass every callback through the
    /// single-flight gate before spawning the token exchange, so the two
    /// delivery paths for one URL (`get_current` start URL + `on_open_url`
    /// event, or any plugin re-emit) collapse to exactly one exchange.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is the hash-then-claim-then-spawn ordering; the dispatch spawns a real token exchange, so the ordering is pinned at the source.
    #[test]
    fn test_handle_deep_link_claims_single_flight_before_dispatch() {
        let source = include_str!("deep_link.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("deep_link.rs has no #[cfg(test)] mod tests block");
        let body = body_of(prod_source, "fn handle_deep_link<");
        assert!(
            body.contains("deep_link_callback_key("),
            "handle_deep_link must hash the (code, state) pair for the \
                 single-flight gate (issue #799)"
        );
        assert!(
            body.contains("deep_link_seen.claim("),
            "handle_deep_link must claim the single-flight gate before spawning \
                 the token exchange (issue #799)"
        );
    }

    /// Issue #799: the gate behind the single-flight claim. Drives one URL
    /// through both delivery paths (same `(code, state)` key claimed twice)
    /// and asserts exactly one proceeds; a different pair and an expired
    /// window proceed normally so legitimate retries are never wedged.
    /// Fails pre-fix (no gate ⇒ two spawns); passes post-fix.
    #[test]
    fn test_deep_link_dedup_single_flight_per_code_and_state() {
        let key_a = deep_link_callback_key("code-abc", Some("csrf.secret"));
        // Same inputs hash to the same key — the two delivery paths for one
        // URL meet at the gate.
        assert_eq!(
            deep_link_callback_key("code-abc", Some("csrf.secret")),
            key_a,
            "identical (code, state) pairs must map to one dedup key"
        );
        // Either component differs → a different login, not a repeat.
        assert_ne!(
            deep_link_callback_key("code-xyz", Some("csrf.secret")),
            key_a,
            "a different code must not share the dedup key"
        );
        assert_ne!(
            deep_link_callback_key("code-abc", Some("other.secret")),
            key_a,
            "a different state must not share the dedup key"
        );

        // Both delivery paths for one URL: first proceeds, repeat drops.
        let gate = DeepLinkDedup::new();
        let t0 = Instant::now();
        assert!(gate.claim_at(key_a, t0), "first delivery must proceed");
        assert!(
            !gate.claim_at(key_a, t0 + std::time::Duration::from_secs(1)),
            "identical repeat inside the window must be dropped"
        );
        assert!(
            !gate.claim_at(
                key_a,
                t0 + DEEP_LINK_DEDUP_WINDOW - std::time::Duration::from_secs(1)
            ),
            "repeat at the window edge must still be dropped"
        );

        // A different (code, state) pair is a new login, not a repeat.
        let gate = DeepLinkDedup::new();
        assert!(gate.claim_at(key_a, t0), "first delivery must proceed");
        assert!(
            gate.claim_at(
                deep_link_callback_key("code-xyz", Some("csrf.secret")),
                t0 + std::time::Duration::from_secs(1)
            ),
            "a different (code, state) pair must proceed"
        );

        // After the window the same pair is a fresh delivery, so a retried
        // flow can never wedge behind a stale claim.
        let gate = DeepLinkDedup::new();
        assert!(gate.claim_at(key_a, t0), "first delivery must proceed");
        assert!(
            gate.claim_at(
                key_a,
                t0 + DEEP_LINK_DEDUP_WINDOW + std::time::Duration::from_secs(1)
            ),
            "same pair past the window must proceed"
        );
    }

    /// Issue #937: `handle_deep_link` used to look the managed state up with
    /// `app.state::<Arc<AppState>>()`, which panics with "state() called
    /// before manage()" when the lookup runs before the setup closure has run
    /// `app.manage(state.clone())`.
    ///
    /// To be accurate about what this pins: no current caller can produce
    /// that window. Both call sites are in the setup closure *after*
    /// `manage()` — the `get_current()` drain and the `on_open_url` listener
    /// registration. `tauri-plugin-deep-link`'s `handle_cli_arguments` does
    /// run during `Builder::build`, but it emits `deep-link://new-url` while
    /// no listener is registered yet, so the URL survives only in the
    /// plugin's `current` slot and is drained by `get_current()` — after
    /// `manage()`. The change is defence-in-depth, and this test is its
    /// regression cover.
    ///
    /// The lookup is injected, so a lookup returning `None` — what a
    /// pre-`manage()` caller would see — is the arm driven here: the callback
    /// must be buffered for replay after `manage()` (issue #1122), never
    /// dispatched for a half-initialised state, and must never take the
    /// callback thread down with it.
    ///
    /// Companion assertion: the arm must not claim the #799 `deep_link_seen`
    /// key (that claim sits below the guard), so the post-setup replay is the
    /// first delivery the dedup gate sees.
    #[test]
    fn handle_deep_link_tolerates_missing_state_issue_937() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        // Hold the slot serial lock: the early arm buffers as a side effect,
        // and the #1122 tests assert exact buffer contents. Drain on entry
        // and exit so no placeholder leaks into a sibling test.
        let _serial = PENDING_DEEP_LINK_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = take_pending_deep_link();

        let dispatched = std::cell::RefCell::new(Vec::<(String, Option<String>)>::new());
        // A syntactically-valid callback URL. The unmanaged-state guard fires
        // before any field is read, so the code/state values are
        // placeholders for this regression test.
        let url = "presencejam://callback?code=test-code&state=csrf.test-secret";

        let unwound = catch_unwind(AssertUnwindSafe(|| {
            handle_deep_link(
                url,
                || None,
                |code, state| {
                    dispatched.borrow_mut().push((code, state));
                },
            );
        }));

        assert!(
            unwound.is_ok(),
            "handle_deep_link must survive an unmanaged AppState (issue #937): \
                 pre-fix this path called app.state::<Arc<AppState>>(), which panics \
                 with 'state() called before manage()' and would take the callback \
                 thread down with it"
        );
        assert!(
            dispatched.borrow().is_empty(),
            "an unmanaged AppState must not dispatch the callback, never spawn a token \
                 exchange for a half-initialised state (issues #937/#1122) — got {} \
                 dispatch(es)",
            dispatched.borrow().len()
        );
        // Issue #1122 changed the "dropped" half of this contract: the early
        // arm buffers the URL for the post-`manage()` replay instead of
        // discarding it. Assert the buffering here so a revert to drop fails.
        assert!(
            take_pending_deep_link().is_some_and(|u| u.contains("code=test-code")),
            "an unmanaged AppState must buffer the callback for replay after \
                 manage(), not discard it (issue #1122)"
        );
        // Leave the slot drained: the companion #1122 tests start clean.
        assert_eq!(take_pending_deep_link(), None);
    }

    /// The other arm of issue #937: with a managed `AppState`, a callback
    /// that clears every gate is dispatched exactly once, with the same
    /// `(code, state)` pair the URL carried. Pins that the unmanaged-state
    /// guard is the only thing the seam changed — the validation ladder
    /// below it (single-flight claim, launch binding, PKCE linkage) still
    /// runs and still gates the exchange.
    #[test]
    fn handle_deep_link_dispatches_valid_callback_when_state_is_managed_issue_937() {
        let state = Arc::new(AppState::new());
        let verifier = crate::pkce::generate_verifier();
        let secret = {
            let binding = state
                .launch_binding
                .get()
                .expect("AppState::new() initialises the launch binding");
            binding.bind_verifier(&verifier);
            binding.launch_secret.clone()
        };
        let state_param = format!("csrf.{secret}");
        *state.pending.spotify_mut() = Some(PendingSpotifyAuth {
            verifier,
            state: state_param.clone(),
            client_id: "cid".to_string(),
            redirect_uri: "presencejam://callback".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(600),
        });

        let url = format!("presencejam://callback?code=the-code&state={state_param}");
        let dispatched = std::cell::RefCell::new(Vec::<(String, Option<String>)>::new());
        handle_deep_link(
            &url,
            || Some(state.clone()),
            |code, state| {
                dispatched.borrow_mut().push((code, state));
            },
        );

        assert_eq!(
            dispatched.borrow().len(),
            1,
            "a managed AppState with a valid launch binding must dispatch the \
                 callback exactly once (issue #937)"
        );
        assert_eq!(
            dispatched.borrow()[0],
            ("the-code".to_string(), Some(state_param)),
            "the dispatched pair must be the one the URL carried (issue #937)"
        );
    }

    /// Issue #1122: a callback arriving before `manage()` must be buffered
    /// and replayed, not dropped. Drives `handle_deep_link` pre-manage (the
    /// #937 seam: lookup returns `None`), asserts nothing dispatched and the
    /// URL is buffered; then drains via `take_pending_deep_link()` and
    /// re-drives through the same `handle_deep_link` path with a managed
    /// state, asserting the callback completes exactly once. Fails pre-fix
    /// (pre-fix the early arm drops the URL, so the drain finds nothing).
    #[test]
    fn handle_deep_link_buffers_pre_manage_callback_and_replays_issue_1122() {
        // The slot is process-wide; hold the serial lock for the whole test
        // so a parallel #937/#1122 sibling cannot interleave a buffer write.
        let _serial = PENDING_DEEP_LINK_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Start clean and leave clean: drain entry and exit.
        let _ = take_pending_deep_link();

        // Build a callback that clears every gate once managed: bind a
        // launch secret + verifier and stage the pending auth, mirroring the
        // #937 managed-state test.
        let state = Arc::new(AppState::new());
        let verifier = crate::pkce::generate_verifier();
        let secret = {
            let binding = state
                .launch_binding
                .get()
                .expect("AppState::new() initialises the launch binding");
            binding.bind_verifier(&verifier);
            binding.launch_secret.clone()
        };
        let state_param = format!("csrf.{secret}");
        *state.pending.spotify_mut() = Some(PendingSpotifyAuth {
            verifier,
            state: state_param.clone(),
            client_id: "cid".to_string(),
            redirect_uri: "presencejam://callback".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(600),
        });
        let url = format!("presencejam://callback?code=replay-code&state={state_param}");

        // 1. Pre-manage delivery: no dispatch, URL buffered.
        let dispatched = std::cell::RefCell::new(Vec::<(String, Option<String>)>::new());
        handle_deep_link(
            &url,
            || None,
            |code, state| {
                dispatched.borrow_mut().push((code, state));
            },
        );
        assert!(
            dispatched.borrow().is_empty(),
            "a pre-manage callback must not dispatch before setup completes \
                 (issue #1122)"
        );
        assert!(
            take_pending_deep_link().is_some(),
            "a pre-manage callback must be buffered for replay after manage() \
                 (issue #1122) — pre-fix this was dropped"
        );

        // 2. Post-manage replay through the same path completes auth.
        // Re-buffer the same URL: the assertion drain above consumed it,
        // mirroring the setup-closure `take()` that feeds the production
        // replay; driving `handle_deep_link` with a managed state is the
        // same path that replay takes.
        buffer_pending_deep_link(&url);
        let replayed = take_pending_deep_link();
        assert!(
            replayed.is_some(),
            "the replay must drain the buffered URL (issue #1122)"
        );
        let replayed_dispatch = std::cell::RefCell::new(Vec::<(String, Option<String>)>::new());
        handle_deep_link(
            &url,
            || Some(state.clone()),
            |code, state| {
                replayed_dispatch.borrow_mut().push((code, state));
            },
        );
        assert_eq!(
            replayed_dispatch.borrow().len(),
            1,
            "the replayed callback must complete auth exactly once (issue #1122)"
        );
        assert_eq!(
            replayed_dispatch.borrow()[0],
            ("replay-code".to_string(), Some(state_param)),
            "the replayed pair must be the one the URL carried (issue #1122)"
        );

        // 3. Single replay: the slot is drained, a second take is a no-op.
        assert_eq!(
            take_pending_deep_link(),
            None,
            "the buffer must drain exactly once — no retry loop (issue #1122)"
        );
    }

    /// Issue #1122: only one URL is retained — a second early callback
    /// overwrites rather than queues.
    #[test]
    fn handle_deep_link_second_early_callback_overwrites_issue_1122() {
        let _serial = PENDING_DEEP_LINK_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = take_pending_deep_link();

        handle_deep_link(
            "presencejam://callback?code=first-code&state=csrf.first-secret",
            || None,
            |_, _| {},
        );
        handle_deep_link(
            "presencejam://callback?code=second-code&state=csrf.second-secret",
            || None,
            |_, _| {},
        );
        let buffered = take_pending_deep_link();
        assert!(
            buffered.is_some_and(|u| u.contains("code=second-code")),
            "a second early callback must overwrite, not queue (issue #1122)"
        );
        assert_eq!(
            take_pending_deep_link(),
            None,
            "the overwrite must still drain exactly once (issue #1122)"
        );
    }

    /// Brace-counted body isolation for a top-level `fn` in this file (house
    /// style — order-independent, never anchored on the following fn, which
    /// drifts).
    fn body_of<'a>(prod_source: &'a str, sig: &str) -> &'a str {
        let after_sig = prod_source
            .split(sig)
            .nth(1)
            .unwrap_or_else(|| panic!("production source has no `{}`", sig));
        let open = after_sig
            .find('{')
            .unwrap_or_else(|| panic!("{} has no opening brace", sig));
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        &after_sig[..end.unwrap_or_else(|| panic!("{} body never closed", sig))]
    }
}
