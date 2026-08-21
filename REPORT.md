# Teams Auth Threading — Fix Report

Branch: `fix/teams-auth-threading` (base ffeee24)

## Changed files
- `src-tauri/src/commands/teams_auth.rs` — async polling command, interval clamp, `teams-auth-failed` emissions.
- `src-tauri/src/teams.rs` — blocking poll helper: clamp, capped chunked sleep, explicit terminal errors, `slow_down` handling.

Note: spec listed `src-tauri/src/polling/teams.rs` but repo has no such file; blocking impl lives in `src-tauri/src/teams.rs` per Main/peer confirmation. Scope limited to `teams.rs`.

## Per-issue

### #216 — `poll_teams_auth` blocks main thread, untrusted interval, RFC 8628 pacing
**What:** Converted `poll_teams_auth` in `commands/teams_auth.rs` to `pub async fn` with body inside `tauri::async_runtime::spawn_blocking`. Clamped interval on entry: `let interval = interval.clamp(1, 15)` (visible in both command and helper). Helper in `teams.rs` now clamps identically, bounds total polling by `900s` timeout, and caps each `thread::sleep` chunk to `min(30)` with timeout re-check between chunks. `slow_down` increments `wait` via `next_poll_wait(wait, "slow_down")` (+5s cumulative). Terminal errors (`expired_token`, `authorization_declined`, `bad_verification_code`, `unauthorized_client`, and `_`) return immediately per RFC 3.5.

**Why:** Previously `pub fn` with `thread::sleep(wait)` honoured attacker-controlled `u64::MAX` via JS devtools and blocked the Tauri main thread for up to 15 min. RFC 8628 §3.5 requires `slow_down` to increase interval and all non-pending errors to stop.

### #223 — missing `teams-auth-failed` event
**What:** Added `let _ = app.emit("teams-auth-failed", e.clone())` in `start_teams_auth_device_code` on `Err` before returning, and in `poll_teams_auth` on every `Err` outcome (timeout/expired/declined/bad_verification/unauthorized/generic). Payload is human-readable error string as expected by `Onboarding/Settings/Reconnect/useAuthListeners`.

**Why:** Four frontend listeners expected `teams-auth-failed` with string payload but backend never emitted it; only `teams-reconnect-required` was emitted from polling loop (different semantic). Kept separate — no double-emit for polling loop failures.

### #215 — (bundled) RFC alignment
Clamp + capped sleep + `slow_down` + terminal mapping together address #215 pacing correctness.

## Risks
- `spawn_blocking` now owns the 900s blocking loop; Tauri async runtime must be available (it is — `is_onboarding_complete` precedent). Poll still blocking inside the dedicated pool, not the main thread.
- `interval.clamp(1,15)` tightens upper bound; Microsoft docs default `interval` 5s, so 15s cap is safe. Chose 1..15 over 1..60 to match issue example.
- Chunked sleep changes timing for large `wait` values (post-`slow_down` ramps) but preserves total wait while respecting overall deadline.

## Grep evidence
```
src-tauri/src/commands/teams_auth.rs:23:  let _ = app.emit("teams-auth-failed", e.clone());
src-tauri/src/commands/teams_auth.rs:43:  pub async fn poll_teams_auth(
src-tauri/src/commands/teams_auth.rs:51:  let interval = interval.clamp(1, 15);
src-tauri/src/commands/teams_auth.rs:59:  tauri::async_runtime::spawn_blocking
src-tauri/src/commands/teams_auth.rs:92:  let _ = app.emit("teams-auth-failed", err_string.clone());
src-tauri/src/teams.rs:278:                let interval = interval.clamp(1, 15);
src-tauri/src/teams.rs:347:                wait = next_poll_wait(wait, error_resp.error.as_str());
src-tauri/src/teams.rs:350,367:            let chunk = remaining.min(30);
src-tauri/src/teams.rs:381:                "bad_verification_code" =>
src-tauri/src/teams.rs:389:                "unauthorized_client" =>
```

## Verification
- Grep confirms `pub async fn poll_teams_auth`, clamp line, `spawn_blocking`, `teams-auth-failed` in both Err paths, `slow_down` increment, capped chunks.
- No `ts-rs` type exports touched; invoke arg names (`device_code`, `interval`) unchanged.
- Log tags retained `[CMD.TEAMS_AUTH]`.
- Skipped cargo/npm gates per lane constraints; orchestrator verifies.
