//! OS keychain integration for storing OAuth secrets.
//!
//! Wraps the `keyring` crate to provide a stable, minimal API for storing
//! the Spotify `client_secret` in the platform's secure credential store
//! (Windows Credential Manager, macOS Keychain, Linux Secret Service).
//!
//! Migration note: prior to this module, the secret was persisted in plain
//! text via `tauri-plugin-store` and inside `config.json`. The first run
//! after this change will need the user to re-enter the secret via
//! Onboarding. See issue #9.

const KEYRING_SERVICE: &str = "presencejam";
const SPOTIFY_CLIENT_SECRET_USER: &str = "spotify_client_secret";

/// Persist the Spotify `client_secret` in the OS keychain.
///
/// Overwrites any existing entry for `(KEYRING_SERVICE, SPOTIFY_CLIENT_SECRET_USER)`.
pub fn store_spotify_client_secret(secret: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, SPOTIFY_CLIENT_SECRET_USER)
        .map_err(|e| format!("Failed to open keychain entry: {}", e))?;
    entry
        .set_password(secret)
        .map_err(|e| format!("Failed to write Spotify client secret to keychain: {}", e))?;
    log::info!("[KEYCHAIN] Stored Spotify client_secret in OS keychain");
    Ok(())
}

/// Read the Spotify `client_secret` from the OS keychain.
///
/// Returns an error if the entry is missing — the caller should treat this
/// as a "user must re-onboard" signal, not a fatal error.
pub fn get_spotify_client_secret() -> Result<String, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, SPOTIFY_CLIENT_SECRET_USER)
        .map_err(|e| format!("Failed to open keychain entry: {}", e))?;
    match entry.get_password() {
        Ok(secret) => Ok(secret),
        Err(keyring::Error::NoEntry) => Err(
            "Spotify client secret not found in keychain. Please re-enter via Onboarding."
                .to_string(),
        ),
        Err(e) => Err(format!("Failed to read Spotify client secret from keychain: {}", e)),
    }
}

/// Check whether the Spotify `client_secret` is present in the OS keychain.
///
/// Returns `Ok(true)` if the entry exists (even if read fails for other reasons
/// after a successful existence check), `Ok(false)` if it is missing.
pub fn has_spotify_client_secret() -> bool {
    match keyring::Entry::new(KEYRING_SERVICE, SPOTIFY_CLIENT_SECRET_USER) {
        Ok(entry) => entry.get_password().is_ok(),
        Err(_) => false,
    }
}

/// Remove the Spotify `client_secret` from the OS keychain.
///
/// Called on user disconnect / reconnect to wipe the secret.
pub fn delete_spotify_client_secret() -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, SPOTIFY_CLIENT_SECRET_USER)
        .map_err(|e| format!("Failed to open keychain entry: {}", e))?;
    match entry.delete_credential() {
        Ok(()) => {
            log::info!("[KEYCHAIN] Deleted Spotify client_secret from OS keychain");
            Ok(())
        }
        Err(keyring::Error::NoEntry) => {
            // Already gone — treat as success.
            log::info!("[KEYCHAIN] Spotify client_secret not in keychain (already cleared)");
            Ok(())
        }
        Err(e) => Err(format!("Failed to delete Spotify client secret from keychain: {}", e)),
    }
}
