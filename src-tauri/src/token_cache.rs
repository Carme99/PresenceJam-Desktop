//! Generic helper for loading OAuth tokens from the in-memory cache or the
//! persistent store.
//!
//! Both `get_spotify_tokens` and `get_teams_tokens` follow the same
//! pattern: check the in-memory `AppState` cache, fall back to the
//! persistent store on miss, and cache the result back. This module
//! captures that pattern in a single function so the two command
//! implementations stay in sync. See issue #16.

use parking_lot::RwLock;

/// Look up tokens in the in-memory cache, falling back to the supplied
/// `load_from_store` closure on miss.
///
/// On a cache hit the cached value is returned without touching the
/// store. On a miss the closure is invoked; if it returns `Some`, the
/// value is also written back to the cache so subsequent lookups are
/// O(1) until the process restarts.
///
/// `log_prefix` is used to label the per-step log lines (e.g.
/// `"[CMD] get_spotify_tokens"`). All logs are at `info!` level so they
/// remain visible at the default log level.
pub fn get_cached_or_load<T, F>(
    cached: &RwLock<Option<T>>,
    log_prefix: &str,
    load_from_store: F,
) -> Result<Option<T>, String>
where
    T: Clone,
    F: FnOnce() -> Result<Option<T>, String>,
{
    // Check the in-memory cache first.
    {
        let guard = cached.read();
        if let Some(t) = guard.as_ref() {
            log::info!("{}: found tokens in AppState", log_prefix);
            return Ok(Some(t.clone()));
        }
    }
    log::info!("{}: not in AppState, checking store", log_prefix);

    // Fall back to the persistent store.
    let loaded = load_from_store()?;
    if let Some(tokens) = &loaded {
        log::info!("{}: loaded from store", log_prefix);
        let mut guard = cached.write();
        *guard = Some(tokens.clone());
    } else {
        log::info!("{}: no tokens found in store", log_prefix);
    }
    Ok(loaded)
}
