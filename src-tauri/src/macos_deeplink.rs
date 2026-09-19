//! macOS-only re-claim of the app's URL scheme (issue #66).
//!
//! `tauri-plugin-deep-link` re-registers the app's custom URL scheme at every
//! launch on Windows (`HKCU\Software\Classes\<scheme>`) and Linux
//! (`~/.local/share/applications/<scheme>.desktop` plus `xdg-mime default`),
//! so a foreign app that pre-registered the scheme is clobbered by our
//! last-write. On macOS that call is a no-op returning
//! `Err(UnsupportedPlatform)`: schemes are claimed through the app bundle's
//! `CFBundleURLTypes`, and LaunchServices gives the *first* claimant
//! priority — so an attacker app that registered `presencejam://` before the
//! user ever installed PresenceJam keeps the redirect, and our plugin call
//! cannot take it back.
//!
//! `LSSetDefaultHandlerForURLScheme` is the supported override: it writes the
//! user's *preferred* handler for a scheme into the LaunchServices database,
//! which beats first-come-first-served registration. This module is the
//! OS-level half of the #66 defence.
//!
//! The cryptographic half is already shipped and stays in force: the PKCE
//! `code_verifier` never leaves `AppState`, the per-launch secret is bound
//! into the OAuth `state` param, and the launch binding is consumed
//! single-use (see `AppState::launch_binding` and `handle_spotify_callback`).
//! Neither half depends on the Spotify Developer Dashboard, so the bare
//! `presencejam://` scheme stays byte-identical to the redirect URI that is
//! already registered there — no dashboard change is needed to ship this.
//!
//! ## FFI contract (CoreServices / LaunchServices)
//!
//! ```c
//! OSStatus LSSetDefaultHandlerForURLScheme(CFStringRef urlScheme,
//!                                          CFStringRef handlerBundleID);
//! ```
//!
//! - Both parameters are borrowed `CFStringRef`s. LaunchServices reads them
//!   for the duration of the call and retains anything it needs internally,
//!   so the `CFString` objects created here may be released as soon as the
//!   call returns (`objc2_core_foundation::CFRetained` handles that on drop).
//! - `handlerBundleID` must name an installed application bundle that
//!   LaunchServices already knows about and whose `Info.plist` declares the
//!   scheme in `CFBundleURLTypes` — i.e. our own `.app`. Under
//!   `npm run tauri dev` the process is a bare binary rather than a bundle,
//!   so the call fails with `kLSNotAnApplicationErr` (-10811) or
//!   `kLSApplicationNotFoundErr` (-10814). That is expected in development
//!   and is only logged.
//! - The call is synchronous, returns `noErr` (0) on success and a negative
//!   `OSStatus` otherwise. It never throws, never blocks on user input, and
//!   never prompts.
//! - LaunchServices is thread-safe. We call it from the Tauri `setup`
//!   closure, after the config-declared windows exist (`tauri.conf.json`
//!   declares the main window visible, so it is already on screen) and
//!   before the tray/menu wiring, and latch the call so the process
//!   performs this global OS-state mutation at most once. The later
//!   ordering is safe because what makes an early interception harmless is
//!   the PKCE launch binding, not window timing — see
//!   `AppState::launch_binding` and `handle_spotify_callback`. A genuinely
//!   pre-window claim would need a plugin init hook and is a separate,
//!   deliberate change.
//! - Apple marks the symbol deprecated as of macOS 12 (superseded by
//!   `-[NSWorkspace setDefaultApplicationAtURL:toOpenURLsWithScheme:completionHandler:]`)
//!   but it remains present and functional, and it is the only *synchronous*
//!   way to set the preferred handler. The replacement is an asynchronous
//!   completion-handler API that would have to be re-entered from an Objective-C
//!   block during startup; we take the deprecated synchronous call and accept
//!   the `#[allow(deprecated)]` at the single call site.
//!
//! The pure parts of this module (`configured_schemes`,
//! `status_description`, the once-only latch) compile and are unit-tested on
//! every host; only the FFI call itself is `#[cfg(target_os = "macos")]`.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// Plugin-config key matching `tauri.conf.json` -> `plugins.deep-link`.
const DEEP_LINK_PLUGIN: &str = "deep-link";

/// Platform sub-object the deep-link plugin reads on desktop targets.
const DESKTOP_KEY: &str = "desktop";

/// Array of URL schemes inside the desktop sub-object.
const SCHEMES_KEY: &str = "schemes";

/// Process-wide latch for the LaunchServices claim.
///
/// `LSSetDefaultHandlerForURLScheme` mutates global OS state (the user's
/// preferred-handler database), so the process performs it at most once no
/// matter how many times [`claim`] is reached. The latch is consumed on the
/// *attempt*, not on success: a retry from the same process would present the
/// same bundle-registration facts to LaunchServices and so would land on the
/// same `OSStatus`, and the call site in `setup` runs exactly once anyway.
static CLAIMED: AtomicBool = AtomicBool::new(false);

