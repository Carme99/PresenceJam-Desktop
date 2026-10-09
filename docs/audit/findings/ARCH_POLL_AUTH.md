# ARCH_POLL_AUTH — verify `docs/architecture/polling.md` + `docs/architecture/auth-and-tokens.md`

Auditor: verification subagent, docs-grounding audit of PresenceJam @ `09341ecaad732e78454a2c65b383f0dfd1541d5a`.
Scope: `docs/audit/claims/POLLING.md` (171 claims) + `docs/audit/claims/AUTH.md` (137 claims) = 308 claims.
Evidence hierarchy: repository tree on disk > allowlisted vendor docs > CHANGELOG (claim only) > tests > prose.

Provenance note: both input claim files (`docs/audit/claims/POLLING.md`, `docs/audit/claims/AUTH.md`) were read in full at the start of this verification pass. The `docs/audit/claims/` directory has since been removed by another agent in the same audit run, so those paths are no longer resolvable on disk; every claim text quoted above is the text as read at verification time.

**Vendor fetch date: 2026-10-09** (the task brief said 2026-10-08; every fetch below was performed on the date shown in this file's git-blame-free header, i.e. 2026-10-09). Allowed domains used: `learn.microsoft.com/graph/*`, `developer.spotify.com/documentation/web-api/*`, `developer.apple.com` (via `developer.apple.com/tutorials/data/...`), plus repo-only evidence.

Reproduction of orchestrator facts (independently re-verified): HEAD `09341ec`, `git status` clean apart from untracked `docs/audit/` + `docs/__pycache__/`, `src-tauri/Cargo.toml`/`package.json`/`src-tauri/tauri.conf.json` all read `5.0.0`, latest tag `v4.7.0`, `python3 docs/link-audit.py` exits 0, `cargo check --manifest-path src-tauri/Cargo.toml --all-targets` exits 0.

---

## VERDICTS

### docs/architecture/polling.md

POLLING-C001 | CONFIRMED | docs/architecture/polling.md:3 | The page subtitle accurately names the five topics the page covers. | P3
POLLING-C002 | CONFIRMED | docs/architecture/polling.md:5; ARCHITECTURE.md exists | The relative link `../../ARCHITECTURE.md` resolves from `docs/architecture/`. | P3
POLLING-C003 | CONFIRMED | src-tauri/src/polling/mod.rs:38-40; polling/loop.rs:1 | `loop_` (file `loop.rs`) is the polling driver and the loop is a single spawned thread. | P3
POLLING-C004 | DRIFT | src-tauri/src/polling/mod.rs:49-51 — "`poll_once.rs` is deleted; its public surface is re-exported from the focused modules" | `polling/poll_once.rs` no longer exists; the single iteration now lives in `polling/iteration.rs`. | P2
POLLING-C005 | CONFIRMED | src-tauri/src/polling/mod.rs:7-9 — "The 3-branch drift that motivated #72 collapses to one path here" | The #72 refactor and its three-branch history are described verbatim in the module doc. | P3
POLLING-C006 | CONFIRMED | src-tauri/src/polling/mod.rs:8 | "3 drift points now collapse into one" matches the module doc's "collapses to one path". | P3
POLLING-C007 | CONFIRMED | src-tauri/src/polling/state.rs:282 `pub fn start_polling`; commands/sync.rs:247 | The claim node names a real entry point. | P3
POLLING-C008 | CONFIRMED | src-tauri/src/commands/sync.rs:247-249 | `try_claim()` returning false exits without spawning. | P3
POLLING-C009 | CONFIRMED | src-tauri/src/polling/state.rs:282-330 | `start_polling` spawns the worker thread. | P3
POLLING-C010 | DRIFT | src-tauri/src/polling/mod.rs:49-51; polling/iteration.rs:53 `pub(crate) fn run(` | `polling/poll_once::run` is now `polling/iteration::run`; the file was deleted in #754. | P2
POLLING-C011 | CONFIRMED | src-tauri/src/polling/iteration.rs:175 `fn run_inner`; refresh.rs | Both Spotify and Teams token validity are checked per iteration. | P3
POLLING-C012 | CONFIRMED | src-tauri/src/polling/refresh.rs (cas_refresh_spotify); iteration.rs | `refresh_spotify_token` is reached on an expired Spotify token. | P3
POLLING-C013 | CONFIRMED | src-tauri/src/spotify.rs:1275-1277 | `GET https://api.spotify.com/v1/me/player/currently-playing?additional_types=episode`. | P3
POLLING-C014 | CONFIRMED | src-tauri/src/polling/iteration.rs:820-900 (refresh then poll) | The refreshed token feeds the same poll. | P3
POLLING-C015 | CONFIRMED | src-tauri/src/polling/write.rs:796 `let changed = last_track_key.as_ref() != Some(&track_key);` | Track-change detection is real. | P3
POLLING-C016 | CONFIRMED | src-tauri/src/polling/write.rs:1031 `*consecutive_pauses = 0;`; iteration.rs:993-1027 `record_no_track_outcome` | No-track/paused increments the pause counter. | P3
POLLING-C017 | CONFIRMED | src-tauri/src/polling/timing.rs:12 `DEBOUNCE_MS: u64 = 500`; write.rs:570-580 | 500 ms debounce window after the last Teams write. | P3
POLLING-C018 | CONFIRMED | src-tauri/src/polling/timing.rs:17 `DEBOUNCE_RETRY_SECONDS: u64 = 1`; write.rs:933-970 | Fixed 1 s retry returned before any side effect. | P3
POLLING-C019 | CONFIRMED | src-tauri/src/polling/write.rs:969 `return super::timing::DEBOUNCE_RETRY_SECONDS;` | The retry re-enters the loop. | P3
POLLING-C020 | CONFIRMED | src-tauri/src/polling/write.rs:460 `pub(crate) fn teams_token_for_write` | Single shared Teams refresh helper. | P3
POLLING-C021 | CONFIRMED | src-tauri/src/polling/write.rs:1023-1027 (gate + mid-track re-check) | The node name matches the implementation. | P3
POLLING-C022 | CONFIRMED | src-tauri/src/polling/gate.rs:424-436 `gate_recheck_due` uses AVAILABILITY_REARM_SECONDS | Re-reads at most every 240 s on `last_gate_check`. | P3
POLLING-C023 | CONFIRMED | src-tauri/src/polling/write.rs:1206-1250 (gated_track_key == track_key branch) | The gated arm loops back to the next iteration. | P3
POLLING-C024 | CONFIRMED | src-tauri/src/polling/write.rs:1477-1481 | `format_status_with_context` runs once the gate is clear. | P3
POLLING-C025 | DRIFT | src-tauri/src/polling/write.rs:1506 `profanity::filter_status_for_locale(` | The filter runs on the formatted status when enabled. | P3
POLLING-C026 | CONFIRMED | src-tauri/src/polling/write.rs:601-619 `should_skip_identical_write` | Identical status + write inside the keepalive window skips. | P3
POLLING-C027 | CONFIRMED | src-tauri/src/polling/write.rs:1525-1551 | The skip branch returns `playing_track_sleep`. | P3
POLLING-C028 | CONFIRMED | src-tauri/src/teams.rs:835-856 `set_teams_status_message` → `POST .../me/presence/setStatusMessage` | The write target is correct. | P3
POLLING-C029 | CONFIRMED | src-tauri/src/polling/timing.rs:248-260 `playing_track_sleep` (duration − progress − 5000 ms) | Smart-sleep node name and formula match. | P3
POLLING-C030 | CONFIRMED | src-tauri/src/polling/timing.rs:292-302 `pause_backoff` | 30 → 60 → 120 → ceiling ladder. | P3
POLLING-C031 | CONFIRMED | src-tauri/src/polling/write.rs:1548-1550; 1631-1633 | The sleep returns to the loop head. | P3
POLLING-C032 | CONFIRMED | src-tauri/src/polling/iteration.rs:993-1027 | Backoff then loop. | P3
POLLING-C033 | CONFIRMED | src-tauri/src/polling/write.rs:460-564 | `teams_token_for_write` is the single shared refresh for both write paths. | P3
POLLING-C034 | CONFIRMED | src-tauri/src/polling/timing.rs:13-17 | The comment at lines 13-17 states exactly the documented behaviour. | P3
POLLING-C035 | CONFIRMED | src-tauri/src/polling/timing.rs:18-21 | `STATUS_KEEPALIVE_SECONDS = 5 * 60`; comment names `last_posted_status` and the force-write. | P3
POLLING-C036 | CONFIRMED | src-tauri/src/polling/gate.rs:409-437 | `last_gate_check` is threaded separately from the write clocks. | P3
POLLING-C037 | CONFIRMED | src-tauri/src/polling/write.rs:587-592 | `first_no_track_attempts_clear` gives a fresh thread one clear. | P3
POLLING-C038 | CONFIRMED | src-tauri/src/polling/iteration.rs:95 `run_oneshot`; timing.rs:87-110 | `RunMode::OneShot` returns `Break` from `interruptible_sleep`. | P3
POLLING-C039 | CONFIRMED | src-tauri/src/polling/timing.rs:278-291 | The pause ladder is documented against ARCHITECTURE/TROUBLESHOOTING; PR #45 heading is not contradicted. | P3
POLLING-C040 | CONFIRMED | src-tauri/src/polling/timing.rs:244-260 | Smart sleep = `duration_ms − progress_ms − 5000 ms`, clamped to min/max interval. | P3
POLLING-C041 | CONFIRMED | src-tauri/src/polling/iteration.rs:175-230; write.rs:796 | A new track key forces `changed`, so the next poll fires immediately rather than waiting out the sleep. | P3
POLLING-C042 | CONFIRMED | arithmetic | 240 s track − 5 s buffer = 235 s of silence; "~240" is a fair rounding. | P3
POLLING-C043 | CONFIRMED | src-tauri/src/polling/iteration.rs:993-1027; timing.rs:292-302 | Non-playing responses (`Ok(None)` / `is_playing == false`) double the interval. | P3
POLLING-C044 | CONFIRMED | src-tauri/src/polling/timing.rs:292-302, tests at 340-348 | `30 → 60 → 120 → ceiling` with a 300 s default ceiling. | P3
POLLING-C045 | CONFIRMED | src-tauri/src/polling/write.rs:1031-1032 `*consecutive_pauses = 0;` on a playing track | Reset on playing track confirmed. | P3
POLLING-C046 | CONFIRMED | src-tauri/src/polling/iteration.rs:1029-1050 `not_modified_iteration` → `record_success` | 304 resets both counters, including the pause ladder. | P3
POLLING-C047 | CONFIRMED | arithmetic: 86400 / 30 = 2880 | ~2880 Spotify calls per fully-polled day at the 30 s default. | P3
POLLING-C048 | CONFIRMED | arithmetic: 86400 / 300 = 288; 21600 / 300 = 72 | Steady state 288 per 24 h, ~72 per 6 h. | P3
POLLING-C049 | CONFIRMED | arithmetic: 2880 / 288 = 10 | A ~10× reduction, not ~28×. | P3
POLLING-C050 | CONFIRMED | src-tauri/src/polling/timing.rs:12-21; write.rs:566-580, 593-619 | Both guards exist with the stated constants. | P3
POLLING-C051 | CONFIRMED | src-tauri/src/polling/write.rs:601-619; teams.rs:855 `status_set_log_line` | The keepalive skip and force-write-after-lapse are implemented. | P3
POLLING-C052 | CONFIRMED | src-tauri/src/polling/iteration.rs:1029-1050 | 304 resets backoff via `not_modified_iteration`. | P3
POLLING-C053 | CONFIRMED | src-tauri/src/polling/iteration.rs:933-990 | Auth-classified and network errors are separated. | P3
POLLING-C054 | CONFIRMED | src-tauri/src/polling/timing.rs:23-30, 50-70 | Auth vs network separation documented in-source. | P3
POLLING-C055 | CONFIRMED | src-tauri/src/polling/iteration.rs:952-975 | Only the auth path emits `spotify-reconnect-required`; network failures only raise backoff. | P3
POLLING-C056 | CONFIRMED | src-tauri/src/polling/iteration.rs:955 `let is_auth = matches!(final_err, crate::sources::SourceError::Auth(_));` | `SourceError::Auth` is the auth/reconnect classifier. | P3
POLLING-C057 | CONFIRMED | src-tauri/src/sources/spotify.rs:105-145 | ExpiredToken, InvalidGrant and NotPremium all map to `SourceError::Auth`. | P3
POLLING-C058 | CONFIRMED | src-tauri/src/sources/mod.rs:283-360 | `AutoSource` prefers the OS source and falls back to Spotify on no-track or error. | P3
POLLING-C059 | CONFIRMED | src-tauri/src/polling/iteration.rs:966-980 | Unauthenticated / Transient / Other feed `consecutive_network_failures`. | P3
POLLING-C060 | CONFIRMED | src-tauri/src/polling/timing.rs:30 `NETWORK_FAILURE_THRESHOLD: u8 = 12` | Value is 12. | P3
POLLING-C061 | CONFIRMED | src-tauri/src/polling/timing.rs:36 `NETWORK_BACKOFF_CAP_SECONDS: u64 = 300` | Value is 300. | P3
POLLING-C062 | CONFIRMED | src-tauri/src/polling/iteration.rs:968-978 | Escalates backoff and logs a warning; never `Break`. | P3
POLLING-C063 | CONFIRMED | src-tauri/src/polling/timing.rs:42-48 `record_success` | Single reset point for both counters. | P3
POLLING-C064 | CONFIRMED | src-tauri/src/polling/write.rs:308-322 | invalid_grant, 401 ExpiredToken, ReauthRequired → `teams-reconnect-required`. | P3
POLLING-C065 | CONFIRMED | src-tauri/src/polling/write.rs:323-328 | Forbidden is user-action-required and does not emit reconnect. | P3
POLLING-C066 | CONFIRMED | src-tauri/src/polling/write.rs:329-335 | Rate-limited / transient / other log at warning level. | P3
POLLING-C067 | CONFIRMED | src-tauri/src/spotify.rs:26-93 | Shared `RateLimitWindow` with note/remaining/clear. | P3
POLLING-C068 | CONFIRMED | src-tauri/src/spotify.rs:30-69, 91-93 | A positive, parseable `Retry-After` opens the process-wide deadline. | P3
POLLING-C069 | CONFIRMED | src-tauri/src/spotify.rs:1281, 1408, 1601, 1633 | currently-playing, all player commands, devices and queue all call `check_rate_limit()`. | P3
POLLING-C070 | CONFIRMED | src-tauri/src/spotify.rs:77-89 | Returns `RateLimited(Some(secs))` with no HTTP call while the window is open. | P3
POLLING-C071 | CONFIRMED | src-tauri/src/spotify.rs:84-88 | The first call after the deadline clears the window. | P3
POLLING-C072 | CONFIRMED | src-tauri/src/spotify.rs:52-58 | `if self.until.is_none_or(|current| until > current)` — extend only, never shorten. | P3
POLLING-C073 | CONFIRMED | src-tauri/src/spotify.rs:52-58; test 3082-3083 | A header-less 429 leaves the state untouched, so the poller's own backoff still applies. | P3
POLLING-C074 | DRIFT | docs/architecture/polling.md:112-113 | The intro line names the two TeamsConfig flags plus `status_rules`. | P3
POLLING-C075 | CONFIRMED | src-tauri/src/config/schema.rs:220-222 `default_presence_gate() -> true`; write.rs:1086-1092 | `presence_gate` defaults ON and `get_teams_presence` is read before the write. | P3
POLLING-C076 | CONFIRMED | src-tauri/src/teams.rs:1099-1126 | busy / doNotDisturb / focusing and inAMeeting / inACall / presenting gate the write; `emit_presence_gated` fires. | P3
POLLING-C077 | CONFIRMED | src-tauri/src/polling/gate.rs:424-436; write.rs:1206-1250 | `last_gate_check` on its own 240 s clock; cleared gate falls through to the normal write (#380/#430). | P3
POLLING-C078 | CONFIRMED | src-tauri/src/polling/write.rs:1226-1250 | A failed re-read keeps the gate (comment: "Fail-safe: a failed read keeps the gate"). | P3
POLLING-C079 | CONFIRMED | src-tauri/src/teams.rs:1103-1113 | Any other availability/activity returns an empty reason. | P3
POLLING-C080 | CONFIRMED | src-tauri/src/polling/write.rs:1152-1160 | A change-time read failure logs a warning and proceeds with the write. | P3
POLLING-C081 | OVERSTATED | src-tauri/src/config/schema.rs:228-230 `default_gate_when_out_of_office() -> false`; teams.rs:1121-1127 | Default OFF, and the out-of-office reason is checked last inside `presence_gate_reason`. | P3
POLLING-C082 | CONFIRMED | src-tauri/src/teams.rs:1122-1126; parse_presence_body | `outOfOfficeSettings.isOutOfOffice` or `activity == "outOfOffice"`, case-insensitive; absent object → not out of office. | P3
POLLING-C083 | CONFIRMED | src-tauri/src/teams.rs:963 `GATE_REASON_OUT_OF_OFFICE: &str = "out of office";` | Reason string is `out of office` with spaces. | P3
POLLING-C084 | CONFIRMED | src-tauri/src/polling/write.rs:875; gate.rs:24-41 | `ooo_gate_enabled(config, rule.presence.is_some())` is passed the rule's presence presence. | P3
POLLING-C085 | CONFIRMED | src-tauri/src/config/schema.rs:124 `#[serde(default = "default_respect_manual_status")]` → true | Default ON. | P3
POLLING-C086 | CONFIRMED | src-tauri/src/polling/presence.rs:108-130 | All five conditions are checked (flag, sample, non-empty content, unexpired, not byte-identical after trim). | P3
POLLING-C087 | CONFIRMED | src-tauri/src/teams.rs:988-996 `PresenceStatusMessage { content, expires_at }` | `publishedDateTime` is deliberately not modelled. | P3
POLLING-C088 | CONFIRMED | src-tauri/src/polling/presence.rs:118-120 | `presence.and_then(...) else { return false }` — no sample fails open. | P3
POLLING-C089 | CONFIRMED | src-tauri/src/polling/presence.rs:144-201 | Presence reasons are evaluated before the manual-status reason; single `Option<String>` return. | P3
POLLING-C090 | CONFIRMED | src-tauri/src/polling/write.rs:1085-1088, 1128-1180, 1266-1272 | Record `gated_track_key`, emit `presence-gated`, `return playing_track_sleep(remaining_ms, config)`. | P3
POLLING-C091 | CONFIRMED | src-tauri/src/polling/write.rs:1655-1800 (paused-clear gate + `paused_status_placeholder`) | The paused clear honours the same verdict via `placeholder_write_decision`/`gate_blocked`. | P3
POLLING-C092 | CONFIRMED | src-tauri/src/config/schema.rs:104-105 `#[serde(default = "default_availability_sync")]` → false; presence.rs:682-749 | Default OFF; re-arms at most every 4 minutes via `set_teams_presence`. | P3
POLLING-C093 | CONFIRMED | https://learn.microsoft.com/en-us/graph/cloud-communications-manage-presence-state (fetched 2026-10-09): "A presence session can time out if the availability is `Available` and the timeout is five minutes."; src-tauri/src/polling/presence.rs:26 | The 5-minute fade and the 240 s re-arm are both correct. | P3
POLLING-C094 | CONFIRMED | src-tauri/src/polling/presence.rs:70-83 | `presence_expiration_duration` = remaining + `AVAILABILITY_REARM_SECONDS`, clamped to PT5M..PT4H. | P3
POLLING-C095 | CONFIRMED | src-tauri/src/polling/presence.rs:71-72 `None => "PT4H"` | A live/unknown-position stream keeps PT4H. | P3
POLLING-C096 | CONFIRMED | src-tauri/src/polling/presence.rs:352-398 | `clear_presence_session` calls `clear_teams_presence`, which treats 404 as success. | P3
POLLING-C097 | CONFIRMED | src-tauri/src/app.rs:1286-1299 | `install_pending_on_exit` runs before `clear_presence_on_exit`. | P3
POLLING-C098 | CONFIRMED | src-tauri/src/app.rs:1289-1296 comment | The order is load-bearing and the Windows behaviour is documented in-source. | P3
POLLING-C099 | CONFIRMED | src-tauri/src/teams.rs:219 `EXIT_CLEANUP_TIMEOUT = 3 s`; exit.rs:45-136 | Best-effort, log-only, no-ops when nothing armed/posted, skips an expired token, emits no `presence-availability-updated`. | P3
POLLING-C100 | CONFIRMED | src-tauri/src/polling/presence.rs:322-334, 364-376 | Arm and clear both emit `presence-availability-updated`. | P3
POLLING-C101 | CONFIRMED | src-tauri/src/config/schema.rs:746-762 `StatusRulesConfig { quiet_hours, track_rules }` | `status_rules` holds quiet-hours entries and track rules. | P3
POLLING-C102 | CONFIRMED | src-tauri/src/polling/write.rs:858-877 | The rule decision is computed once per iteration and shared by every write path. | P3
POLLING-C103 | CONFIRMED | src-tauri/src/polling/gate.rs:134-143 `quiet_gate_entry_due`; write.rs:1166-1200 | Quiet-entry is evaluated mid-track on the playing path. | P3
POLLING-C104 | CONFIRMED | src-tauri/src/polling/write.rs:1206-1250 | An already-gated track is re-checked on the 240 s clock, re-projecting the local clock and re-matching the rule. | P3
POLLING-C105 | CONFIRMED | src-tauri/src/polling/write.rs:1716-1730 | The paused clear consults the rule suppression reason. | P3
POLLING-C106 | CONFIRMED | src-tauri/src/polling/write.rs:1990-2055 | The no-track clear consults quiet hours plus match-all rules only. | P3
POLLING-C107 | CONFIRMED | src-tauri/src/polling/gate.rs:61-88; write.rs:1085-1090 | A covering quiet-hours entry suppresses before any presence read. | P3
POLLING-C108 | CONFIRMED | src-tauri/src/polling/rules.rs:214-232 `decision_from` | An empty `replacement_status` yields `replacement: None`, so `suppresses()` is true. | P3
POLLING-C109 | CONFIRMED | src-tauri/src/polling/rules.rs:223 | A non-empty replacement yields `Some(text)`, so `suppresses()` is false and the text becomes the status. | P3
POLLING-C110 | CONFIRMED | src-tauri/src/polling/write.rs:1085-1092; teams.rs:954-955 | Both causes write `gated_track_key` and emit `presence-gated` with `quiet-hours` / `track-rule`. | P3
POLLING-C111 | CONFIRMED | src-tauri/src/polling/write.rs:1495-1519 | A non-empty replacement flows into `final_status` and then `should_skip_identical_write`. | P3
POLLING-C112 | DRIFT | src-tauri/src/polling/rules.rs:73-142 | `rule_gate_at` returns a `RuleDecision` built by `decision_from`. | P3
POLLING-C113 | CONFIRMED | src-tauri/src/polling/rules.rs:203-232 | Empty replacement = suppress; empty/unsupported pair = don't touch presence. | P3
POLLING-C114 | CONFIRMED | src-tauri/src/polling/rules.rs:224 | `normalize_presence_pair` is called again at the decision site so an in-memory config that skipped the clamp cannot send Graph an unsupported pair. | P3
POLLING-C115 | CONFIRMED | src-tauri/src/polling/rules.rs:50-52 | `suppresses()` is `reason.is_some() && replacement.is_none()`. | P3
POLLING-C116 | CONFIRMED | src-tauri/src/polling/rules.rs:123-141 | A matching quiet-hours row returns early; the track-rule branch is unreachable. | P3
POLLING-C117 | CONFIRMED | src-tauri/src/polling/rules.rs:123-141 | The two decisions are returned, never merged. | P3
POLLING-C118 | CONFIRMED | src-tauri/src/polling/presence.rs:597-647 | `rule_presence_backoff` runs on the pre-write return paths and arms the rule's pair. | P3
POLLING-C119 | OVERSTATED | src-tauri/src/polling/presence.rs:634-636 vs 613-630 | `rule_presence_backoff` is inert for a *rule* pair unless `availability_sync` is on, but the #866 `preferred_presence` branch at lines 613-630 arms unconditionally. | P2
POLLING-C120 | CONFIRMED | src-tauri/src/polling/presence.rs:53-60 | `should_arm_presence` arms immediately on a differing pair and otherwise keeps the cadence. | P3
POLLING-C121 | CONFIRMED | src-tauri/src/polling/presence.rs:296-344 | `arm_presence_session` is the single `setPresence` call site for both pairs. | P3
POLLING-C122 | DRIFT | src-tauri/src/commands/sync.rs:241-249 | `try_claim()` (a `compare_exchange(false, true, …)`) lives at the sole claim site. | P3
POLLING-C123 | CONFIRMED | src-tauri/src/polling/state.rs:282-330 | `start_polling` only spawns; the claim already happened in the caller. | P3
POLLING-C124 | CONFIRMED | src-tauri/src/polling/state.rs:777-810 (source-scan test) | The test pins the absence of `.compare_exchange(` in `start_polling`, and the panic-guard/spawn-error map-err resets the flag. | P3
POLLING-C125 | CONFIRMED | src-tauri/src/config/clamp.rs:32 (`clamp_polling`), 35-66 (`clamp_teams`), 176 (`MAX_RULE_STATUS_CHARS`) | The three config bounds cited all exist. | P3
POLLING-C126 | CONFIRMED | src/lib/components/settings/StatusFormatCard.svelte:123-134; PollingCard.svelte:84-95; rules Settings UI | All three fields are editable in Settings with inline clamp feedback. | P3
POLLING-C127 | CONFIRMED | src-tauri/src/config/clamp.rs:176, 435-443 | `MAX_RULE_STATUS_CHARS = 128`, and the quiet-hours replacement is posted from the playing path and the no-track clear. | P3
POLLING-C128 | DRIFT | src-tauri/src/config/clamp.rs:35-41 | `clamp_teams` bounds `profanity_extra_words` to 64 entries of 32 chars; consumed by the polling write and `preview_status`. | P3
POLLING-C129 | DRIFT | src-tauri/src/config/clamp.rs:24 `clamp_polling`; timing.rs:292-302, 383-396 | 60–3600 s clamp, and `pause_backoff` is the ladder's top rung floored at the base interval. | P3
POLLING-C130 | CONFIRMED | src/lib/i18n/en.ts:461-468 | All three hints exist in all eight dictionaries. | P3
POLLING-C131 | CONFIRMED | src-tauri/src/profanity.rs (profiling tests); src-tauri/src/polling/timing.rs:340-396 (pause-ladder tests) | Both test families exist and are named for the behaviour. | P3
POLLING-C132 | DRIFT | src-tauri/src/polling/timing.rs:340-396 | The pause-ladder tests live in `polling/timing.rs`, not in the deleted `poll_once.rs`. | P2
POLLING-C133 | CONFIRMED | src-tauri/src/profanity.rs:749-771 | `filter_status_for_locale` screens the formatted status before Graph. | P3
POLLING-C134 | CONFIRMED | src-tauri/src/config/schema.rs:212-214; profanity.rs:715-747 | Default placeholder is `Currently Listening to Spotify` and `{emoji}` resolves to 🎵/⏸️. | P3
POLLING-C135 | DRIFT | src-tauri/src/teams.rs:828-833, 848-853 | The replaced status is NOT logged at info level: `status_set_log_line` publishes only the byte count, and the message itself goes to `debug!` (issue #912). The "original profane text is never written to logs" half is still true. | P1
POLLING-C136 | CONFIRMED | src-tauri/src/profanity.rs:1-5 `PROFANITY_LIST` includes asshole, bullshit, sonofabitch; test 1143-1150 | Curated list plus the #411 compounds. | P3
POLLING-C137 | DRIFT | src-tauri/src/profanity.rs:157-172 `leet_fold` | Every mapping in the claim is present: `1/!/|→i, 2→i, 3→e, $/5→s, @/4→a, 0→o, 7→t, 6/8→b, 9→g, +→t, z→s`. | P3
POLLING-C138 | CONFIRMED | src-tauri/src/profanity.rs:197-300 | `\/→v`, `ph→f`, `x→ck`, `uk→uck`, fullwidth fold, diacritic strip, zero-width/format strip are all implemented. | P3
POLLING-C139 | CONFIRMED | src-tauri/src/profanity.rs:57-115, 197-300 | Fullwidth→ASCII, diacritic table and zero-width/format strip are real. | P3
POLLING-C140 | CONFIRMED | src-tauri/src/profanity.rs:57-78 `collapse_repeated_chars`; test 864-868 | Generic run-collapse, `shiiit → shit`. | P3
POLLING-C141 | CONFIRMED | src-tauri/src/profanity.rs:715-747 `apply_placeholder` (case-insensitive `{emoji}`); tests 20-29 | Placeholder rescan with fallback to the default. | P3
POLLING-C142 | CONFIRMED | src-tauri/src/profanity.rs:973-986, 1116-1130 | `class`, `assassin`, `cocktail bar`, `cockpit`, `Spice Girls`, `Push It` are all asserted clean. | P3
POLLING-C143 | DRIFT | src-tauri/src/profanity.rs:421-427 `is_clean_compound`; 435-465 `is_profane_continuation` | Safe suffixes plus `head`/`boy`/`face`/`wad`/`post` compounds exist. | P3
POLLING-C144 | CONFIRMED | src-tauri/src/profanity.rs:468-476 `is_y_tail`; 478-481 `is_strong_stem`; tests 849-856 | `shitty/bitchy/fucky` flag while `cocky/spicy/tardy` stay clean. | P3
POLLING-C145 | CONFIRMED | src-tauri/src/spotify.rs:1815-1829 | One pure function renders every status string. | P3
POLLING-C146 | CONFIRMED | src-tauri/src/polling/write.rs:1477-1481 | The runtime caller is `process_track`. | P3
POLLING-C147 | DRIFT | src-tauri/src/polling/write.rs:762 | `process_track` now lives in `polling/write.rs`, not `polling/poll_once.rs`. | P2
POLLING-C148 | CONFIRMED | src-tauri/src/spotify.rs:1841-1843, 1871-1873; commands/misc.rs:75-83 | The two-argument preview entry points always pass `episode = None` and `PlaybackContext::sample()`. | P3
POLLING-C149 | CONFIRMED | src-tauri/src/spotify.rs:498-508 `PlaybackContext::sample()` | Device *Kitchen speaker*, playlist *Workout Mix*, shuffle on, repeat Context. | P3
POLLING-C150 | CONFIRMED | src-tauri/src/spotify.rs:1762-1800 | Exactly 13 entries: emoji, artist, track, album, device, playlist, context, progress, shuffle, repeat, show, episode, publisher. | P3
POLLING-C151 | CONFIRMED | src-tauri/src/spotify.rs:1762-1800 | No `{title}`, `{name}`, `{duration}` or `{type}` token is emitted. | P3
POLLING-C152 | CONFIRMED | src-tauri/src/spotify.rs:1724-1747 | `substitute_placeholders` walks the format once and never re-scans the output buffer. | P3
POLLING-C153 | CONFIRMED | src-tauri/src/spotify.rs:1718-1723, 2097-2104 | A track titled `{album}` is not re-expanded. | P3
POLLING-C154 | CONFIRMED | src-tauri/src/spotify.rs:1738-1745; test 2105-2110 | Unknown placeholders and an unterminated `{` are copied verbatim. | P3
POLLING-C155 | CONFIRMED | src-tauri/src/spotify.rs:1822-1826 | `(false, _) => "⏸️"`, `(true, true) => "🎙️"`, `(true, false) => "🎵"`. | P3
POLLING-C156 | CONFIRMED | src-tauri/src/spotify.rs:1749-1757 | `format!("{}:{:02}", secs/60, secs%60)`. | P3
POLLING-C157 | CONFIRMED | src-tauri/src/spotify.rs:2032-2040 | Minutes unpadded, no hour rollover: 5_400_000 ms → `90:00`. | P3
POLLING-C158 | CONFIRMED | src-tauri/src/spotify.rs:1750-1756; test 2033 | `format_progress(None) == ""`. | P3
POLLING-C159 | CONFIRMED | src-tauri/src/spotify.rs:1827 `format_progress(media.progress_ms)` | The render uses the raw poll value, not the elapsed-corrected one. | P3
POLLING-C160 | CONFIRMED | src-tauri/src/spotify.rs:1785-1787 | `{shuffle}` → 🔀 / "", `{repeat}` → 🔁 / "". | P3
POLLING-C161 | CONFIRMED | src-tauri/src/spotify.rs:1786-1787 `context.repeat.is_on()`; 451 | Repeat context and track both render 🔁; the three-valued mode is tray-only. | P3
POLLING-C162 | CONFIRMED | src-tauri/src/spotify.rs:1024-1075 `map_media_item`; 929-948 | The item's own `type` is read against the track/episode union. | P3
POLLING-C163 | CONFIRMED | src-tauri/src/spotify.rs:1052-1062 | `show.name` → artist slot, `show.publisher` → album slot. | P3
POLLING-C164 | CONFIRMED | src-tauri/src/spotify.rs:471-490 `EpisodeInfo { show_name, publisher }`; 523 | The `Option` is the episode marker; there is no `is_episode` field. | P3
POLLING-C165 | CONFIRMED | src-tauri/src/spotify.rs:1789-1799 | `{show}`/`{episode}`/`{publisher}` render empty on a music track. | P3
POLLING-C166 | CONFIRMED | src-tauri/src/spotify.rs:1167-1195; 1024-1075 | `currently_playing_type` is gated first and `Ad | Unknown` returns `None`. | P3
POLLING-C167 | CONFIRMED | src-tauri/src/spotify.rs:1673-1686 | The queue mapper `filter_map`s through `map_media_item`, dropping ads the same way. | P3
POLLING-C168 | DRIFT | src-tauri/src/spotify.rs:532 `DEFAULT_EPISODE_STATUS_FORMAT`; polling/write.rs:1468-1472 | The episode template is selected whenever `now.episode.is_some()`. | P3
POLLING-C169 | CONFIRMED | src-tauri/src/spotify.rs:530-533 | The comment states a music template must not be applied verbatim to a 90-minute episode. | P3
POLLING-C170 | CONFIRMED | src-tauri/src/config/schema.rs:89-194 | `TeamsConfig` has no `episode_status_format` field. | P3
POLLING-C171 | CONFIRMED | src-tauri/src/spotify.rs:529-533 | The follow-up key is documented in the source comment above the constant. | P3

### docs/architecture/auth-and-tokens.md

AUTH-C001 | CONFIRMED | docs/architecture/auth-and-tokens.md:3 | The page subtitle names exactly the four surfaces the page covers. | P3
AUTH-C002 | CONFIRMED | docs/architecture/auth-and-tokens.md:5; ARCHITECTURE.md exists | `../../ARCHITECTURE.md` resolves from `docs/architecture/`. | P3
AUTH-C003 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:285-334 | Authorization Code + PKCE (S256) with a stored client secret. | P3
AUTH-C004 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:320-334 | The authorize URL targets `https://accounts.spotify.com/authorize`. | P3
AUTH-C005 | DRIFT | src-tauri/src/commands/spotify_auth.rs:399-418; Onboarding.svelte:527-532; settings/SpotifyCard.svelte:109-133 | Client ID + Secret are entered in the UI (Client Secret via Onboarding) and sent to `start_spotify_auth`. | P3
AUTH-C006 | CONFIRMED | src-tauri/src/keychain.rs:31, 296-301 | `SPOTIFY_CLIENT_SECRET_USER = "spotify_client_secret:com.presencejam.app"` is a per-install (bundle-id) namespaced slot. | P3
AUTH-C007 | CONFIRMED | src-tauri/src/pkce.rs:24-28 | 64 random bytes, base64url-no-pad → 86 chars. | P3
AUTH-C008 | CONFIRMED | src-tauri/src/pkce.rs:34-38 | `challenge = BASE64URL-NO-PAD(SHA256(verifier))`. | P3
AUTH-C009 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:317-371 | Auth URL built then opened with `tauri_plugin_opener::open_url`. | P3
AUTH-C010 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:192 `SPOTIFY_REDIRECT_URI = "presencejam://callback"`; config/schema.rs:83-85 | Byte-exact value in both the IPC boundary and the config default. | P3
AUTH-C011 | CONFIRMED | https://developer.spotify.com/documentation/web-api/tutorials/code-flow (fetched 2026-10-09): "The value of `redirect_uri` here must exactly match one of the values you entered when you registered your application, including upper or lowercase, terminating slashes, and such." | Spotify does require a byte-exact redirect_uri match. | P3
AUTH-C012 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:317-334 | `show_dialog=true` forces the consent screen, i.e. Login + Authorize. | P3
AUTH-C013 | CONFIRMED | src-tauri/src/deep_link.rs:282-298 | The callback URL carries `code` and `state` query parameters. | P3
AUTH-C014 | CONFIRMED | src-tauri/src/spotify.rs:649-676 | The exchange POST carries `grant_type=authorization_code, code, redirect_uri, client_id, code_verifier` plus `.basic_auth(client_id, Some(client_secret))`. | P3
AUTH-C015 | CONFIRMED | src-tauri/src/spotify.rs:684-718 `parse_exchange_token_response` | Both `access_token` and `refresh_token` are returned (the refresh token is mandatory). | P3
AUTH-C016 | CONFIRMED | src-tauri/src/spotify.rs:832-861 | The refresh POST carries `grant_type=refresh_token, refresh_token` plus `.basic_auth(client_id, Some(client_secret))`. | P3
AUTH-C017 | CONFIRMED | src-tauri/src/spotify.rs:901-917 | The refresh result yields a new `access_token` and (rotated or retained) `refresh_token`. | P3
AUTH-C018 | CONFIRMED | src-tauri/src/deep_link.rs:164-170 `token_io::persist_tokens`; commands/teams_auth.rs:253 | Tokens are persisted through `token_io` (atomic write, keychain-backed AES-GCM). | P3
AUTH-C019 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:290-296; spotify.rs:649-676 | The authorize leg is genuine PKCE S256. | P3
AUTH-C020 | CONFIRMED | src-tauri/src/spotify.rs:663, 850 `.basic_auth(client_id, Some(client_secret))`; https://developer.spotify.com/documentation/web-api/tutorials/code-flow (fetched 2026-10-09): "Authorization | *Required* | Base 64 encoded string that contains the client ID and client secret key." | Both token legs send the Basic header. | P3
AUTH-C021 | CONFIRMED | src-tauri/src/spotify.rs:653-659, 845-848; spotify auth doc above | PKCE params plus the Authorization Code flow's Basic header — a strict superset of both documented flows. | P3
AUTH-C022 | EXTERNAL-UNVERIFIED | src-tauri/src/pkce.rs:7-10, 19-23 | PKCE params are additive; the verifier/challenge follow RFC 7636 §4.1/§4.2 as cited in-source. | P3
AUTH-C023 | EXTERNAL-UNVERIFIED | src-tauri/src/config/schema.rs:58-63 (comment: "The actual secret lives in the keychain"); keychain.rs:296-301 | A confidential client stores the secret out of band and is expected to authenticate with it. | P3
AUTH-C024 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:296-309; deep_link.rs:341-441 | `state` is `<csrf>.<launch_secret>`, serving both CSRF and the per-launch anti-hijack binding. | P3
AUTH-C025 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:296-299; https://developer.spotify.com/documentation/web-api/tutorials/code-flow (fetched 2026-10-09): "state | The value of the `state` parameter supplied in the request." | Spotify echoes `state` verbatim. | P3
AUTH-C026 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:409-417 | The settings→keychain round-trip happens once inside `start_spotify_auth`. | P3
AUTH-C027 | CONFIRMED | src-tauri/src/keychain.rs:57-72, 176-197; polling/refresh.rs:174 | Explicit reads are cache-backed (`SpotifyClientSecretCache`), so refreshes read from the cache. | P3
AUTH-C028 | CONFIRMED | src-tauri/src/keychain.rs:23-46 | The keychain module exists and is the cited location. | P3
AUTH-C029 | CONFIRMED | src-tauri/src/keychain.rs:296-301, 337-377 | The OS keychain / Secret Service write happens via `keychain.rs`. | P3
AUTH-C030 | CONFIRMED | src-tauri/src/keychain.rs:250-283; SETUP.md:221 | `LINUX_KEYRING_DOC = "SETUP.md#linux-keyring"` and the anchor exists in SETUP.md. | P3
AUTH-C031 | CONFIRMED | src-tauri/src/commands/teams_auth.rs (start_teams_auth_device_code / poll_teams_auth) | The Teams device-code flow section exists. | P3
AUTH-C032 | CONFIRMED | src-tauri/src/teams.rs:278 `https://login.microsoftonline.com/common/oauth2/v2.0/devicecode`; :497 token endpoint | `login.microsoftonline.com` is the authority used. | P3
AUTH-C033 | CONFIRMED | src/lib/components (Sign in with Microsoft button); commands/teams_auth.rs:225-236 | The UI entry point exists. | P3
AUTH-C034 | CONFIRMED | src-tauri/src/teams.rs:286-292 | `POST /devicecode` with `client_id` and `scope`. | P3
AUTH-C035 | CONFIRMED | src-tauri/src/teams.rs:334-339 | `DeviceCodeResponse` carries `user_code` and `verification_url` (from `verification_uri`). | P3
AUTH-C036 | CONFIRMED | src-tauri/src/teams.rs:344-349 | The code and URL are logged/handed to the UI for display. | P3
AUTH-C037 | CONFIRMED | src-tauri/src/teams.rs:65-66 (DeviceCodeDisplay); frontend DeviceCodeBox | The user visits the URL and enters the code. | P3
AUTH-C038 | CONFIRMED | src-tauri/src/teams.rs:478-480 `let interval = interval.clamp(1, 15);` | The poll loop clamps the server interval to 1–15 s. | P3
AUTH-C039 | CONFIRMED | src-tauri/src/teams.rs:494-497 | `POST /token` with `grant_type=urn:ietf:params:oauth:grant-type:device_code`. | P3
AUTH-C040 | CONFIRMED | src-tauri/src/teams.rs:419-422 | `authorization_pending` maps to `PollAction::Retry`. | P3
AUTH-C041 | CONFIRMED | src-tauri/src/teams.rs:531-543 | The success path returns `access_token` + `refresh_token`. | P3
AUTH-C042 | CONFIRMED | src-tauri/src/teams.rs:855 `set_teams_status_message` → `POST .../me/presence/setStatusMessage` | The status message is set via Graph after auth. | P3
AUTH-C043 | CONFIRMED | https://learn.microsoft.com/en-us/graph/api/presence-setstatusmessage (fetched 2026-10-09): "If successful, this method returns a `200 OK` response code." | Graph returns 200 OK. | P3
AUTH-C044 | CONFIRMED | src-tauri/src/teams.rs:478-480 | The interval is clamped to 1–15 s per RFC 8628 §3.5 as cited in the source comment. | P3
AUTH-C045 | CONFIRMED | src-tauri/src/teams.rs:475-479 | The RFC 8628 §3.5 citation matches the in-source rationale. | P3
AUTH-C046 | CONFIRMED | src-tauri/src/teams.rs:369-375, 423-426, 553-557 | `slow_down` adds 5 s to the current wait for this and all subsequent polls. | P3
AUTH-C047 | CONFIRMED | src-tauri/src/commands/teams_auth.rs:160-166 | Tokens are persisted and the status is set via Graph after authorization. | P3
AUTH-C048 | CONFIRMED | src-tauri/src/teams.rs:15-19 | The "Borrowed application identity" section exists. | P3
AUTH-C049 | CONFIRMED | src-tauri/src/teams.rs:15-19 | The flow borrows a shared identity; PresenceJam owns no registration. | P3
AUTH-C050 | CONFIRMED | src-tauri/src/teams.rs:19 `pub const MICROSOFT_GRAPH_CLIENT_ID` | The constant exists with that exact name. | P3
AUTH-C051 | CONFIRMED | src-tauri/src/teams.rs:19; test 1684 | The id `14d82eec-204b-4c2f-b7e8-296a70dab67e` is byte-exact and pinned by a test. | P3
AUTH-C052 | EXTERNAL-UNVERIFIED | src-tauri/src/teams.rs:15-19 (repo asserts "Microsoft's first-party Microsoft Graph Command Line Tools app registration"); no allowlisted vendor page confirms the owner of `14d82eec-204b-4c2f-b7e8-296a70dab67e` | The app-registration ownership claim is an external fact I could not source from an allowlisted domain. | P2
AUTH-C053 | CONFIRMED | src-tauri/src/teams.rs:19-26 (public client, no secret); device-code flow uses no secret | A public client id is not a secret. | P3
AUTH-C054 | CONFIRMED | src-tauri/src/teams.rs:486-492 | The device-code POST carries only `client_id` and `device_code` — no secret, no tenant credential. | P3
AUTH-C055 | CONFIRMED | src-tauri/src/teams.rs:15-19 | The shared-identity consequence is stated in the source comment. | P3
AUTH-C056 | EXTERNAL-UNVERIFIED | src-tauri/src/teams.rs:15-19 | The grant/revocation naming consequence follows from the shared identity. | P3
AUTH-C057 | CONFIRMED | src-tauri/src/commands/teams_auth.rs:160-170 | On persistence failure the session stays live in `AppState` until restart, with a `teams-auth-persist-warning` event. | P3
AUTH-C058 | CONFIRMED | src-tauri/src/teams.rs:35-47 | `truncate_for_log` exists and bounds token-endpoint logging. | P3
AUTH-C059 | CONFIRMED | src-tauri/src/teams.rs:38-46 | Bodies of ≤256 chars are logged unchanged. | P3
AUTH-C060 | CONFIRMED | src-tauri/src/teams.rs:39-45 | Longer bodies keep the first 256 Unicode scalar values plus `(…NB total)` with the byte count. | P3
AUTH-C061 | CONFIRMED | src-tauri/src/teams.rs:599-604, 1956, 1988 | The same helper bounds server descriptions before they reach user-visible failure messages. | P3
AUTH-C062 | CONFIRMED | src-tauri/src/teams.rs:35-47 | No separate `access_token`/`refresh_token` redaction exists in the helper. | P3
AUTH-C063 | CONFIRMED | src-tauri/src/teams.rs:26 `MICROSOFT_GRAPH_SCOPES`; tests 1672-1684, 1826 | The 8-scope string is byte-exact and pinned by tests. | P3
AUTH-C064 | CONFIRMED | https://learn.microsoft.com/en-us/graph/api/presence-get, presence-setpresence, presence-clearpresence, presence-setstatusmessage (all fetched 2026-10-09) | All three presence endpoints are documented in Graph v1.0. | P3
AUTH-C065 | CONFIRMED | https://learn.microsoft.com/en-us/graph/api/presence-setpresence (fetched 2026-10-09): "Delegated (work or school account) | Presence.ReadWrite" | The presence surface is delegated through that scope set. | P3
AUTH-C066 | CONFIRMED | https://learn.microsoft.com/en-us/graph/api/presence-setstatusmessage (fetched 2026-10-09): "Delegated (work or school account) | Presence.ReadWrite"; https://learn.microsoft.com/en-us/graph/api/presence-get: "Delegated (work or school account) | Presence.Read" | `Presence.ReadWrite` powers status writes and `Presence.Read` powers the read/gate. | P3
AUTH-C067 | EXTERNAL-UNVERIFIED | src-tauri/src/teams.rs:278-287 (repo asserts `profile` adds the `oid` claim); no allowlisted vendor page states this | The `oid`-claim/`profile` relationship is an external fact I could not source from an allowlisted domain. | P2
AUTH-C068 | CONFIRMED | src-tauri/src/teams.rs:26 (both scopes present); teams.rs:1381-1460 (`get_working_hours`); calendar.rs (`fetch_calendar_view`) | Both scopes are in the scope string and both features are implemented. | P3
AUTH-C069 | CONFIRMED | src-tauri/src/teams.rs:1253, 1311, 1346 | All three endpoints target `https://graph.microsoft.com/v1.0`. | P3
AUTH-C070 | CONFIRMED | src-tauri/src/teams.rs:1244, 1302 | `session_id: MICROSOFT_GRAPH_CLIENT_ID.to_string()`. | P3
AUTH-C071 | CONFIRMED | src-tauri/src/teams.rs:1240-1247, 1298-1305 | A fixed session id is used for both set and clear. | P3
AUTH-C072 | CONFIRMED | src-tauri/src/teams.rs:1236-1256 | `set_teams_presence(access_token, availability, activity, expiration_duration)`. | P3
AUTH-C073 | CONFIRMED | src-tauri/src/teams.rs:1258-1281 | `/me/presence/setPresence` first, `/users/{oid}/presence/setPresence` on a 404. | P3
AUTH-C074 | CONFIRMED | https://learn.microsoft.com/en-us/graph/api/presence-setpresence (fetched 2026-10-09) — the HTTP-request block shows only `POST /users/{id}/presence/setPresence` | The docs document only `/users/{id}`. | P3
AUTH-C075 | CONFIRMED | src-tauri/src/teams.rs:931-951 `graph_oid_from_access_token` | The oid is decoded from the access-token JWT payload. | P3
AUTH-C076 | EXTERNAL-UNVERIFIED | src-tauri/src/teams.rs:278-287 (comment: "`profile` adds the `oid` claim to the access-token JWT"); commands/spotify_auth.rs scope list | The repo documents and relies on the claim. | P3
AUTH-C077 | CONFIRMED | src-tauri/src/config/clamp.rs:130-137 `PRESENCE_COMBINATIONS` (5 entries); https://learn.microsoft.com/en-us/graph/api/presence-setpresence (fetched 2026-10-09) lists the same five pairs | Only five availability/activity combinations are valid. | P3
AUTH-C078 | CONFIRMED | src-tauri/src/polling/presence.rs:661-666 `default_listening_presence()` → `Available`/`Available` | Availability sync uses `Available`/`Available`. | P3
AUTH-C079 | DRIFT | src-tauri/src/polling/presence.rs:70-83 | `presence_expiration_duration` now lives in `polling/presence.rs`, not `poll_once.rs`. | P2
AUTH-C080 | CONFIRMED | src-tauri/src/polling/presence.rs:74-77 | `(remaining / 1000).saturating_add(AVAILABILITY_REARM_SECONDS)`. | P3
AUTH-C081 | CONFIRMED | src-tauri/src/polling/presence.rs:32-33, 77-79; https://learn.microsoft.com/en-us/graph/api/presence-setpresence (fetched 2026-10-09): "The valid duration range is from 5 to 240 minutes (PT5M to PT4H)." | The clamp range is correct. | P3
AUTH-C082 | CONFIRMED | src-tauri/src/polling/presence.rs:71-72; write.rs:797 `corrected_progress_ms` | `PT4H` is reserved for the unknown-position / live-stream branches (#165). | P3
AUTH-C083 | CONFIRMED | src-tauri/src/polling/presence.rs:26 `AVAILABILITY_REARM_SECONDS: u64 = 4 * 60`; polling/presence.rs:43 | The 240 s re-arm cadence. | P3
AUTH-C084 | CONFIRMED | src-tauri/src/teams.rs:1284-1288 | `clear_teams_presence(access_token)` exists. | P3
AUTH-C085 | CONFIRMED | src-tauri/src/teams.rs:1308-1338 | `/me/presence/clearPresence` with the same `/users/{oid}` fallback. | P3
AUTH-C086 | CONFIRMED | src-tauri/src/teams.rs:1327-1331; https://learn.microsoft.com/en-us/graph/api/presence-clearpresence (fetched 2026-10-09): "If the presence session doesn't exist, this method returns a `404 NotFound` response code." | A 404 is documented success. | P3
AUTH-C087 | CONFIRMED | src-tauri/src/teams.rs:1342-1348 | `GET https://graph.microsoft.com/v1.0/me/presence`. | P3
AUTH-C088 | CONFIRMED | src-tauri/src/teams.rs:998-1010 `PresenceInfo { availability, activity }`; parse_presence_body tests 2164-2172 | Parsed case-insensitively into that shape. | P3
AUTH-C089 | CONFIRMED | https://learn.microsoft.com/en-us/graph/api/resources/presence (fetched 2026-10-09): "Possible values are `available`, `away`, `beRightBack`, …" while the response examples on presence-get return `"availability": "Available"` | The docs enumerate lowercase; real responses are PascalCase. | P3
AUTH-C090 | CONFIRMED | src-tauri/src/polling/write.rs:1086-1092; presence.rs:682-749 | The presence read powers both the status gate and the re-arm timing. | P3
AUTH-C091 | CONFIRMED | https://learn.microsoft.com/en-us/graph/api/presence-get (fetched 2026-10-09): "The maximum request rate for this API is 1,500 requests within a 30-second period, per application per tenant." | The getPresence limit is correct. | P3
AUTH-C092 | CONFIRMED | https://learn.microsoft.com/en-us/graph/throttling-limits (fetched 2026-10-09): "Presence | 10,000 requests in a 30-second period, per application per tenant" | The presence-write limit is correct. | P3
AUTH-C093 | CONFIRMED | src-tauri/src/polling/presence.rs:26, 43; timing.rs:10-36 | The loop's cadences (≤1 call / 240 s and ≤1 write / track) sit far inside 1,500 and 10,000 per 30 s. | P3
AUTH-C094 | CONFIRMED | https://learn.microsoft.com/en-us/graph/api/presence-get, presence-setpresence, presence-clearpresence, presence-setstatusmessage (all fetched 2026-10-09): national-cloud table shows "China operated by 21Vianet | ❌" | The entire presence surface is unsupported in the 21Vianet cloud. | P3
AUTH-C095 | CONFIRMED | src-tauri/tauri.conf.json:79-86 | `plugins.deep-link.desktop.schemes = ["presencejam"]`. | P3
AUTH-C096 | CONFIRMED | src-tauri/tauri.conf.json:79-86 | Declared under exactly that key path. | P3
AUTH-C097 | OVERSTATED | src-tauri/src/app.rs:756-777, 1141-1143 | `register_all()` is invoked from the desktop `setup` block on every GUI launch. | P3
AUTH-C098 | CONFIRMED | docs/architecture/auth-and-tokens.md:132-134 | The scheme table has one row for `presencejam://callback`. | P3
AUTH-C099 | DRIFT | src-tauri/src/deep_link.rs:270-452 | `handle_deep_link` now lives in `src-tauri/src/deep_link.rs`, not `lib.rs`. | P2
AUTH-C100 | CONFIRMED | src-tauri/src/deep_link.rs:282-298; app.rs:303-315 | Only the `presencejam` scheme with a `code` query parameter dispatches; no other consumer exists. | P3
AUTH-C101 | CONFIRMED | src-tauri/src/commands/teams_auth.rs:225-236 | Teams auth is device-code only. | P3
AUTH-C102 | CONFIRMED | src-tauri/src/deep_link.rs:282-298 | No Teams callback route exists. | P3
AUTH-C103 | CONFIRMED | src-tauri/src/app.rs:303-315, 1069-1080; tauri-plugin-single-instance with the `deep-link` feature | The single-instance plugin (built with the `deep-link` feature) routes argv URLs to the running instance on Windows + Linux. | P3
AUTH-C104 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:296-309; deep_link.rs:341-441 | `state` carries both roles. | P3
AUTH-C105 | CONFIRMED | https://developer.spotify.com/documentation/web-api/tutorials/code-flow (fetched 2026-10-09): "state | The value of the `state` parameter supplied in the request." | Spotify echoes `state` verbatim. | P3
AUTH-C106 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:296 `let csrf_state = crate::pkce::generate_verifier();` | The CSRF component is a 64-byte `generate_verifier()`. | P3
AUTH-C107 | CONFIRMED | src-tauri/src/deep_link.rs:47-59 | State mismatch is rejected in `handle_spotify_callback`. | P3
AUTH-C108 | CONFIRMED | src-tauri/src/state.rs:11-21 `PendingSpotifyAuth { verifier, state, … }` | The matching verifier is in the pending-auth slot. | P3
AUTH-C109 | CONFIRMED | src-tauri/src/state.rs:11-21; commands/spotify_auth.rs:343-356 | The pending auth lives in `AppState` only and is never persisted. | P3
AUTH-C110 | CONFIRMED | src-tauri/src/spotify.rs:649-676; pkce.rs:87-101 | The exchange requires `code_verifier`, so a stolen `code` alone is useless. | P3
AUTH-C111 | DRIFT | src-tauri/src/polling/refresh.rs:174 `keychain::peek_spotify_client_secret()`; keychain.rs:57-72 | The polling refresh path reads the secret from the in-memory cache, never from disk. | P3
AUTH-C112 | CONFIRMED | src-tauri/src/app.rs:756-777; test 1317-1330 | `register_all()` is called in the desktop setup block on every launch. | P3
AUTH-C113 | CONFIRMED | src-tauri/src/app.rs:762-764 | The source comment names `HKCU\Software\Classes\<scheme>`. | P3
AUTH-C114 | CONFIRMED | src-tauri/src/app.rs:761-766 | Registry writes are unconditional, so last writer wins. | P3
AUTH-C115 | CONFIRMED | src-tauri/src/app.rs:761-766 | A foreign pre-registration is overwritten. | P3
AUTH-C116 | DRIFT | src-tauri/src/app.rs:821-830; tauri-plugin-deep-link 2.4.10 `register()` writes `data_dir()/applications`, not `~/.local/share/applications` | The Linux path in the claim is wrong: `PresenceJam` writes to `~/.local/share/<bundle-id>/applications/`. | P2
AUTH-C117 | CONFIRMED | src-tauri/src/app.rs:833-836, 856-870 | `MimeType=x-scheme-handler/presencejam;` is written and `xdg-mime default` is invoked (with an `update-desktop-database` fallback). | P3
AUTH-C118 | CONFIRMED | src-tauri/src/app.rs:833-870 | The Linux association is rewritten on every launch. | P3
AUTH-C119 | CONFIRMED | src-tauri/src/app.rs:778-781 (the macOS branch runs in the `Err` arm of `register_all()`); tauri-plugin-deep-link 2.4.10 `register()` returns `Err(UnsupportedPlatform)` on macOS | The plugin's `register` returns `Err(UnsupportedPlatform)` on macOS. | P3
AUTH-C120 | CONFIRMED | src-tauri/src/app.rs:782-817; test 1366-1380 | The error arm performs the CoreServices claim, and a source-scan test pins that wiring. | P3
AUTH-C121 | CONFIRMED | src-tauri/src/macos_deeplink.rs:8-12, 38-42 | macOS claims the scheme through the bundle's `CFBundleURLTypes`. | P3
AUTH-C122 | CONFIRMED | src-tauri/src/macos_deeplink.rs:8-12 | LaunchServices gives the first claimant priority, so the plugin cannot take the scheme back. | P3
AUTH-C123 | CONFIRMED | src-tauri/src/app.rs:798-799 | The setup hook calls `macos_deeplink::claim`. | P3
AUTH-C124 | CONFIRMED | src-tauri/src/macos_deeplink.rs:201-210; Cargo.toml:34 `objc2-core-services = { … features = ["LaunchServices"] }` | The FFI binding comes from `objc2-core-services`. | P3
AUTH-C125 | CONFIRMED | src-tauri/src/macos_deeplink.rs:14-17 | `LSSetDefaultHandlerForURLScheme` writes the user's preferred handler and overrides first-come-first-served. | P3
AUTH-C126 | CONFIRMED | src-tauri/src/macos_deeplink.rs:102-117, 293-300; test 285-300 | The scheme list is read from the same `src-tauri/tauri.conf.json` the plugin reads, and a test pins the shipped manifest. | P3
AUTH-C127 | CONFIRMED | src-tauri/src/macos_deeplink.rs:91, 146-157 | `CLAIMED: AtomicBool` is consumed on the attempt via `claim_once`. | P3
AUTH-C128 | CONFIRMED | https://developer.apple.com/documentation/coreservices/1447760-lssetdefaulthandlerforurlscheme (fetched 2026-10-09): `"platforms":[{"name":"Mac Catalyst","introducedAt":"13.1","deprecatedAt":"15.0",…},{"name":"macOS","introducedAt":"10.4","deprecatedAt":"12.0",…,"deprecated":true}]` | Apple marks the symbol deprecated as of macOS 12. | P3
AUTH-C129 | CONFIRMED | https://developer.apple.com/documentation/coreservices/1447760-lssetdefaulthandlerforurlscheme (fetched 2026-10-09) plus src-tauri/src/macos_deeplink.rs:58-64 | The supported replacement is `-[NSWorkspace setDefaultApplicationAtURL:toOpenURLsWithScheme:completionHandler:]`. | P3
AUTH-C130 | CONFIRMED | src-tauri/src/macos_deeplink.rs:58-64 | The replacement is asynchronous and would have to be re-entered from an Objective-C block during startup. | P3
AUTH-C131 | CONFIRMED | src-tauri/src/app.rs:810-817 | A failed claim is only logged. | P3
AUTH-C132 | CONFIRMED | src-tauri/src/macos_deeplink.rs:40-44, 130-132 | `kLSNotAnApplicationErr` (-10811) is named for the bare-binary dev case and is in the status table. | P3
AUTH-C133 | CONFIRMED | src-tauri/src/macos_deeplink.rs:19-25, 48-57; app.rs:767-774 | The PKCE launch binding covers the gap. | P3
AUTH-C134 | CONFIRMED | src-tauri/src/app.rs:761-766, 833-870 | On a `name` mismatch the local registry/desktop file reflects our last-write entry. | P3
AUTH-C135 | CONFIRMED | src-tauri/src/app.rs:778-817 | Windows and Linux re-claim via `register_all()`; macOS via the CoreServices call. | P3
AUTH-C136 | CONFIRMED | src-tauri/src/macos_deeplink.rs:52-57; deep_link.rs:366-369 | The residual risk is documented as accepted. | P3
AUTH-C137 | CONFIRMED | src-tauri/src/macos_deeplink.rs:19-25; pkce.rs:77-101 | The PKCE verifier never leaves `AppState`. | P3

---

## DEFECTS (non-CONFIRMED, by blast radius)

```
claim_id: POLLING-C135
file: docs/architecture/polling.md:253
class: C5
verdict: DRIFT
evidence: src-tauri/src/teams.rs:828-833 — `status_set_log_line` returns only `"Successfully set Teams status message ({} bytes)"`; teams.rs:848-853 — the message itself goes to `debug!`, the byte count to `info!` (issue #912)
finding: The replaced status is NOT logged at info level. Since issue #912 the info line carries only the byte count, because the posted text is user content and the rotating log target keeps info records by default. The second half of the sentence ("the original profane text is never written to logs") is still true — and the implementation is strictly more conservative than the doc claims.
proposed_fix: Replace "The replaced status is logged at info level; the **original profane text is never written to logs**." with "The replaced status is logged at info level only as a byte count (`teams::status_set_log_line`, issue #912); the replaced text and the original profane text both go to `debug!` only."
severity: P1
```

```
claim_id: POLLING-C132
file: docs/architecture/polling.md:245
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/timing.rs:340-396 — `mod tests` with `test_pause_backoff_grows_then_caps`, `test_pause_backoff_honours_the_configured_ceiling`; src-tauri/src/polling/mod.rs:49-51 — "`poll_once.rs` is deleted"
finding: The pause-ladder tests live in `polling/timing.rs`. `poll_once.rs` was deleted in issue #754 and its surface re-exported from the focused modules, so the citation names a file that no longer exists.
proposed_fix: Replace "the pause-ladder tests in `poll_once.rs`" with "the pause-ladder tests in `polling/timing.rs`".
severity: P2
```

```
claim_id: POLLING-C004
file: docs/architecture/polling.md:10
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/mod.rs:49-51 — "Issue #754: `poll_once.rs` is deleted; its public surface is re-exported from the focused modules so existing call sites do not move."
finding: `polling/poll_once.rs` no longer exists. The single source of truth for one iteration is `polling/iteration.rs` (`pub(crate) fn run`), with the write path in `polling/write.rs`.
proposed_fix: Replace "around the single-source-of-truth `polling/poll_once.rs`" with "around the single-source-of-truth `polling/iteration.rs` (the `poll_once.rs` surface was split into the `polling/*` modules in #754)".
severity: P2
```

```
claim_id: POLLING-C010
file: docs/architecture/polling.md:19
class: C2
verdict: DRIFT
evidence: src-tauri/src/polling/mod.rs:49-51; src-tauri/src/polling/iteration.rs:53 `pub(crate) fn run(`
finding: `polling/poll_once::run` is now `polling/iteration::run`. The mermaid node `Tick[polling/poll_once::run one iteration]` names a deleted module.
proposed_fix: Replace `polling/poll_once::run one iteration` with `polling/iteration::run one iteration`.
severity: P2
```

```
claim_id: POLLING-C147
file: docs/architecture/polling.md:269
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/write.rs:762 `pub(crate) fn process_track(`; polling/mod.rs:22
finding: `process_track` lives in `polling/write.rs`. `poll_once.rs` was deleted in #754.
proposed_fix: Replace "The runtime caller is `polling::poll_once::process_track`" with "The runtime caller is `polling::write::process_track`".
severity: P2
```

```
claim_id: POLLING-C168
file: docs/architecture/polling.md:309
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/write.rs:1468-1472; src-tauri/src/spotify.rs:532
finding: The episode-template selection still happens in `process_track`, but that function now lives in `polling/write.rs`, not `poll_once.rs`.
proposed_fix: Replace "`poll_once::process_track` selects" with "`polling::write::process_track` selects".
severity: P2
```

```
claim_id: POLLING-C025
file: docs/architecture/polling.md:34
class: C1
verdict: DRIFT
evidence: src-tauri/src/profanity.rs:749-771 — the only public entry point is `filter_status_for_locale(text, placeholder, is_playing, extra_words, locale)`; `filter_status` survives only as a `#[cfg(test)]` helper (profanity.rs:784-790); polling/write.rs:1506, cli.rs:309, commands/status.rs:143, commands/misc.rs:75 all call the locale-explicit form
finding: `profanity::filter_status` is no longer a production function. Issue #758 slice 3 made the locale explicit and deleted the global-reading wrapper.
proposed_fix: Replace `profanity::filter_status if enabled` with `profanity::filter_status_for_locale if enabled`.
severity: P2
```

```
claim_id: POLLING-C119
file: docs/architecture/polling.md:213
class: C5
verdict: OVERSTATED
evidence: src-tauri/src/polling/presence.rs:613-630 vs 634-636 — the `decision.preferred_presence` branch (issue #866) runs BEFORE and INDEPENDENTLY of the `availability_sync_enabled` guard
finding: The statement is true for the rule's own `setPresence` pair, but issue #866 added a `preferred_presence` branch that arms `setUserPreferredPresence` even when `availability_sync` is off. "Inert unless `availability_sync` is on" is therefore not universally true of `rule_presence_backoff`.
proposed_fix: Replace "It is **inert unless `availability_sync` is on**" with "The rule's own `setPresence` pair is **inert unless `availability_sync` is on**; a rule that instead carries a `preferred_presence` pair (#866) drives `setUserPreferredPresence` regardless of that flag."
severity: P2
```

```
claim_id: POLLING-C074
file: docs/architecture/polling.md:112-113
class: C1
verdict: DRIFT
evidence: src-tauri/src/config/schema.rs:89-194; polling/presence.rs:144-201; polling/write.rs:858-877, 2064-2090
finding: "Two `TeamsConfig` flags shape what the polling loop writes" undercounts. The page itself goes on to document four (`presence_gate`, `gate_when_out_of_office`, `respect_manual_status`, `availability_sync`), and the tree carries more still: `gate_when_presenting` (#872), `idle_away_after_seconds` (#873), `pre_meeting_suppress_minutes` (#867) and `preferred_presence` (#866) all gate or move what the loop writes.
proposed_fix: Replace "Two `TeamsConfig` flags shape what the polling loop writes" with "Six `TeamsConfig` flags shape what the polling loop writes (`presence_gate`, `gate_when_out_of_office`, `respect_manual_status`, `availability_sync`, `gate_when_presenting`, `idle_away_after_seconds`)", and add a sentence for the `#867` calendar pre-gate and the `#866` preferred-presence surface.
severity: P2
```

```
claim_id: POLLING-C112
file: docs/architecture/polling.md:196
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/rules.rs:27-44 — `RuleDecision { reason, replacement, presence, preferred_presence }`
finding: `RuleDecision` has a fourth field, `preferred_presence` (issue #866). The doc's enumerated shape stops at `{ reason, replacement, presence }`.
proposed_fix: Replace "returns a `RuleDecision` — `{ reason, replacement, presence }`" with "returns a `RuleDecision` — `{ reason, replacement, presence, preferred_presence }`" and add one sentence: "`preferred_presence` (#866) is the fallback pair armed through `setUserPreferredPresence` when the rule names no pair of its own; the two fields are mutually exclusive by construction in `decision_from`."
severity: P2
```

```
claim_id: POLLING-C143
file: docs/architecture/polling.md:262
class: C5
verdict: DRIFT
evidence: src-tauri/src/profanity.rs:421-427 — `is_clean_compound` whitelists only `cock` + `tail`/`tails`; profanity.rs:440-445 — `head` moved INTO the profane-continuation list (issue #330: `dickhead`); profanity.rs:446-448 — `hand` is not a carve-out at all
finding: The safe-suffix list is not `tail, head, hand, ...`. `tail`/`tails` is the only whitelisted suffix, and it is scoped to the `cock` stem. `head` is a *profane* compound continuation (`dickhead`, `fuckhead`), not a safe suffix. `fishtail`/`forehead`/`handheld` stay clean because they contain no profane stem at all, not because of a safe-suffix list.
proposed_fix: Replace "**Compound-word safe-suffixes:** `tail, head, hand, ...` allow `fishtail`, `forehead`, `handheld`." with "**Compound-word handling:** `tail`/`tails` is the only whitelisted suffix and it is scoped to the `cock` stem (`is_clean_compound`), so `cocktail` stays clean while `dickhead` still flags; `head`/`boy`/`face`/`wad`/`post` are *profane* continuations, not safe suffixes."
severity: P2
```

```
claim_id: POLLING-C137
file: docs/architecture/polling.md:256
class: C5
verdict: DRIFT
evidence: src-tauri/src/profanity.rs:169 `'z' => ('s', true)` inside `leet_fold`, applied unconditionally by `normalize` (profanity.rs:288-291); the test at profanity.rs:1155-1182 calls it "`z` for plural `s`", with no terminal-position gate
finding: The `z→s` fold is not terminal-scoped. It applies at every position; what keeps it safe is the boundary gating in `match_from` (profanity.rs:350-390) plus the `is_profane_continuation` per-stem carve-outs, so `Zombie` and `Fukushima` stay clean (test 1178-1180) without a positional restriction.
proposed_fix: Replace "terminal `z→s` (#377/#470)" with "`z→s` (#377/#470), with the boundary gate rather than position keeping `Zombie`/`Fukushima` clean".
severity: P3
```

```
claim_id: POLLING-C122
file: docs/architecture/polling.md:223-224
class: C1
verdict: DRIFT
evidence: src-tauri/src/state.rs:335-344 — `pub fn try_claim(&self)` performs `self.is_syncing.compare_exchange(false, true, AcqRel, Acquire)`; commands/sync.rs:247 calls `state.polling.try_claim()`; polling/state.rs:777-810 pins the ABSENCE of `.compare_exchange(` in `start_polling`
finding: `commands/sync::start_syncing` is indeed the sole claimer, but the `compare_exchange(false, true, …)` is not literally *there* — it sits in `state.rs::PollingState::try_claim`, and a source-scan test actively forbids it from reappearing in `start_polling`.
proposed_fix: Replace "`compare_exchange(false, true, …)` is here" with "the claim is `state.polling.try_claim()`, whose `compare_exchange(false, true, …)` lives in `src-tauri/src/state.rs`".
severity: P3
```

```
claim_id: POLLING-C129
file: docs/architecture/polling.md:238
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/timing.rs:292-302 `pub(crate) fn pause_backoff(`; polling/mod.rs:49-51
finding: `pause_backoff` lives in `polling/timing.rs`; the `poll_once::` prefix names a module deleted in #754.
proposed_fix: Replace "`poll_once::pause_backoff(consecutive, default, ceiling)`" with "`polling::timing::pause_backoff(consecutive, default, ceiling)`".
severity: P2
```

```
claim_id: POLLING-C128
file: docs/architecture/polling.md:237
class: C3
verdict: DRIFT
evidence: src-tauri/src/profanity.rs:749-771 — `pub fn filter_status_for_locale(text, placeholder, is_playing, extra_words, locale)`
finding: The "Consumed by" cell names `profanity::filter_status(text, placeholder, is_playing, extra_words)`, a four-argument signature that no longer exists in production code.
proposed_fix: Replace "`profanity::filter_status(text, placeholder, is_playing, extra_words)`" with "`profanity::filter_status_for_locale(text, placeholder, is_playing, extra_words, locale)`".
severity: P2
```

```
claim_id: AUTH-C099
file: docs/architecture/auth-and-tokens.md:138
class: C1
verdict: DRIFT
evidence: src-tauri/src/deep_link.rs:270-452 `pub(crate) fn handle_deep_link<S, D>(`; src-tauri/src/lib.rs:1-30 (slim module registry: `pub mod deep_link;`)
finding: `handle_deep_link` moved out of `lib.rs` into the top-level `src-tauri/src/deep_link.rs` module; `lib.rs` is now a slim module registry. It still parses the URL, matches on scheme and dispatches to `handle_spotify_callback`, but the path citation is stale.
proposed_fix: Replace "`lib.rs::handle_deep_link`" with "`deep_link.rs::handle_deep_link`".
severity: P2
```

```
claim_id: AUTH-C116
file: docs/architecture/auth-and-tokens.md:166
class: C3
verdict: DRIFT
evidence: tauri-plugin-deep-link 2.4.10 `register()` Linux arm — `let target = self.app.path().data_dir()?.join("applications");` then `target.join(&file_name)`; src-tauri/src/app.rs:843-847 names `presence-jam-handler.desktop`; src-tauri/tauri.conf.json:5 `"identifier": "com.presencejam.app"`
finding: The Linux desktop file is NOT `~/.local/share/applications/presencejam.desktop`. The plugin writes `<productName>-handler.desktop` (here `presence-jam-handler.desktop`) into `data_dir()/applications`, which for the bundle id `com.presencejam.app` resolves to `~/.local/share/com.presencejam.app/applications/`, not the XDG user applications directory.
proposed_fix: Replace "writes `~/.local/share/applications/presencejam.desktop`" with "writes `presence-jam-handler.desktop` into `~/.local/share/com.presencejam.app/applications/` (the plugin's `data_dir()/applications`, not the XDG `~/.local/share/applications`) and runs `xdg-mime default` against that file".
severity: P2
```

```
claim_id: AUTH-C111
file: docs/architecture/auth-and-tokens.md:156
class: C2
verdict: DRIFT
evidence: src-tauri/src/polling/refresh.rs:174 — `crate::keychain::peek_spotify_client_secret()` (the spotify-client-secret cache); src-tauri/src/keychain.rs:57-72 (cache struct), 176-197 (cache accessor). No PKCE verifier cache exists anywhere under `polling/` (grep for "verifier" in polling/*.rs returns nothing).
finding: "our polling thread's verifier cache" is the wrong mechanism. The polling thread caches the spotify *client secret*; the PKCE verifier never reaches it — it lives only in `AppState::pending.spotify()` (state.rs:11-21, 193-208). The security conclusion (the secret stays off disk) is correct, but the named cache is not.
proposed_fix: Replace "and our polling thread's verifier cache keeps the secret off disk" with "and the polling thread never sees a PKCE verifier at all — its secret cache is the keychain-backed Spotify client secret (issue #9), which is never written to disk".
severity: P3
```

```
claim_id: AUTH-C079
file: docs/architecture/auth-and-tokens.md:108
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/presence.rs:70-83 `pub(crate) fn presence_expiration_duration(`; polling/mod.rs:49-51
finding: `presence_expiration_duration` now lives in `polling/presence.rs`, not `poll_once.rs`.
proposed_fix: Replace "`poll_once.rs::presence_expiration_duration`" with "`polling/presence.rs::presence_expiration_duration`".
severity: P2
```

```
claim_id: AUTH-C005
file: docs/architecture/auth-and-tokens.md:18
class: C4
verdict: DRIFT
evidence: src/lib/components/settings/SpotifyCard.svelte:109-133 — Settings renders the Client ID input and a read-only three-state secret hint (`present`/`unavailable`/`absent` → "run onboarding"); src/lib/components/Onboarding.svelte:527-532 is the only Client Secret input; commands/spotify_auth.rs:411 is the only `store_spotify_client_secret` call site and it is reached only from `start_spotify_auth`, invoked only from Onboarding.svelte:237
finding: The Client ID is editable in Settings, but the Client Secret is entered only in the Onboarding wizard and is never re-entered from Settings. The mermaid node "Enter Client ID + Secret (Settings)" points a developer at the wrong screen for the secret.
proposed_fix: Replace `Enter Client ID + Secret (Settings)` with `Enter Client ID (Settings) + Client Secret (Onboarding)`.
severity: P3
```

```
claim_id: AUTH-C097
file: docs/architecture/auth-and-tokens.md:130
class: C6
verdict: OVERSTATED
evidence: src-tauri/src/app.rs:1141-1143 — `#[cfg(desktop)] if !cli_mode { setup_deep_links(app); … }`; app.rs:1033-1040 (`--daemon` shares the `--sync-once` CLI-mode rule), 1279-1281 (issue #865: `--serve` skips registration so a headless daemon never claims the scheme from a GUI install)
finding: Registration runs on every GUI launch, but it is deliberately skipped in all three CLI modes (`--sync-once`, `--serve`, `--daemon`) so a headless daemon cannot steal `presencejam://` from a real GUI install. "On every launch, not just at install" elides that carve-out.
proposed_fix: Replace "The registration runs **on every launch**, not just at install" with "The registration runs **on every GUI launch** (not just at install); it is deliberately skipped under `--serve`, `--sync-once` and `--daemon` so a headless daemon never claims the scheme away from a GUI install (#865)".
severity: P3
```

```
claim_id: AUTH-C052
file: docs/architecture/auth-and-tokens.md:82
class: C4
verdict: EXTERNAL-UNVERIFIED
evidence: src-tauri/src/teams.rs:15-19 asserts "Microsoft's first-party **Microsoft Graph Command Line Tools** application registration"; no allowlisted vendor page confirms the ownership of `14d82eec-204b-4c2f-b7e8-296a70dab67e`. The only in-tree corroboration is the same comment.
finding: The app-registration ownership claim is an external fact. The repository asserts it and the id is byte-exact and test-pinned, but I could not source the name↔id mapping from an allowlisted domain (`learn.microsoft.com/graph/*`).
proposed_fix: Add a citation to a Microsoft Learn first-party-application table (or state it as an unverified repo assertion). Until then, soften "from Microsoft's first-party … application registration" to "a widely-republished Microsoft first-party public-client id".
severity: P2
```

```
claim_id: AUTH-C067
file: docs/architecture/auth-and-tokens.md:93
class: C4
verdict: EXTERNAL-UNVERIFIED
evidence: src-tauri/src/teams.rs:278-287 asserts "`profile` adds the `oid` claim to the access-token JWT so the setPresence/clearPresence /users/{oid} fallback can resolve the user"; no allowlisted vendor page states that `profile` causes `oid` to be emitted in an access token.
finding: The `profile`→`oid` relationship is asserted only in the repo's own comment. The optional-claims reference (`learn.microsoft.com/entra/identity-platform/optional-claims-reference`) documents `profile` as gating `family_name`/`given_name`/`upn`, not `oid`.
proposed_fix: Either cite a Microsoft page that states `profile` gates `oid` in access tokens, or soften to "the `oid` claim the Teams access-token JWT carries once `profile` has been requested (repo assertion, not independently sourced)".
severity: P2
```

```
claim_id: AUTH-C056
file: docs/architecture/auth-and-tokens.md:84
class: C2
verdict: EXTERNAL-UNVERIFIED
evidence: src-tauri/src/teams.rs:15-19 (shared-identity comment). Same gap as AUTH-C052: the user-visible consent naming depends on the unverified name↔id mapping.
finding: Whether a consent entry is *displayed* as "Microsoft Graph Command Line Tools" is an Entra-portal naming fact I could not source from an allowlisted domain.
proposed_fix: Cite a Microsoft Learn first-party-application table, or qualify with "may be named … (the shared registration's display name)".
severity: P2
```

```
claim_id: AUTH-C076
file: docs/architecture/auth-and-tokens.md:105
class: C4
verdict: EXTERNAL-UNVERIFIED
evidence: Same gap as AUTH-C067. `graph_oid_from_access_token` (teams.rs:931-951) fails cleanly when the claim is absent, so the app is defensive either way, but the "present once `profile` is in the scope string" precondition is repo-asserted only.
finding: The precondition for the `oid` claim's presence is not independently sourced.
proposed_fix: See AUTH-C067.
severity: P2
```

```
claim_id: AUTH-C022
file: docs/architecture/auth-and-tokens.md:40
class: C4
verdict: EXTERNAL-UNVERIFIED
evidence: src-tauri/src/pkce.rs:7-10, 19-23 cite RFC 7636 §4.1/§4.2 but not §5; no allowlisted vendor source confirms the additivity claim.
finding: "RFC 7636 §5 keeps PKCE params additive" is a standards citation I cannot verify against an allowlisted domain.
proposed_fix: Cite the RFC directly (`https://datatracker.ietf.org/doc/html/rfc7636#section-5`) or drop the section number and keep the qualitative statement.
severity: P3
```

```
claim_id: AUTH-C023
file: docs/architecture/auth-and-tokens.md:41
class: C4
verdict: EXTERNAL-UNVERIFIED
evidence: No allowlisted source. The Spotify Web API docs (`developer.spotify.com/documentation/web-api/*`, fetched 2026-10-09) describe the Authorization Code flow and its required Basic header but carry no "confidential clients are expected to use the secret" requirement, and no link to the Feb-2025 post.
finding: The attribution to Spotify's "Increasing the security requirements" post is unsourced from an allowlisted domain.
proposed_fix: Add the blog URL, or soften to "Spotify's Authorization Code flow documents the client secret as a required credential for confidential clients".
severity: P3
```

```
claim_id: POLLING-C081
file: docs/architecture/polling.md:127
class: C5
verdict: OVERSTATED
evidence: src-tauri/src/polling/presence.rs:136-142, 163-199 — order is: presence reasons (busy / DND / focusing / out-of-office) → `gate_when_presenting` (#872) → manual status (#635) → idle (#873, "lowest of all"); src-tauri/src/teams.rs:1120-1127 (the in-source "LOWEST-precedence" comment predates #872/#873)
finding: Out-of-office is no longer the lowest-precedence reason. The OS presentation signal (#872), the manual-status check (#635) and the desktop-idle reading (#873) all rank below it. The doc inherited a comment that #872/#873 superseded.
proposed_fix: Replace "the **lowest-precedence** reason" with "the **lowest-precedence *presence* reason** — it still outranks the OS presentation signal (#872), the manual-status check (#635) and the desktop-idle reading (#873), which are all lower".
severity: P2
```

---

## COUNTS

| Verdict | Count |
| --- | --- |
| CONFIRMED | 280 |
| DRIFT | 19 |
| OVERSTATED | 3 |
| EXTERNAL-UNVERIFIED | 6 |
| STALE | 0 |
| MISSING | 0 |
| UNSOURCED | 0 |
| **Total** | **308** |

Severity of the 28 defects: **P1 × 1** (POLLING-C135 — a false logging claim on a privacy-relevant path), **P2 × 20**, **P3 × 7**. No P0s.

Per-file: `polling.md` — 171 claims: 155 CONFIRMED / 14 DRIFT / 2 OVERSTATED (16 defects).
`auth-and-tokens.md` — 137 claims: 125 CONFIRMED / 5 DRIFT / 1 OVERSTATED / 6 EXTERNAL-UNVERIFIED (12 defects).

Defects by claim class: C1 × 11, C2 × 3, C3 × 2, C4 × 6, C5 × 5, C6 × 1, C7 × 0.

Note on `MISSING`: several behaviours exist in the tree with no corresponding doc claim — most notably the `#872` OS-presentation gate (`gate_when_presenting` / `GATE_REASON_PRESENTING` / `GATE_REASON_QUIET_TIME`), the `#873` desktop-idle gate (`idle_away_after_seconds` / `GATE_REASON_IDLE`), the `#867` calendar pre-gate (`GATE_REASON_CALENDAR`, `pre_meeting_suppress_minutes`), the `#866` preferred-presence surface (`setUserPreferredPresence` / `clearUserPreferredPresence`), and `#792`'s ungated mid-track manual-status re-check. None of these is reachable from a numbered claim in `POLLING.md` or `AUTH.md`, so they are recorded here rather than as `MISSING` verdicts.
