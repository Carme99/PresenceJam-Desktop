# Deep-Link Security Lane — Report

Branch: `fix/deep-link-security` off `ffeee24` (v3.1.0)

## Changed files
- `src-tauri/src/lib.rs` — AppState launch_secret, handle_deep_link validation + redaction, logging wiring (#226), store/shell plugin removal
- `src-tauri/src/pkce.rs` — `generate_launch_secret()` + redact helpers (#66, #228)
- `src-tauri/src/commands/spotify_auth.rs` — state composition `<csrf>.<launch_secret>`, async+spawn_blocking for #215, redacted logs (#228)
- `src-tauri/Cargo.toml` — removed `tauri-plugin-store` + `tauri-plugin-shell` (#227)
- `src-tauri/capabilities/default.json` — audited, no store/shell over-grants (unchanged, already minimal)

## Per-issue what/why

### #66 — hijackable `presencejam://` (per-launch secret, option b)
**Why:** `presencejam://` is registered at build time via `tauri.conf.json` → `Info.plist` on macOS; runtime `register_all()` is `UnsupportedPlatform` on macOS, so another app could pre-register and intercept the redirect. PKCE verifier already in AppState (#65) makes intercepted `code` unusable, but defence-in-depth needs a per-launch binding.

**What:** Added `AppState::launch_secret: OnceLock<String>` (32 random bytes, base64url-no-pad, 43 chars) generated in `AppState::new()` (every `Arc::new(AppState::new())` at `run()` setup). `run_spotify_oauth_flow()` now composes `state = <csrf=verifier(86 chars)>.<launch_secret(43 chars)>`; Spotify echoes `state` verbatim. `handle_deep_link()` splits on `'.'` and validates second component against `AppState::launch_secret`; mismatch → `log::warn!` with redacted prefix/len and `return` (ignore callback, no token exchange). Scheme stays `presencejam://` (config-time constraint). Documented macOS limitation in field doc and inline comments: hijack still possible on macOS but code useless without secret+verifier.

**Grep evidence:** `grep -rn "launch_secret" src-tauri/src/lib.rs` → field, `new()` init, `handle_deep_link` validation, `spotify_auth.rs` composition.

### #228 — redact OAuth codes/states in logs
**Why:** raw `code`/`state`/callback URL must never hit log file.

**What:** 
- `handle_deep_link`: `ENTRY` now `url_len`+4-char prefix + `[REDACTED]`; `state` mismatch logs prefix+len, not value; missing state logs generic.
- `setup` start URLs / `on_open_url`: `[REDACTED len N] prefix=abcd…`.
- `run_spotify_oauth_flow`: `state generated len N [REDACTED]` (no value).
- `complete_spotify_auth_manual`: `ENTRY` adds `[REDACTED]`; state mismatch logs `len X vs Y` only.
- `handle_spotify_callback`: already len-only; left as is.
- Added `pkce::redact_len` / `redact_prefix` helpers for future use.

**Grep evidence:** `grep -rn "REDACTED" src-tauri/src/lib.rs src-tauri/src/commands/spotify_auth.rs` shows all sensitive paths now use `[REDACTED len N]` or prefix-truncated form; `grep -rn "url={}" src-tauri/src/lib.rs` → no matches.

### #227 — audit `tauri-plugin-store` / `tauri-plugin-shell`
**Why:** dead plugins widen supply-chain and capability surface.

**What:** Grepped `src-tauri/**/*.rs` + `src/**/*.{ts,svelte,js}`:
- `tauri_plugin_store` / `tauri_plugin_shell` only appears in `lib.rs` registrations and `Cargo.toml` deps; zero `Store`/`shell` imports in Rust; zero `@tauri-apps/plugin-store` / `@tauri-apps/plugin-shell` imports in `src/` (checked via `grep -R plugin-store src/` → no matches).
- `capabilities/default.json` already had no `store:*` / `shell:*` permissions — already minimal (only core, tray, menu, event, opener, autostart, log, deep-link, updater, notification).
- Removed `lib.rs` `.plugin(tauri_plugin_store::Builder::new().build())` and `.plugin(tauri_plugin_shell::init())`; removed `Cargo.toml` `tauri-plugin-store = "2"` and `tauri-plugin-shell = "2"`.
- `package.json` still lists `@tauri-apps/plugin-store` / `plugin-shell` (not owned — target list excludes `package.json`). Reported here for frontend lane to remove; Rust side and capabilities already trimmed. `tauri-plugin-http` audited similarly — registered but unused via `plugin-http` import (zero matches), left as-is per scope (only store/shell in #227); capabilities already lack `http:*`, so over-grant already trimmed.

### #226 — wire `logging.enabled` / `log_level` into logger
**Why:** `config.rs` `LoggingConfig` fields were persisted but never honoured.

**What:** In `lib.rs::run().setup` after `load_config()` `Ok(cfg)` branch, added block (comment citing #226) that lowercases `cfg.logging.log_level`, maps `off/error/warn/info/debug/trace` case-insensitively to `log::LevelFilter`, with `!enabled → Off`, then `log::set_max_level(max_level)` + info log of resulting level. `Err` (no config) leaves plugin-log default (Info) — matches previous behaviour.

### #215 — `spotify_auth` slice: async + spawn_blocking
**Why:** HTTPS (reqwest::blocking) + OS keychain are blocking; must not pin async runtime.

**What:** 
- `start_spotify_auth`: `pub fn` → `pub async fn`; keychain `store_spotify_client_secret` offloaded via `tauri::async_runtime::spawn_blocking(move || store(&secret_clone)).await.map_err(... )??`; `run_spotify_oauth_flow` stays sync (only memory + opener).
- `complete_spotify_auth_manual`: `pub fn` → `pub async fn`; after fast pending take / expiry / CSRF check, token exchange (`get_spotify_client_secret` + `complete_spotify_auth` blocking HTTPS) offloaded via `spawn_blocking` capturing `code_clone` + `pending_clone`; join error mapped to string, then `??` to propagate `SpotifyApiError`. Pattern mirrors `commands/onboarding.rs::is_onboarding_complete` (precedent cited in code comment). Log tags `[CMD.SPOTIFY_AUTH]` preserved; no `ts-rs` type changes; invoke arg names/casing unchanged.

## Risks
- `launch_secret` is 43 chars b64url; `csrf_state` is 86 chars; composite `state` ~130 chars, well within URL query limits (Spotify allows up to ~500). `urlencoding::encode` handles `.` safely.
- `OnceLock::set` in `AppState::new()` is idempotent; tests that construct multiple `AppState` each get their own secret (no cross-test leak).
- `start_spotify_reconnect` still sync (uses same `run_spotify_oauth_flow` which now reads secret) — no async change needed per #215 slice.
- `package.json` store/shell deps remain — frontend lane must remove to avoid npm bloat; not a Rust build break (Cargo deps already removed).
- `tauri-plugin-http` remains registered but unused and without capability — harmless; removing it would need explicit #227 extension.

## Verification (no build per lane constraints)
- `grep log.*REDACTED` → all sensitive logs now redacted.
- `grep -rn "tauri_plugin_store\|tauri_plugin_shell" src-tauri/src` → zero matches after edit.
- `grep -rn "launch_secret" src-tauri/src` → present in `lib.rs` + `pkce.rs` + `spotify_auth.rs`.
- `cargo check` / `npm` skipped per orchestration (orchestrator verifies once).