/// Collects the desktop URL schemes the app declares in
/// `tauri.conf.json` -> `plugins.deep-link.desktop.schemes`.
///
/// Reading the configured list rather than hardcoding a scheme keeps this
/// module in sync with the manifest that the plugin itself uses; a scheme
/// added to `tauri.conf.json` is re-claimed automatically. Returns an empty
/// vector when the plugin config, the desktop sub-object, or the scheme array
/// is absent or malformed — callers treat that as "nothing to do", not as an
/// error, because the deep-link plugin owns validating its own config.
pub fn configured_schemes(plugins: &HashMap<String, Value>) -> Vec<String> {
    plugins
        .get(DEEP_LINK_PLUGIN)
        .and_then(|plugin| plugin.get(DESKTOP_KEY))
        .and_then(|desktop| desktop.get(SCHEMES_KEY))
        .and_then(Value::as_array)
        .map(|schemes| {
            schemes
                .iter()
                .filter_map(Value::as_str)
                .filter(|scheme| !scheme.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Names a LaunchServices `OSStatus` for the log.
///
/// The values mirror the `kLS*` constants in
/// `<CoreServices/LaunchServices.h>`. They are spelled out as literals rather
/// than imported from `objc2-core-services` so that this table also compiles
/// — and is unit-tested — on the Linux and Windows hosts that build and test
/// this crate; the FFI surface itself is macOS-only.
pub fn status_description(status: i32) -> &'static str {
    match status {
        0 => "noErr",
        -10810 => "kLSUnknownErr",
        -10811 => "kLSNotAnApplicationErr",
        -10813 => "kLSDataUnavailableErr",
        -10814 => "kLSApplicationNotFoundErr",
        -10826 => "kLSNoLaunchPermissionErr",
        _ => "unrecognised OSStatus",
    }
}

/// Re-claims every scheme in `schemes` for `bundle_id`.
///
/// Returns `Ok(true)` when this call was the one that talked to
/// LaunchServices, and `Ok(false)` when a previous call in this process
/// already did (the claim is never repeated). Returns `Err` — with the
/// offending scheme and the decoded `OSStatus` — when LaunchServices rejects
/// the claim; the first failure aborts the remaining schemes, because they
/// will fail for the same reason. Callers log and continue: a failed claim
/// must never block startup, and the PKCE launch binding still defends the
/// callback.
pub fn claim(schemes: &[String], bundle_id: &str) -> Result<bool, String> {
    if !claim_once(&CLAIMED) {
        return Ok(false);
    }
    claim_inner(schemes, bundle_id)
}

/// Latch primitive behind [`claim`], split out so the once-only behaviour is
/// unit-testable against a local flag instead of the process-wide static.
fn claim_once(flag: &AtomicBool) -> bool {
    !flag.swap(true, Ordering::SeqCst)
}

#[cfg(target_os = "macos")]
fn claim_inner(schemes: &[String], bundle_id: &str) -> Result<bool, String> {
    if schemes.is_empty() {
        return Err("no desktop URL schemes configured for the deep-link plugin".to_string());
    }
    for scheme in schemes {
        if let Err(status) = set_default_handler(scheme, bundle_id) {
            return Err(format!(
                "LSSetDefaultHandlerForURLScheme(\"{scheme}\", \"{bundle_id}\") failed: {} ({status})",
                status_description(status)
            ));
        }
    }
    Ok(true)
}

#[cfg(not(target_os = "macos"))]
fn claim_inner(_schemes: &[String], _bundle_id: &str) -> Result<bool, String> {
    Err(
        "LSSetDefaultHandlerForURLScheme is a macOS-only CoreServices API; on this target \
         tauri-plugin-deep-link's register_all() performs the per-launch re-claim"
            .to_string(),
    )
}

/// Sets `scheme`'s preferred handler to the app with bundle id `bundle_id`.
///
/// Returns `Err(os_status)` for anything other than `noErr`.
///
/// # Safety
///
/// `LSSetDefaultHandlerForURLScheme` is an `unsafe extern "C-unwind"` binding
/// (see the FFI contract in the module docs). It is sound here because both
/// arguments are non-null `CFStringRef`s owned by the `CFRetained` values
/// below, which outlive the call.
// Deprecation is accepted deliberately: the supported replacement
// (`-[NSWorkspace setDefaultApplicationAtURL:toOpenURLsWithScheme:completionHandler:]`)
// is asynchronous, and the deprecated symbol is still functional. See the
// module docs. The allow covers the `use` as well as the call.
#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn set_default_handler(scheme: &str, bundle_id: &str) -> Result<(), i32> {
    use objc2_core_foundation::CFString;
    use objc2_core_services::LSSetDefaultHandlerForURLScheme;

    let scheme_cf = CFString::from_str(scheme);
    let bundle_cf = CFString::from_str(bundle_id);

    let status = unsafe { LSSetDefaultHandlerForURLScheme(&scheme_cf, &bundle_cf) };

    if status == 0 {
        log::info!(
            "[DEEP_LINK] set_default_handler: claimed scheme \"{scheme}\" for bundle \"{bundle_id}\""
        );
        Ok(())
    } else {
        Err(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugins_from(json: &str) -> HashMap<String, Value> {
        serde_json::from_str(json).expect("test fixture must be a JSON object")
    }

    #[test]
    fn test_configured_schemes_reads_desktop_scheme_list() {
        let plugins = plugins_from(
            r#"{
                "deep-link": {
                    "desktop": { "schemes": ["presencejam", "presencejam-dev"] }
                },
                "opener": {}
            }"#,
        );
        assert_eq!(
            configured_schemes(&plugins),
            vec!["presencejam".to_string(), "presencejam-dev".to_string()]
        );
    }

    #[test]
    fn test_configured_schemes_tolerates_missing_or_malformed_config() {
        let empty: HashMap<String, Value> = HashMap::new();
        assert!(configured_schemes(&empty).is_empty());

        // Plugin present but no desktop sub-object.
        let no_desktop = plugins_from(r#"{ "deep-link": { "mobile": {} } }"#);
        assert!(configured_schemes(&no_desktop).is_empty());

        // Wrong shape for the scheme list, and non-string / empty entries.
        let malformed =
            plugins_from(r#"{ "deep-link": { "desktop": { "schemes": "presencejam" } } }"#);
        assert!(configured_schemes(&malformed).is_empty());

        let mixed = plugins_from(
            r#"{ "deep-link": { "desktop": { "schemes": [1, "", "presencejam"] } } }"#,
        );
        assert_eq!(
            configured_schemes(&mixed),
            vec!["presencejam".to_string()],
            "non-string and empty entries must be dropped, valid ones kept"
        );
    }

    /// The plugin config path is duplicated as three string constants; this
    /// pins them against the manifest that actually ships.
    #[test]
    fn test_configured_schemes_matches_shipped_tauri_conf() {
        let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("tauri.conf.json must be valid JSON");
        let plugins: HashMap<String, Value> =
            serde_json::from_value(conf["plugins"].clone()).expect("plugins must be an object");

        let schemes = configured_schemes(&plugins);
        assert!(
            schemes.iter().any(|s| s == "presencejam"),
            "tauri.conf.json no longer declares the presencejam scheme for the deep-link \
             plugin (found {schemes:?}) — the macOS re-claim would silently do nothing"
        );
    }

    #[test]
    fn test_status_description_names_launchservices_errors() {
        assert_eq!(status_description(0), "noErr");
        assert_eq!(status_description(-10811), "kLSNotAnApplicationErr");
        assert_eq!(status_description(-10814), "kLSApplicationNotFoundErr");
        assert_eq!(
            status_description(-2),
            "unrecognised OSStatus",
            "an unmapped status must still be reported as a status, not as success"
        );
    }

    #[test]
    fn test_claim_once_is_once_only() {
        let flag = AtomicBool::new(false);
        assert!(claim_once(&flag), "the first caller must win the claim");
        assert!(
            !claim_once(&flag),
            "later callers must not repeat the claim"
        );
        assert!(!claim_once(&flag));
    }

    #[test]
    fn test_claim_once_admits_exactly_one_thread() {
        use std::sync::Arc;
        let flag = Arc::new(AtomicBool::new(false));
        let winners: usize = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let flag = Arc::clone(&flag);
                    scope.spawn(move || usize::from(claim_once(&flag)))
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("thread")).sum()
        });
        assert_eq!(winners, 1, "exactly one thread may perform the OS claim");
    }

    /// `claim` must dispatch through the latch: a second call in the same
    /// process reports "already claimed" without touching the platform.
    #[test]
    fn test_claim_second_call_reports_already_claimed() {
        // Uses the process-wide latch, so this is the only test that calls
        // `claim` directly; `claim_once` is covered by the tests above.
        let first = claim(&[], "com.presencejam.app");
        let second = claim(&[], "com.presencejam.app");
        assert_eq!(
            second,
            Ok(false),
            "a repeat call must short-circuit on the latch, before platform dispatch"
        );
        // On macOS the first call fails (empty scheme list); on other targets
        // it fails as unsupported. Either way it must not panic and must not
        // report success.
        assert!(
            first.is_err(),
            "claiming an empty scheme list must not report success: {first:?}"
        );
    }
}
