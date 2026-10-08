//! Configuration store split (issue #755).
//!
//! One concern per file; `crate::config::X` paths stay stable through the
//! re-exports below.

pub mod clamp;
pub mod io;
pub mod migrate;
pub mod patch;
pub mod schema;
pub mod snooze;
pub mod transfer;

pub use clamp::{
    clamp_presence_profiles, clamp_profile_id, clamp_rule_text, effective_config,
    effective_snapshot, normalize_presence_pair, preferred_presence_expiry_duration,
    preferred_presence_pair, profanity_extra_words_for_filter, PresencePair, MAX_PROFILE_ID_CHARS,
    MAX_RULE_STATUS_CHARS, MAX_TRACK_RULE_DURATION_SECS, MAX_TRACK_RULE_SNOOZE_MINUTES,
    PRESENCE_COMBINATIONS, TRACK_RULE_DAY_MINUTES,
};
#[cfg(test)]
pub(crate) use io::atomic_write_json;
pub(crate) use io::quarantine_backup_path;
pub use io::{
    clamped_config, config_dir, config_quarantine_backup_name, config_was_quarantined,
    emit_config_changed, get_config_path, load_config, logging_config_for_startup, save_config,
    save_config_persisted, CONFIG_CHANGED_EVENT, STALE_REVISION_MARKER,
};
pub use migrate::{
    migrate_legacy_client_secret, migrate_legacy_client_secret_with_app, stamp_schema_version,
    LegacySecretOutcome, SCHEMA_VERSION, SPOTIFY_SECRET_CONFLICT_EVENT,
};
pub use patch::{
    apply_patch, ConfigPatch, LoggingPatch, NotificationsPatch, PlaybackPatch, PollingPatch,
    ShortcutsPatch, SpotifyPatch, StatusRulesPatch, TeamsPatch, UpdatesPatch,
};
pub use schema::{
    apply_log_level, AppConfig, ClientSecretState, LoggingConfig, NotificationsConfig,
    PlaybackConfig, PollingConfig, PreferredPresenceConfig, PresenceProfile, QuietHoursEntry,
    ShortcutsConfig, SpotifyConfig, StatusRulesConfig, TeamsConfig, TrackRuleAction,
    TrackRuleEntry, TrackRuleMatchKind, UpdateChannel, UpdatesConfig,
    DEFAULT_TOGGLE_PLAYBACK_SHORTCUT, DEFAULT_TOGGLE_SYNC_SHORTCUT,
};
pub use snooze::{
    clamp_snooze, snooze_deadline, snooze_expired_deadline, snooze_minutes_left,
    snooze_preset_deadline, snooze_status, snooze_store_form, SnoozePreset, SnoozeStatus,
};
pub use transfer::{
    export_document, export_file_name, import_config_document, prepare_import, PreparedImport,
};

#[cfg(test)]
mod tests {
    use super::clamp::{
        clamp_logging, clamp_polling, clamp_preferred_presence, clamp_rules, clamp_teams,
        MAX_SHORTCUT_BINDING_CHARS,
    };
    use super::io::{
        load_config_from, quarantine_backup_name_for, quarantine_corrupt_config, save_config_to,
        staged_import_path, stamp_keychain_flags, with_keychain_flags, TYPED_CONFIG_KEYS,
    };
    use super::migrate::{
        decide_legacy_secret_outcome, default_schema_version, migrate_config,
        write_legacy_secret_sidecar, LEGACY_SECRET_SIDECAR_NAME,
    };
    use super::schema::{default_redirect_uri, default_status_format, lenient_update_channel};
    use super::snooze::{next_local_midnight_utc, resolve_local_forward};
    use super::transfer::{client_secret_paths, legacy_client_secret};
    use super::*;
    use crate::profanity;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    /// Serialises the tests that assert on captured log output (issue #758
    /// slice 2). This is NOT a state-isolation lock: the quarantine flag and
    /// the config caches it used to guard are per-`AppCaches` now, so those
    /// tests construct their own and run in parallel. What remains genuinely
    /// process-wide is `log::set_boxed_logger` — one logger per process, whose
    /// sink is the shared `LOG_LINES` buffer below — so a test that clears and
    /// reads that buffer has to exclude the other tests that do the same.
    static LOG_CAPTURE_TEST_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.spotify.redirect_uri, "presencejam://callback");
        assert_eq!(config.teams.status_format, "🎵 {artist} - {track} 🎧");
        assert!(config.teams.clear_on_pause);
        assert!(config.teams.profanity_filter);
        assert_eq!(
            config.teams.profanity_placeholder,
            profanity::safe_placeholder_default()
        );
        // Issue #3.0-P1/P2: availability sync OFF, presence gate ON.
        assert!(!config.teams.availability_sync);
        assert!(config.teams.presence_gate);
        assert_eq!(config.polling.default_interval_seconds, 30);
        assert!(config.logging.enabled);
    }

    /// Issue #560: the whole point of the tri-state is that `Unavailable`
    /// survives the keychain → config → wire boundary as a *distinct* state.
    /// Anything that folded it into `Absent` (or into the `Present`-only
    /// bool) would put a locked-keyring user back in the setup wizard with
    /// their secret still stored.
    #[test]
    fn client_secret_state_maps_keychain_presence() {
        use crate::keychain::KeychainPresence;
        assert_eq!(
            ClientSecretState::from(&KeychainPresence::Present),
            ClientSecretState::Present
        );
        assert_eq!(
            ClientSecretState::from(&KeychainPresence::Absent),
            ClientSecretState::Absent
        );
        assert_eq!(
            ClientSecretState::from(&KeychainPresence::Unavailable("keyring locked".into())),
            ClientSecretState::Unavailable
        );
    }

    /// A locked keychain remains distinct from a missing secret on the full
    /// config shape: only `Present` sets the legacy bool, while the tri-state
    /// carries `Unavailable` to the UI.
    #[test]
    fn unavailable_keychain_is_not_collapsed_into_absent() {
        let config = stamp_keychain_flags(
            AppConfig::default(),
            crate::keychain::KeychainPresence::Unavailable("keyring locked".into()),
        );
        assert!(!config.spotify.client_secret_set);
        assert_eq!(
            config.spotify.client_secret_state,
            ClientSecretState::Unavailable
        );
    }

    /// The config-load seam stamps both derived fields from one presence
    /// observation. This exercises the same function that `load_config` uses,
    /// rather than testing the keychain cache in isolation.
    #[test]
    fn config_presence_flags_use_the_cached_config_seam() {
        let dir = std::env::temp_dir().join(format!(
            "presencejam-keychain-config-seam-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("the config seam test directory must be creatable");
        let path = dir.join("config.json");
        std::fs::write(&path, "{}\n").expect("the config seam fixture must be writable");

        let caches = crate::state::AppCaches::new();
        let loaded = load_config_from(&caches, &path).expect("the real config file seam must load");
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let config = with_keychain_flags(loaded, || {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            crate::keychain::KeychainPresence::Present
        });
        assert!(config.spotify.client_secret_set);
        assert_eq!(
            config.spotify.client_secret_state,
            ClientSecretState::Present
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);

        std::fs::remove_dir_all(dir).expect("the config seam test directory must be removable");
    }

    /// The frontend switches on these three literals, so the spelling is part
    /// of the wire contract — and both directions must round-trip.
    #[test]
    fn client_secret_state_wire_spelling() {
        for (state, wire) in [
            (ClientSecretState::Present, "\"present\""),
            (ClientSecretState::Absent, "\"absent\""),
            (ClientSecretState::Unavailable, "\"unavailable\""),
        ] {
            assert_eq!(serde_json::to_string(&state).unwrap(), wire);
            assert_eq!(
                serde_json::from_str::<ClientSecretState>(wire).unwrap(),
                state
            );
        }
    }

    /// A `config.json` written before #560 has no `client_secret_state` key;
    /// it must still load, defaulting to the state that does not accuse the
    /// keychain of anything (`with_keychain_flags` re-stamps it on every load
    /// anyway — the default only has to be safe, never authoritative).
    #[test]
    fn pre_4_6_config_json_still_loads() {
        let legacy: SpotifyConfig = serde_json::from_str(
            r#"{"client_id":"abc","client_secret_set":true,"redirect_uri":"presencejam://callback"}"#,
        )
        .expect("a config.json written before #560 must still deserialize");
        assert_eq!(legacy.client_secret_state, ClientSecretState::Absent);
        assert!(legacy.client_secret_set);
    }

    /// 4.7.0 / issue #675: `notifications` is a new top-level section, so a
    /// pre-4.7 `config.json` has no such key and every class must come back
    /// ON (the frontend's per-class dispatch reads these flags — a
    /// `false` default would silently disable a class the user never turned
    /// off). The other direction matters too: an explicit `false` is a user
    /// decision and must survive the round-trip, since `save_config` is how
    /// the Settings card persists a toggle.
    #[test]
    fn notifications_default_on_and_round_trip() {
        let legacy: NotificationsConfig =
            serde_json::from_str("{}").expect("a pre-4.7 config must still deserialize");
        assert!(legacy.track_change);
        assert!(legacy.sync_stopped);
        assert!(legacy.auth_required);
        assert!(legacy.update_staged);
        assert_eq!(
            legacy.track_change,
            NotificationsConfig::default().track_change
        );

        let off = r#"{"track_change":false,"sync_stopped":true,"auth_required":true,"update_staged":false}"#;
        let parsed: NotificationsConfig = serde_json::from_str(off).unwrap();
        assert!(!parsed.track_change);
        assert!(!parsed.update_staged);
        assert_eq!(serde_json::to_string(&parsed).unwrap(), off);
    }

    /// Regression guard for issue found in PR review: a redundant
    /// `fs::remove_file(path)` before the final `rename` opened a window
    /// where a process crash leaves config.json missing. Drop the
    /// remove_file; rename() atomically replaces the destination on
    /// POSIX + same-volume Windows renames. Mirrors token_io.rs pattern.
    #[test]
    fn test_atomic_write_json_replaces_existing_file() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, b"OLD_CONTENTS_AAA").unwrap();

        atomic_write_json(&path, "NEW_CONTENTS_BBB").expect("write should succeed");

        // After atomic_write_json, the destination must hold the new bytes
        // (no mix with the old), and there must be no leftover .tmp sidecar.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "NEW_CONTENTS_BBB");
        let sidecar = path.with_extension("tmp");
        assert!(
            !sidecar.exists(),
            "temp sidecar {} must be consumed by rename (rename atomicity)",
            sidecar.display()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Source-level regression guard for the crash-window bug: a redundant
    /// `fs::remove_file(path)` between temp-write-fsync and `rename()`
    /// breaks the rename-atomicity guarantee (rename atomically replaces
    /// the destination on POSIX + Windows same-volume renames, removing
    /// first leaves a crash window where the destination is gone and the
    /// rename never happens).
    ///
    /// Robust anchor: walk a brace count from the first `{` after the
    /// `fn atomic_write_json(...)` signature. The body's `{`/`}` count is
    /// independent of what other functions are declared around it, so this
    /// test survives reordering / splitting / renaming of adjacent code.
    #[test]
    fn test_atomic_write_json_does_not_remove_destination_first() {
        let src = concat!(
            include_str!("schema.rs"),
            include_str!("clamp.rs"),
            include_str!("snooze.rs"),
            include_str!("patch.rs"),
            include_str!("migrate.rs"),
            include_str!("io.rs"),
            include_str!("transfer.rs")
        );
        // Find the function signature (the line that starts the body).
        let sig_idx = src
            .find("fn atomic_write_json(")
            .expect("atomic_write_json must exist");
        // Walk forward until the first `{`, then count braces to find the
        // matching `}`. Robust to whatever comes after the function.
        let brace_open_rel = src[sig_idx..]
            .find('{')
            .expect("atomic_write_json body must have an opening brace");
        let body_start = sig_idx + brace_open_rel;
        let mut depth: u32 = 0;
        let mut i = body_start;
        let body_end = loop {
            let ch = src.as_bytes()[i];
            match ch {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break i;
                    }
                }
                _ => {}
            }
            i += 1;
            if i >= src.len() {
                panic!("atomic_write_json body has unbalanced braces");
            }
        };
        let body = &src[body_start + 1..body_end];
        // Allow `remove_file(&temp_path)` (pre-clearing a stale sidecar from
        // a prior crash, introduced by issue #135 path A) but forbid
        // `remove_file(path)` (removing the destination before rename would
        // break the rename-atomicity guarantee). The latter is the original
        // PR #133 regression. We anchor on the destination-path identifier
        // to be string-literal-safe (the brace counter excludes braces
        // inside string contents only by accident, so we rely on the
        // specific `remove_file(path` token rather than free-form
        // `remove_file`).
        assert!(
            !body.contains("remove_file(path"),
            "atomic_write_json must not call remove_file(path) on the destination \
             before rename — rename atomically replaces the destination on POSIX \
             + Windows same-volume renames, and the explicit remove breaks \
             crash-safety. `remove_file(&temp_path)` on the sidecar IS allowed \
             (issue #135 path A: pre-clear stale sidecar from a prior crash). \
             See ARCHITECTURE.md 'Storage' section and PR #133 for the \
             original regression context."
        );
    }

    fn chrono_like_nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    /// Regression guard for issue #135: a stale `.tmp` from a previous crash
    /// must not block the next write. Without the pre-clear, the new
    /// create_new(true) on a leftover sidecar would error with AlreadyExists
    /// and turn a one-off crash into a permanent save failure.
    #[test]
    fn test_atomic_write_json_recovers_from_stale_tmp_sidecar() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-recover-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let sidecar = path.with_extension("tmp");

        // Simulate a previous crash that left the sidecar behind.
        std::fs::write(&sidecar, b"PARTIAL_GARBAGE_FROM_CRASH").unwrap();
        assert!(sidecar.exists(), "sidecar must exist before recovery");

        atomic_write_json(&path, "NEW_CONTENTS_AFTER_CRASH")
            .expect("write must succeed despite stale sidecar");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "NEW_CONTENTS_AFTER_CRASH"
        );
        assert!(
            !sidecar.exists(),
            "sidecar must be consumed by rename (no .tmp leftover)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #376: the keychain/config conflict branch must resolve to a
    /// dedicated outcome (which the `_with_app` entry point turns into a
    /// user-visible `spotify-secret-conflict` event) — never silently to
    /// `Migrated` (which strips the file) or `NoLegacyField` (which stays
    /// log-only). Case from the issue repro: config plaintext "AAA" vs
    /// keychain "BBB".
    #[test]
    fn test_decide_legacy_secret_conflict_keychain_differs() {
        let outcome = decide_legacy_secret_outcome(Some("AAA"), &Ok("BBB".to_string()));
        assert_eq!(outcome, LegacySecretOutcome::ConflictKeychainDiffers);
    }
    #[test]
    fn test_decide_legacy_secret_outcome_matrix() {
        // No plaintext at all (fresh install / already migrated).
        assert_eq!(
            decide_legacy_secret_outcome(None, &Err("empty".to_string())),
            LegacySecretOutcome::NoLegacyField
        );
        // Empty-string field is not a secret.
        assert_eq!(
            decide_legacy_secret_outcome(Some(""), &Err("empty".to_string())),
            LegacySecretOutcome::NoLegacyField
        );
        // Empty keychain (typical pre-v2.6.0 upgrader): migrate + strip.
        assert_eq!(
            decide_legacy_secret_outcome(Some("AAA"), &Err("empty".to_string())),
            LegacySecretOutcome::Migrated
        );
        // Keychain already holds the identical value: strip-only.
        assert_eq!(
            decide_legacy_secret_outcome(Some("AAA"), &Ok("AAA".to_string())),
            LegacySecretOutcome::Migrated
        );
        // Same value with surrounding keychain state must not count as a
        // conflict: equality is exact, so near-misses still conflict.
        assert_eq!(
            decide_legacy_secret_outcome(Some("AAA"), &Ok("AAA ".to_string())),
            LegacySecretOutcome::ConflictKeychainDiffers
        );
    }
    /// The frontend listens on the literal event name, so a rename of the
    /// constant silently breaks Settings without a compile error on either
    /// side. Pin the contract string (issue #376).
    #[test]
    fn test_spotify_secret_conflict_event_name_contract() {
        assert_eq!(SPOTIFY_SECRET_CONFLICT_EVENT, "spotify-secret-conflict");
    }

    /// Issue #379: files written before `schema_version` existed must load
    /// as version 1.
    #[test]
    fn test_schema_version_defaults_to_1_when_absent() {
        let cfg: AppConfig = serde_json::from_str("{}").expect("empty object must parse");
        assert_eq!(cfg.schema_version, 1);
        assert_eq!(AppConfig::default().schema_version, 1);
    }

    /// Issue #379: an explicit schema_version round-trips untouched.
    #[test]
    fn test_schema_version_round_trip() {
        let cfg: AppConfig = serde_json::from_str(r#"{"schema_version": 3}"#).expect("must parse");
        assert_eq!(cfg.schema_version, 3);
        let json = serde_json::to_string(&cfg).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.schema_version, 3);
    }

    /// Issue #379: unknown top-level keys survive a save round-trip via
    /// `extra` instead of being silently stripped.
    #[test]
    fn test_unknown_future_key_survives_save_round_trip() {
        let cfg: AppConfig =
            serde_json::from_str(r#"{"autostart": true, "future_key": {"nested": [1, 2, 3]}}"#)
                .expect("must parse");
        assert_eq!(
            cfg.extra
                .get("future_key")
                .expect("future_key must be retained"),
            &serde_json::json!({"nested": [1, 2, 3]})
        );
        // `save_config` serialises a `clamped_config` clone with
        // `to_string_pretty`; clamping only touches polling and the
        // profanity lexicon, so this exercises the same serde path as a real
        // save without touching the user's config file.
        let json = serde_json::to_string_pretty(&clamped_config(&cfg)).expect("must serialize");
        assert!(
            json.contains("future_key"),
            "serialised config must still carry the unknown key"
        );
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.extra.get("future_key"), cfg.extra.get("future_key"));
        assert!(back.autostart);
    }

    /// Issue #379: a corrupt config file is quarantined to `<name>.bak`
    /// alongside the original and the diagnostics-visible flag is raised.
    #[test]
    fn test_corrupt_config_quarantined_to_bak() {
        // Issue #758 slice 2: the quarantine flag is per-`AppCaches`, so this
        // test owns its flag and needs no process-wide lock or save/restore.
        let caches = crate::state::AppCaches::new();
        let dir = std::env::temp_dir().join(format!(
            "pj-test-quarantine-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, b"{ NOT VALID JSON !!!").unwrap();
        // Precondition: the file is genuinely unparsable as AppConfig.
        assert!(
            serde_json::from_str::<AppConfig>(&std::fs::read_to_string(&path).unwrap()).is_err()
        );

        let backup = quarantine_corrupt_config(&caches, &path, "test corrupt sentinel");
        assert_eq!(backup, dir.join("config.json.bak"));
        assert!(!path.exists(), "corrupt original must be renamed away");
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            b"{ NOT VALID JSON !!!",
            "quarantined copy must preserve the corrupt bytes"
        );
        assert!(
            config_was_quarantined(&caches),
            "diagnostics-visible flag must be raised after quarantine"
        );
        // Defaults remain loadable alongside the quarantine.
        assert_eq!(AppConfig::default().schema_version, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #379: when the quarantine rename itself fails (e.g. a
    /// directory already occupies the `<name>.bak` target), the load must
    /// still fall back to defaults — the corrupt original is preserved and
    /// the diagnostics-visible flag is raised either way.
    #[test]
    fn test_corrupt_config_quarantine_rename_failure_preserves_original() {
        // Issue #758 slice 2: per-test caches, no process-wide lock.
        let caches = crate::state::AppCaches::new();
        let dir = std::env::temp_dir().join(format!(
            "pj-test-quarantine-fail-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let corrupt_bytes = b"{ NOT VALID JSON !!!";
        std::fs::write(&path, corrupt_bytes).unwrap();
        // Colliding target: a directory at the backup path makes
        // `fs::rename(file, dir)` fail on both POSIX and Windows.
        std::fs::create_dir_all(dir.join("config.json.bak")).unwrap();

        let backup = quarantine_corrupt_config(&caches, &path, "test rename-failure sentinel");
        assert_eq!(backup, dir.join("config.json.bak"));
        // Rename failed → the corrupt original must still be in place with
        // its bytes untouched, and the flag is raised so diagnostics still
        // observe the quarantine attempt.
        assert!(
            path.exists(),
            "corrupt original must be preserved when the quarantine rename fails"
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            corrupt_bytes,
            "failed quarantine must not truncate or alter the original"
        );
        assert!(
            config_was_quarantined(&caches),
            "diagnostics-visible flag must be raised even when the rename fails"
        );
        // Defaults remain loadable alongside the failed quarantine.
        assert_eq!(AppConfig::default().schema_version, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #379: `extra` keys serialize in deterministic (sorted) order so
    /// multi-key future payloads do not flap between saves.
    #[test]
    fn test_extra_keys_serialize_in_sorted_order() {
        let mut cfg = AppConfig::default();
        cfg.extra.insert("zeta".to_string(), serde_json::json!(1));
        cfg.extra.insert("alpha".to_string(), serde_json::json!(2));
        cfg.extra.insert("mid".to_string(), serde_json::json!(3));
        let json = serde_json::to_string(&cfg).expect("must serialize");
        let alpha = json.find("\"alpha\"").expect("alpha must serialize");
        let mid = json.find("\"mid\"").expect("mid must serialize");
        let zeta = json.find("\"zeta\"").expect("zeta must serialize");
        assert!(
            alpha < mid && mid < zeta,
            "extra keys must serialize in sorted order, got: {json}"
        );
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.extra, cfg.extra);
    }

    /// Issue #432: pre-4.5 config files without `status_rules` load with
    /// empty rule lists (serde default), and the #379 versioning is
    /// untouched — schema_version still defaults to 1.
    #[test]
    fn test_status_rules_default_empty_when_absent() {
        let cfg: AppConfig = serde_json::from_str(r#"{"autostart": true}"#).expect("must parse");
        assert!(cfg.status_rules.quiet_hours.is_empty());
        assert!(cfg.status_rules.track_rules.is_empty());
        assert_eq!(cfg.schema_version, 1);
        assert_eq!(AppConfig::default().status_rules.quiet_hours.len(), 0);
    }

    /// Issue #432: rules round-trip through serde with per-field defaults
    /// (a hand-edited entry missing optional fields still loads).
    #[test]
    fn test_status_rules_round_trip() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{"status_rules": {"quiet_hours": [{"enabled": true, "start_minutes": 1320}], "track_rules": [{"enabled": true, "artist_substring": "lofi"}]}}"#,
        )
        .expect("must parse");
        assert_eq!(cfg.status_rules.quiet_hours.len(), 1);
        assert_eq!(cfg.status_rules.quiet_hours[0].start_minutes, 1320);
        // `end_minutes` absent → default_quiet_end (07:00).
        assert_eq!(cfg.status_rules.quiet_hours[0].end_minutes, 420);
        assert!(cfg.status_rules.quiet_hours[0].days.is_empty());
        assert_eq!(cfg.status_rules.track_rules.len(), 1);
        assert!(cfg.status_rules.track_rules[0]
            .replacement_status
            .is_empty());
        let json = serde_json::to_string(&cfg).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.status_rules.quiet_hours.len(), 1);
        assert_eq!(back.status_rules.track_rules.len(), 1);
    }

    // ---------------------------------------------------------------
    // Issue #866: preferred-presence defaults + clamp + gating.
    // ---------------------------------------------------------------

    /// Issue #866: a hand-edited config that omits `teams.preferred_presence`
    /// loads the documented OFF defaults — every field absent, expiry at the
    /// 60-minute default, and the resolved pair `None`. The absence is the
    /// "feature off" state, not a failure.
    #[test]
    fn test_preferred_presence_default_when_absent() {
        let cfg: AppConfig = serde_json::from_str(r#"{}"#).expect("must parse");
        assert!(!cfg.teams.preferred_presence.enabled);
        assert!(cfg.teams.preferred_presence.availability.is_empty());
        assert!(cfg.teams.preferred_presence.activity.is_empty());
        assert_eq!(cfg.teams.preferred_presence.expiry_minutes, 60);
    }

    /// Issue #866: `clamp_preferred_presence` rejects unsupported pairs
    /// exactly like `clamp_rules` does — clearing BOTH fields and disabling
    /// the feature so a Graph POST would never 4xx. The expiry is clamped to
    /// `5..=720`.
    #[test]
    fn test_preferred_presence_clamp_rejects_unsupported_pair() {
        let mut pp = PreferredPresenceConfig {
            enabled: true,
            availability: "Busy".to_string(),
            activity: "InACall".to_string(),
            expiry_minutes: 60,
            ..PreferredPresenceConfig::default()
        };
        clamp_preferred_presence(&mut pp);
        assert!(pp.enabled);
        assert_eq!(pp.availability, "Busy");
        assert_eq!(pp.activity, "InACall");

        // Case-insensitive normalization mirrors `normalize_presence_pair`.
        let mut pp = PreferredPresenceConfig {
            enabled: true,
            availability: "busy".to_string(),
            activity: "inacall".to_string(),
            expiry_minutes: 60,
            ..PreferredPresenceConfig::default()
        };
        clamp_preferred_presence(&mut pp);
        assert_eq!(pp.availability, "Busy");
        assert_eq!(pp.activity, "InACall");

        // Unsupported pair → both fields empty, feature disabled.
        let mut pp = PreferredPresenceConfig {
            enabled: true,
            availability: "Busy".to_string(),
            activity: "DoNotDisturb".to_string(),
            expiry_minutes: 60,
            ..PreferredPresenceConfig::default()
        };
        clamp_preferred_presence(&mut pp);
        assert!(!pp.enabled, "unsupported pair must disable the feature");
        assert!(pp.availability.is_empty());
        assert!(pp.activity.is_empty());

        // Expiry clamping is independent of the pair.
        let mut pp = PreferredPresenceConfig {
            enabled: true,
            availability: "Busy".to_string(),
            activity: "InACall".to_string(),
            expiry_minutes: 1,
            ..PreferredPresenceConfig::default()
        };
        clamp_preferred_presence(&mut pp);
        assert_eq!(pp.expiry_minutes, 5);
        let mut pp = PreferredPresenceConfig {
            enabled: true,
            availability: "Busy".to_string(),
            activity: "InACall".to_string(),
            expiry_minutes: 9999,
            ..PreferredPresenceConfig::default()
        };
        clamp_preferred_presence(&mut pp);
        assert_eq!(pp.expiry_minutes, 720);
    }

    /// Issue #866: `preferred_presence_pair` is the single source of truth
    /// for "do we have a preferred pair to POST?" — disabled, empty, and
    /// respect-manual-status all return `None`. A valid Busy/InACall pair
    /// returns the canonical pair.
    #[test]
    fn test_preferred_presence_pair_gates_on_manual_status() {
        let mut teams = TeamsConfig {
            preferred_presence: PreferredPresenceConfig {
                enabled: true,
                availability: "Busy".to_string(),
                activity: "InACall".to_string(),
                expiry_minutes: 60,
                ..PreferredPresenceConfig::default()
            },
            ..TeamsConfig::default()
        };
        let pair = preferred_presence_pair(&teams, false).expect("must resolve");
        assert_eq!(pair.availability, "Busy");
        assert_eq!(pair.activity, "InACall");
        // Respect-manual-status wins: the user's own bubble is never
        // overridden by an opt-in preferred presence.
        assert!(preferred_presence_pair(&teams, true).is_none());

        // Disabled feature returns None even with a valid pair.
        teams.preferred_presence.enabled = false;
        assert!(preferred_presence_pair(&teams, false).is_none());
    }

    /// Issue #866: the expiry is the documented ISO-8601 `PT<minutes>M`
    /// shape `setUserPreferredPresence` expects. The `5` floor matches the
    /// clamp — a stored `0` can never survive the round-trip and reach
    /// here, but we still do not pass `PT0M` to Graph if a stale config
    /// does.
    #[test]
    fn test_preferred_presence_expiry_duration_is_iso8601() {
        let mut teams = TeamsConfig::default();
        teams.preferred_presence.expiry_minutes = 60;
        assert_eq!(preferred_presence_expiry_duration(&teams), "PT60M");
        teams.preferred_presence.expiry_minutes = 5;
        assert_eq!(preferred_presence_expiry_duration(&teams), "PT5M");
        // Stale `0` cannot round-trip past clamp_preferred_presence,
        // but the helper still floors it rather than emitting PT0M.
        teams.preferred_presence.expiry_minutes = 0;
        assert_eq!(preferred_presence_expiry_duration(&teams), "PT5M");
    }

    // ---------------------------------------------------------------
    // CfgDiag#5 (#540): `minimum <= default <= maximum` is an invariant.
    // ---------------------------------------------------------------

    /// Hostile-but-persistable inputs (the Settings number inputs' `min`/
    /// `max` attributes do not constrain a typed value, and `clamp_polling`
    /// is the only normalizer) must land on the invariant, not merely on
    /// each field's own range.
    #[test]
    fn test_clamp_polling_pins_default_between_min_and_max() {
        let mut polling = PollingConfig {
            default_interval_seconds: 300,
            minimum_interval_seconds: 10,
            max_interval_seconds: 30,
            expiry_buffer_seconds: 10,
            pause_backoff_max_seconds: 300,
            ..PollingConfig::default()
        };
        clamp_polling(&mut polling);
        assert_eq!(polling.max_interval_seconds, 30);
        assert_eq!(polling.minimum_interval_seconds, 10);
        assert_eq!(
            polling.default_interval_seconds, 30,
            "a default above the max must be pulled down to the max"
        );

        // The mirror case: a default below the minimum.
        let mut polling = PollingConfig {
            default_interval_seconds: 5,
            minimum_interval_seconds: 20,
            max_interval_seconds: 60,
            expiry_buffer_seconds: 10,
            pause_backoff_max_seconds: 300,
            ..PollingConfig::default()
        };
        clamp_polling(&mut polling);
        assert_eq!(polling.default_interval_seconds, 20);

        // Out-of-range fields still clamp to their own bands first, and the
        // invariant survives the combination.
        let mut polling = PollingConfig {
            default_interval_seconds: 9999,
            minimum_interval_seconds: 9999,
            max_interval_seconds: 9999,
            expiry_buffer_seconds: 9999,
            pause_backoff_max_seconds: 9999,
            ..PollingConfig::default()
        };
        clamp_polling(&mut polling);
        assert!(polling.minimum_interval_seconds <= polling.default_interval_seconds);
        assert!(polling.default_interval_seconds <= polling.max_interval_seconds);
        assert_eq!(polling.expiry_buffer_seconds, 60);
        assert_eq!(polling.pause_backoff_max_seconds, 3600);

        // CfgDiag#3(c): the backoff ceiling has a floor too.
        let mut polling = PollingConfig {
            pause_backoff_max_seconds: 1,
            ..PollingConfig::default()
        };
        clamp_polling(&mut polling);
        assert_eq!(polling.pause_backoff_max_seconds, 60);
    }

    // ---------------------------------------------------------------
    // CfgDiag#0 (#535): a partial-knowledge caller cannot lose fields.
    // ---------------------------------------------------------------

    /// A config that differs from `AppConfig::default()` in EVERY field a
    /// patch can name, so an accidental whole-section overwrite shows up as a
    /// changed value instead of coincidentally matching a default.
    fn non_default_config() -> AppConfig {
        let mut cfg = AppConfig {
            autostart: true,
            schema_version: SCHEMA_VERSION,
            ..AppConfig::default()
        };
        cfg.spotify.client_id = "stored-client-id".to_string();
        cfg.spotify.redirect_uri = "presencejam://stored".to_string();
        cfg.teams.status_format = "Stored {artist}".to_string();
        cfg.teams.clear_on_pause = false;
        cfg.teams.profanity_filter = false;
        cfg.teams.profanity_placeholder = "Custom placeholder".to_string();
        cfg.teams.start_minimized = true;
        cfg.teams.availability_sync = true;
        cfg.teams.presence_gate = false;
        cfg.teams.profanity_extra_words = vec!["spam".to_string()];
        cfg.teams.paused_status_format = "Custom paused".to_string();
        cfg.teams.stopped_status_format = "Custom stopped".to_string();
        cfg.polling.default_interval_seconds = 45;
        cfg.polling.minimum_interval_seconds = 20;
        cfg.polling.max_interval_seconds = 120;
        cfg.polling.expiry_buffer_seconds = 5;
        cfg.polling.pause_backoff_max_seconds = 600;
        cfg.logging.enabled = false;
        cfg.logging.log_level = "Debug".to_string();
        cfg.status_rules.quiet_hours.push(QuietHoursEntry {
            enabled: true,
            start_minutes: 1320,
            end_minutes: 420,
            days: vec![1, 2, 3, 4, 5],
            replacement_status: "Busy".to_string(),
            pause_polling: true,
            ..QuietHoursEntry::default()
        });
        cfg.status_rules.track_rules.push(TrackRuleEntry {
            enabled: true,
            artist_substring: "lofi".to_string(),
            track_substring: String::new(),
            days: vec![6, 7],
            start_minutes: 480,
            end_minutes: 1020,
            replacement_status: "Focus".to_string(),
            presence_availability: "DoNotDisturb".to_string(),
            presence_activity: "Presenting".to_string(),
            ..TrackRuleEntry::default()
        });
        cfg.extra
            .insert("future_key".to_string(), serde_json::json!({"a": 1}));
        cfg
    }

    /// Every leaf path (`spotify.client_id`, `status_rules.quiet_hours`, …)
    /// whose value differs between two configs.
    ///
    /// Comparing the whole document this way is what makes "the patch touched
    /// nothing else" exhaustive, instead of a list of fields somebody
    /// remembered to assert.
    fn changed_paths(before: &AppConfig, after: &AppConfig) -> Vec<String> {
        fn join(prefix: &str, key: &str) -> String {
            if prefix.is_empty() {
                key.to_string()
            } else {
                format!("{prefix}.{key}")
            }
        }
        fn walk(a: &serde_json::Value, b: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
            match (a, b) {
                (serde_json::Value::Object(a_map), serde_json::Value::Object(b_map)) => {
                    for (key, a_value) in a_map {
                        let path = join(prefix, key);
                        match b_map.get(key) {
                            Some(b_value) => walk(a_value, b_value, &path, out),
                            None => out.push(path),
                        }
                    }
                    for key in b_map.keys() {
                        if !a_map.contains_key(key) {
                            out.push(join(prefix, key));
                        }
                    }
                }
                // A list is a leaf: a patch either replaces it or does not
                // name it, there is no per-entry addressing.
                _ => {
                    if a != b {
                        out.push(prefix.to_string());
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(
            &serde_json::to_value(before).expect("must serialize"),
            &serde_json::to_value(after).expect("must serialize"),
            "",
            &mut out,
        );
        out.sort();
        out
    }

    /// The #531 failure mode in one test: a patch that carries only
    /// `teams.status_format` must leave the stored quiet hours, track rules,
    /// `start_minimized` and logging exactly as they were.
    ///
    /// Asserted through the same `clamped_config` + `stamp_schema_version` +
    /// serde path `save_config` uses — writing the real config.json from a
    /// unit test is not acceptable, and the merge itself is pure.
    #[test]
    fn test_apply_patch_preserves_unpatched_fields() {
        let base = non_default_config();

        let patch: ConfigPatch =
            serde_json::from_str(r#"{"teams": {"status_format": "NEW {track}"}}"#)
                .expect("patch must parse");
        assert!(patch.spotify.is_none());
        assert!(patch.autostart.is_none());

        let mut merged = base.clone();
        apply_patch(&mut merged, &patch);

        // The one patched field took the new value, and it is the ONLY leaf
        // in the whole document that moved.
        assert_eq!(merged.teams.status_format, "NEW {track}");
        assert_eq!(
            changed_paths(&base, &merged),
            vec!["teams.status_format".to_string()],
            "a patch naming one field must not move any other"
        );

        // The losses issue #531 reports, named explicitly.
        assert!(merged.teams.start_minimized);
        assert!(!merged.teams.profanity_filter);
        assert_eq!(merged.teams.profanity_extra_words, vec!["spam".to_string()]);
        assert_eq!(merged.teams.profanity_placeholder, "Custom placeholder");
        assert!(!merged.teams.clear_on_pause);
        assert!(merged.teams.availability_sync);
        assert!(!merged.teams.presence_gate);
        assert_eq!(merged.logging.log_level, "Debug");
        assert!(!merged.logging.enabled);
        assert!(merged.autostart);
        assert_eq!(merged.spotify.client_id, "stored-client-id");
        assert_eq!(merged.spotify.redirect_uri, "presencejam://stored");
        assert_eq!(merged.polling.default_interval_seconds, 45);
        assert_eq!(merged.polling.minimum_interval_seconds, 20);
        assert_eq!(merged.polling.max_interval_seconds, 120);
        assert_eq!(merged.polling.expiry_buffer_seconds, 5);
        assert_eq!(merged.polling.pause_backoff_max_seconds, 600);
        assert_eq!(merged.status_rules.quiet_hours.len(), 1);
        assert_eq!(
            merged.status_rules.quiet_hours[0].replacement_status,
            "Busy"
        );
        assert_eq!(merged.status_rules.track_rules.len(), 1);
        assert_eq!(
            merged.status_rules.track_rules[0].replacement_status,
            "Focus"
        );

        // What actually lands on disk round-trips with the same values.
        let mut persisted = clamped_config(&merged);
        stamp_schema_version(&mut persisted);
        let json = serde_json::to_string_pretty(&persisted).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.status_rules.quiet_hours.len(), 1);
        assert_eq!(back.status_rules.track_rules.len(), 1);
        assert!(back.teams.start_minimized);
        assert!(!back.teams.profanity_filter);
        assert_eq!(back.teams.status_format, "NEW {track}");
        assert_eq!(back.schema_version, SCHEMA_VERSION);
    }

    /// Naming ONE field inside a section must leave that section's other
    /// fields alone — the case a whole-struct `Option<TeamsConfig>` patch
    /// shape gets wrong, because a partial JSON object deserializes the
    /// omitted fields to their defaults and the section is then assigned
    /// wholesale.
    ///
    /// Table-driven over every patchable field (plus "named a section but no
    /// field", which must be a no-op), asserting the exact set of leaves that
    /// moved.
    #[test]
    fn test_apply_patch_merges_each_section_field_by_field() {
        let cases: &[(&str, &[&str])] = &[
            (
                r#"{"spotify": {"client_id": "new"}}"#,
                &["spotify.client_id"],
            ),
            (
                r#"{"spotify": {"redirect_uri": "presencejam://new"}}"#,
                &["spotify.redirect_uri"],
            ),
            (
                r#"{"teams": {"status_format": "new"}}"#,
                &["teams.status_format"],
            ),
            (
                r#"{"teams": {"clear_on_pause": true}}"#,
                &["teams.clear_on_pause"],
            ),
            (
                r#"{"teams": {"profanity_filter": true}}"#,
                &["teams.profanity_filter"],
            ),
            (
                r#"{"teams": {"profanity_placeholder": "new"}}"#,
                &["teams.profanity_placeholder"],
            ),
            (
                r#"{"teams": {"start_minimized": false}}"#,
                &["teams.start_minimized"],
            ),
            (
                r#"{"teams": {"availability_sync": false}}"#,
                &["teams.availability_sync"],
            ),
            (
                r#"{"teams": {"presence_gate": true}}"#,
                &["teams.presence_gate"],
            ),
            (
                r#"{"teams": {"profanity_extra_words": ["a"]}}"#,
                &["teams.profanity_extra_words"],
            ),
            (
                r#"{"teams": {"paused_status_format": "BRB"}}"#,
                &["teams.paused_status_format"],
            ),
            (
                r#"{"teams": {"stopped_status_format": "Idle"}}"#,
                &["teams.stopped_status_format"],
            ),
            (
                r#"{"polling": {"default_interval_seconds": 100}}"#,
                &["polling.default_interval_seconds"],
            ),
            (
                r#"{"polling": {"minimum_interval_seconds": 25}}"#,
                &["polling.minimum_interval_seconds"],
            ),
            (
                r#"{"polling": {"max_interval_seconds": 200}}"#,
                &["polling.max_interval_seconds"],
            ),
            (
                r#"{"polling": {"expiry_buffer_seconds": 30}}"#,
                &["polling.expiry_buffer_seconds"],
            ),
            (
                r#"{"polling": {"pause_backoff_max_seconds": 1200}}"#,
                &["polling.pause_backoff_max_seconds"],
            ),
            (r#"{"logging": {"enabled": true}}"#, &["logging.enabled"]),
            (
                r#"{"logging": {"log_level": "Trace"}}"#,
                &["logging.log_level"],
            ),
            (
                r#"{"status_rules": {"quiet_hours": []}}"#,
                &["status_rules.quiet_hours"],
            ),
            (
                r#"{"status_rules": {"track_rules": []}}"#,
                &["status_rules.track_rules"],
            ),
            (r#"{"autostart": false}"#, &["autostart"]),
            // Naming a section without naming a field is a no-op.
            (r#"{}"#, &[]),
            (r#"{"spotify": {}}"#, &[]),
            (r#"{"teams": {}}"#, &[]),
            (r#"{"polling": {}}"#, &[]),
            (r#"{"logging": {}}"#, &[]),
            (r#"{"status_rules": {}}"#, &[]),
        ];

        for (json, expected) in cases {
            let base = non_default_config();
            let patch: ConfigPatch = serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("patch {json} must parse: {e}"));
            let mut merged = base.clone();
            apply_patch(&mut merged, &patch);
            let expected: Vec<String> = expected.iter().map(|path| (*path).to_string()).collect();
            assert_eq!(
                changed_paths(&base, &merged),
                expected,
                "patch {json} moved the wrong set of fields"
            );
        }
    }

    /// A named list is replaced wholesale: the patch value is what lands, not
    /// a merge with the stored list, and the sibling list is untouched.
    #[test]
    fn test_apply_patch_replaces_a_named_list() {
        let mut cfg = non_default_config();
        let patch: ConfigPatch = serde_json::from_str(
            r#"{"status_rules": {"quiet_hours": [{"enabled": false, "replacement_status": "Away"}]}}"#,
        )
        .expect("must parse");

        apply_patch(&mut cfg, &patch);

        assert_eq!(cfg.status_rules.quiet_hours.len(), 1);
        assert!(!cfg.status_rules.quiet_hours[0].enabled);
        assert_eq!(cfg.status_rules.quiet_hours[0].replacement_status, "Away");
        assert_eq!(cfg.status_rules.track_rules.len(), 1);
    }

    /// A patch that names nothing must change nothing (`update_config`'s
    /// base-config path relies on this).
    #[test]
    fn test_apply_patch_is_identity_when_empty() {
        let base = non_default_config();
        let mut merged = base.clone();
        apply_patch(&mut merged, &ConfigPatch::default());
        assert!(
            changed_paths(&base, &merged).is_empty(),
            "an empty patch must be an identity"
        );
    }

    /// `extra` (the #379 forward-compat bucket) is never touched by a patch.
    #[test]
    fn test_apply_patch_leaves_extra_untouched() {
        let base = non_default_config();
        let mut merged = base.clone();
        let patch: ConfigPatch =
            serde_json::from_str(r#"{"autostart": false}"#).expect("must parse");
        apply_patch(&mut merged, &patch);
        assert!(!merged.autostart);
        assert_eq!(
            merged.extra.get("future_key"),
            Some(&serde_json::json!({"a": 1}))
        );
        assert_eq!(base.extra.get("future_key"), merged.extra.get("future_key"));
    }

    // ---------------------------------------------------------------
    // Issue #767: ConfigPatch covers every writable section, and the
    // patch types are exported so the frontend cannot drift from them.
    // ---------------------------------------------------------------

    /// Issue #767: every section the app can write is reachable through
    /// `ConfigPatch`. Before this, `teams` carried neither `respect_manual_status`
    /// nor `gate_when_out_of_office`, and `updates`, `playback`, `locale` and
    /// `shortcuts` were absent from the patch entirely — so a caller holding
    /// one of those settings had exactly one way to change it, the
    /// whole-document `save_config` that drops whatever it did not send.
    ///
    /// Asserted by round-tripping a patch through serde and through
    /// `clamped_config` (the write path), so a field that is declared but not
    /// merged, or merged but not re-clamped, both fail here.
    #[test]
    fn test_patch_reaches_every_section_it_declares() {
        let base = non_default_config();

        // Every key below names a section and a field that did NOT exist on
        // `ConfigPatch` before this change.
        let raw = r#"{
            "teams": {
                "respect_manual_status": false,
                "gate_when_out_of_office": true,
                "gate_when_presenting": true,
                "idle_away_after_seconds": 900,
                "pre_meeting_suppress_minutes": 15
            },
            "logging": {"max_file_size_mb": 42, "keep_files": 9, "presence_history": true},
            "updates": {"channel": "beta"},
            "playback": {"source": "spotify"},
            "locale": "de",
            "shortcuts": {"toggle_playback": "Ctrl+Alt+M", "toggle_sync": ""}
        }"#;
        let patch: ConfigPatch = serde_json::from_str(raw).expect("patch must parse");

        let mut merged = base.clone();
        apply_patch(&mut merged, &patch);

        assert!(!merged.teams.respect_manual_status, "gate must be settable");
        assert!(merged.teams.gate_when_out_of_office);
        assert!(merged.teams.gate_when_presenting);
        assert_eq!(merged.teams.idle_away_after_seconds, 900);
        assert_eq!(merged.teams.pre_meeting_suppress_minutes, 15);
        assert_eq!(merged.logging.max_file_size_mb, 42);
        assert_eq!(merged.logging.keep_files, 9);
        assert!(merged.logging.presence_history);
        assert_eq!(merged.updates.channel, UpdateChannel::Beta);
        assert_eq!(
            merged.playback.source,
            crate::sources::PlaybackSourceKind::Spotify
        );
        assert_eq!(merged.locale.as_deref(), Some("de"));
        assert_eq!(
            merged.shortcuts.toggle_playback.as_deref(),
            Some("Ctrl+Alt+M")
        );

        // A blank binding is the documented "unbound" spelling and is stored
        // verbatim: `configured_binding` reads it as unbound when planning a
        // registration, and rewriting it here would make the Settings field
        // lie about what the document actually holds. The clamp bounds LENGTH
        // only — see `clamp_shortcuts`.
        let persisted = clamped_config(&merged);
        assert_eq!(persisted.shortcuts.toggle_sync.as_deref(), Some(""));

        // The whole thing survives the disk round trip with the same values,
        // which is what proves the patch struct and the config struct agree
        // on the key names rather than merely compiling.
        let json = serde_json::to_string_pretty(&persisted).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.updates.channel, UpdateChannel::Beta);
        assert_eq!(back.locale.as_deref(), Some("de"));
        assert!(back.teams.gate_when_out_of_office);
        assert_eq!(back.logging.keep_files, 9);
    }

    /// Issue #767, the drift guard the issue asks for: "a test fails when a
    /// patch field is dropped from its struct."
    ///
    /// A dropped field is invisible to the compiler (serde ignores an unknown
    /// key), and the symptom is a setting that appears to save and then
    /// reverts. This asserts the exact set of leaves a patch moves, so
    /// removing any one field from any one patch struct turns this red.
    #[test]
    fn test_every_patch_field_is_actually_merged() {
        // (patch JSON, the leaves it must move)
        let cases: &[(&str, &[&str])] = &[
            (
                r#"{"teams": {"respect_manual_status": false}}"#,
                &["teams.respect_manual_status"],
            ),
            (
                r#"{"teams": {"gate_when_out_of_office": true}}"#,
                &["teams.gate_when_out_of_office"],
            ),
            (
                r#"{"teams": {"gate_when_presenting": true}}"#,
                &["teams.gate_when_presenting"],
            ),
            (
                r#"{"teams": {"idle_away_after_seconds": 900}}"#,
                &["teams.idle_away_after_seconds"],
            ),
            (
                r#"{"teams": {"pre_meeting_suppress_minutes": 15}}"#,
                &["teams.pre_meeting_suppress_minutes"],
            ),
            (
                r#"{"logging": {"max_file_size_mb": 42}}"#,
                &["logging.max_file_size_mb"],
            ),
            (r#"{"logging": {"keep_files": 9}}"#, &["logging.keep_files"]),
            (
                r#"{"logging": {"presence_history": true}}"#,
                &["logging.presence_history"],
            ),
            (r#"{"updates": {"channel": "beta"}}"#, &["updates.channel"]),
            (
                r#"{"playback": {"source": "spotify"}}"#,
                &["playback.source"],
            ),
            (r#"{"locale": "de"}"#, &["locale"]),
            (
                r#"{"shortcuts": {"toggle_playback": "Ctrl+Alt+M"}}"#,
                &["shortcuts.toggle_playback"],
            ),
            (
                r#"{"shortcuts": {"toggle_sync": "Ctrl+Alt+N"}}"#,
                &["shortcuts.toggle_sync"],
            ),
        ];

        for (json, expected) in cases {
            let base = non_default_config();
            let patch: ConfigPatch = serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("patch {json} must parse: {e}"));
            let mut merged = base.clone();
            apply_patch(&mut merged, &patch);
            let expected: Vec<String> = expected.iter().map(|p| (*p).to_string()).collect();
            assert_eq!(
                changed_paths(&base, &merged),
                expected,
                "patch {json} did not move exactly the leaf it names — a patch \
                 field is declared but never merged"
            );
        }
    }

    /// Issue #767's acceptance criterion: a partial patch leaves unmentioned
    /// sections byte-identical — `snooze_until` and `notifications` named
    /// explicitly, because those two were the ones that had to move off the
    /// whole-document save path.
    #[test]
    fn test_partial_patch_leaves_snooze_and_notifications_untouched() {
        let mut base = non_default_config();
        base.snooze_until = Some("2099-01-01T00:00:00Z".to_string());
        base.notifications.track_change = false;
        base.notifications.sync_stopped = false;
        base.notifications.auth_required = false;
        base.notifications.update_staged = false;

        let patch: ConfigPatch =
            serde_json::from_str(r#"{"teams": {"respect_manual_status": false}}"#)
                .expect("patch must parse");
        let mut merged = base.clone();
        apply_patch(&mut merged, &patch);

        assert_eq!(
            merged.snooze_until.as_deref(),
            Some("2099-01-01T00:00:00Z"),
            "a patch that names no snooze must not touch the deadline"
        );
        assert!(!merged.notifications.track_change);
        assert!(!merged.notifications.sync_stopped);
        assert!(!merged.notifications.auth_required);
        assert!(!merged.notifications.update_staged);

        // Byte-identical, not merely equal in the fields named above: the
        // whole document except the one patched leaf is unchanged.
        assert_eq!(
            changed_paths(&base, &merged),
            vec!["teams.respect_manual_status".to_string()]
        );
    }

    /// Issue #767: `locale` is a three-state field like `snooze_until` — absent
    /// leaves the tag alone, `null` clears it, a string sets it — and
    /// `clamp_locale` canonicalises it on the way to disk so a patch cannot
    /// persist a tag the app cannot render.
    #[test]
    fn test_locale_patch_is_three_state_and_canonicalised() {
        let base_locale = Some("de".to_string());

        let mut untouched = non_default_config();
        untouched.locale = base_locale.clone();
        apply_patch(
            &mut untouched,
            &serde_json::from_str(r#"{"teams": {"clear_on_pause": true}}"#).unwrap(),
        );
        assert_eq!(
            untouched.locale, base_locale,
            "absent must not touch locale"
        );

        let mut cleared = non_default_config();
        cleared.locale = base_locale.clone();
        apply_patch(
            &mut cleared,
            &serde_json::from_str(r#"{"locale": null}"#).unwrap(),
        );
        assert_eq!(cleared.locale, None, "an explicit null clears the tag");

        let mut set = non_default_config();
        set.locale = base_locale.clone();
        apply_patch(
            &mut set,
            &serde_json::from_str(r#"{"locale": "fr"}"#).unwrap(),
        );
        assert_eq!(set.locale.as_deref(), Some("fr"));

        // `clamp_locale` reduces a regional variant to a shipped dictionary and
        // maps an unknown tag to English — the same function `set_locale` uses,
        // so the two write paths cannot disagree.
        let mut regional = non_default_config();
        regional.locale = Some("de-AT".to_string());
        assert_eq!(clamped_config(&regional).locale.as_deref(), Some("de"));
        let mut brazilian = non_default_config();
        brazilian.locale = Some("pt-BR".to_string());
        assert_eq!(clamped_config(&brazilian).locale.as_deref(), Some("pt"));
        let mut unknown = non_default_config();
        unknown.locale = Some("ja".to_string());
        assert_eq!(clamped_config(&unknown).locale.as_deref(), Some("en"));
        let mut blank = non_default_config();
        blank.locale = Some(String::new());
        assert_eq!(clamped_config(&blank).locale.as_deref(), Some("en"));
        let mut absent = non_default_config();
        absent.locale = None;
        assert_eq!(
            clamped_config(&absent).locale,
            None,
            "\"follow the OS\" is a real state, not a synonym for English"
        );
    }

    /// Issue #767: an unrecognised `updates.channel` in a patch REJECTS the
    /// patch, so a caller whose channel list drifted from the binary's
    /// changes nothing rather than silently storing `stable`.
    #[test]
    fn test_unknown_update_channel_in_a_patch_is_refused() {
        assert!(
            serde_json::from_str::<ConfigPatch>(r#"{"updates": {"channel": "nightly"}}"#).is_err(),
            "a channel outside the enum must fail the patch, not default to stable"
        );
        // The lenient document read is unchanged by this (#678): a config FILE
        // carrying an unknown channel keeps every other setting.
        let (dir, path) = temp_config_file(
            "patch-channel-refused",
            r#"{"updates": {"channel": "nightly", "future_channel_key": 1}, "autostart": true}"#,
        );
        let caches = crate::state::AppCaches::new();
        let cfg =
            load_config_from(&caches, &path).expect("a bad channel must not fail the document");
        assert_eq!(cfg.updates.channel, UpdateChannel::Stable);
        assert!(
            cfg.updates.extra.contains_key("future_channel_key"),
            "its sibling unknown key is still retained"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #767: `clamp_shortcuts` bounds a binding's LENGTH and leaves
    /// everything else alone.
    ///
    /// A blank binding is deliberately NOT normalised to `None`: the
    /// registration planner (`configured_binding`) is what decides that blank
    /// means unbound, and a writer that rewrote it would make the Settings
    /// field render something other than what is stored — pinned by
    /// `shortcut_bindings_round_trip_through_json`.
    #[test]
    fn test_clamp_shortcuts_bounds_length_and_preserves_blanks() {
        let mut blank = non_default_config();
        blank.shortcuts.toggle_playback = Some("CmdOrCtrl+Shift+P".to_string());
        blank.shortcuts.toggle_sync = Some("  ".to_string());
        let clamped = clamped_config(&blank);
        assert_eq!(
            clamped.shortcuts.toggle_playback.as_deref(),
            Some("CmdOrCtrl+Shift+P")
        );
        assert_eq!(
            clamped.shortcuts.toggle_sync.as_deref(),
            Some("  "),
            "a blank binding is stored verbatim — the registrar, not the \
             writer, decides that blank means unbound"
        );

        let mut padded = non_default_config();
        padded.shortcuts.toggle_playback = Some("  Ctrl+Alt+P  ".to_string());
        assert_eq!(
            clamped_config(&padded).shortcuts.toggle_playback.as_deref(),
            Some("  Ctrl+Alt+P  "),
            "surrounding whitespace is part of what the user stored"
        );

        // An over-long binding is bounded, so a pasted paragraph cannot become
        // the stored document.
        let mut huge = non_default_config();
        huge.shortcuts.toggle_playback = Some("x".repeat(MAX_SHORTCUT_BINDING_CHARS * 3));
        let bounded = clamped_config(&huge);
        assert_eq!(
            bounded
                .shortcuts
                .toggle_playback
                .as_deref()
                .map(str::chars)
                .map(Iterator::count),
            Some(MAX_SHORTCUT_BINDING_CHARS),
            "a binding longer than the cap must be truncated to it"
        );
        // The sibling slot is untouched by that clamp.
        assert_eq!(
            bounded.shortcuts.toggle_sync,
            non_default_config().shortcuts.toggle_sync
        );
    }

    /// Issue #767: the patch types carry `#[ts(export)]`, so the generated
    /// `ConfigPatch` the frontend aliases is the SAME contract the backend
    /// deserializes — rather than a hand-written mirror that could assert a
    /// coverage the backend does not have.
    ///
    /// Asserted through the real behaviour the export exists for: a payload
    /// in the generated shape deserializes into the Rust patch, and every
    /// section key it names survives to the merged document.
    #[test]
    fn test_generated_patch_shape_deserializes_and_merges() {
        // Mirrors `src/lib/types-generated/ConfigPatch.ts`: every section key
        // optional, snake_case as serde names them.
        let patch: ConfigPatch = serde_json::from_str(
            r#"{"spotify": {"client_id": "a"}, "teams": {"status_format": "x"},
                "polling": {"default_interval_seconds": 45}, "logging": {"enabled": true},
                "updates": {"channel": "stable"}, "playback": {"source": "auto"},
                "autostart": false, "notifications": {"track_change": true},
                "locale": "fr", "snooze_until": null,
                "status_rules": {"quiet_hours": []},
                "shortcuts": {"toggle_sync": "Ctrl+Alt+S"}}"#,
        )
        .expect("the generated ConfigPatch shape must deserialize");

        let mut merged = non_default_config();
        apply_patch(&mut merged, &patch);
        assert_eq!(merged.spotify.client_id, "a");
        assert_eq!(merged.locale.as_deref(), Some("fr"));
        assert_eq!(merged.shortcuts.toggle_sync.as_deref(), Some("Ctrl+Alt+S"));
        assert!(!merged.autostart);
    }

    // ---------------------------------------------------------------
    // CfgDiag#1 (#536): schema_version is the binary's to set.
    // ---------------------------------------------------------------

    /// A client payload stuck on the pre-4.6 version must not lower the
    /// version that is actually persisted.
    #[test]
    fn test_stamp_schema_version_overrides_stale_client_value() {
        let mut cfg: AppConfig =
            serde_json::from_str(r#"{"schema_version": 1}"#).expect("must parse");
        assert_eq!(cfg.schema_version, 1);
        stamp_schema_version(&mut cfg);
        assert_eq!(cfg.schema_version, SCHEMA_VERSION);
        const { assert!(SCHEMA_VERSION > 1, "4.6 must have bumped the schema") };
    }

    /// The dispatcher raises an old (or absent → v1) file to the current
    /// version and never relabels a file written by a newer binary.
    #[test]
    fn test_migrate_config_raises_old_and_preserves_newer() {
        let mut old = AppConfig {
            schema_version: 1,
            ..AppConfig::default()
        };
        let from = old.schema_version;
        migrate_config(&mut old, from);
        assert_eq!(old.schema_version, SCHEMA_VERSION);

        let mut newer = AppConfig {
            schema_version: 99,
            ..AppConfig::default()
        };
        let from = newer.schema_version;
        migrate_config(&mut newer, from);
        assert_eq!(
            newer.schema_version, 99,
            "a newer file must not be relabelled downward"
        );

        // v0 (a hand-edited or truncated key) is treated as old, not newer.
        let mut zero = AppConfig {
            schema_version: 0,
            ..AppConfig::default()
        };
        let from = zero.schema_version;
        migrate_config(&mut zero, from);
        assert_eq!(zero.schema_version, SCHEMA_VERSION);
    }

    // ---------------------------------------------------------------
    // CfgDiag#4 (#539): logging changes take effect without a restart.
    // ---------------------------------------------------------------

    #[test]
    fn test_apply_log_level_maps_enabled_and_level() {
        // `log::set_max_level` is process-global: serialize this test against
        // itself so a parallel sibling cannot observe the transient value.
        static LOG_LEVEL_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
        let _guard = LOG_LEVEL_LOCK.lock();

        apply_log_level(&LoggingConfig {
            enabled: false,
            log_level: "Debug".to_string(),
            ..LoggingConfig::default()
        });
        assert_eq!(
            log::max_level(),
            log::LevelFilter::Off,
            "logging.enabled = false must silence the logger immediately"
        );

        apply_log_level(&LoggingConfig {
            enabled: true,
            log_level: "DEBUG".to_string(),
            ..LoggingConfig::default()
        });
        assert_eq!(log::max_level(), log::LevelFilter::Debug);

        apply_log_level(&LoggingConfig {
            enabled: true,
            log_level: "not-a-level".to_string(),
            ..LoggingConfig::default()
        });
        assert_eq!(
            log::max_level(),
            log::LevelFilter::Info,
            "an unrecognised level must never disable logging"
        );

        // Restore the level the rest of the suite runs under.
        apply_log_level(&LoggingConfig::default());
        assert_eq!(log::max_level(), log::LevelFilter::Info);
    }

    // ---------------------------------------------------------------
    // CfgDiag#3 (#538): the three additive 4.6 config fields.
    // ---------------------------------------------------------------

    /// Pre-4.6 files keep loading: every new field carries a serde default.
    #[test]
    fn test_four_six_additions_default_on_old_files() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{"teams": {}, "polling": {}, "status_rules": {"quiet_hours": [{"enabled": true}]}}"#,
        )
        .expect("a pre-4.6 document must still parse");
        assert!(cfg.teams.profanity_extra_words.is_empty());
        assert_eq!(cfg.polling.pause_backoff_max_seconds, 300);
        assert!(cfg.status_rules.quiet_hours[0]
            .replacement_status
            .is_empty());
    }

    /// The new fields round-trip through serde.
    #[test]
    fn test_four_six_additions_round_trip() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{
                "teams": {"profanity_extra_words": ["spam", "spammer"]},
                "polling": {"pause_backoff_max_seconds": 120},
                "status_rules": {"quiet_hours": [{"enabled": true, "replacement_status": "Busy"}]}
            }"#,
        )
        .expect("must parse");
        assert_eq!(cfg.teams.profanity_extra_words.len(), 2);
        assert_eq!(cfg.polling.pause_backoff_max_seconds, 120);
        assert_eq!(cfg.status_rules.quiet_hours[0].replacement_status, "Busy");

        let json = serde_json::to_string(&cfg).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(
            back.teams.profanity_extra_words,
            cfg.teams.profanity_extra_words
        );
        assert_eq!(back.polling.pause_backoff_max_seconds, 120);
        assert_eq!(back.status_rules.quiet_hours[0].replacement_status, "Busy");
    }

    // ---------------------------------------------------------------
    // S4 (#672): rule schedules, `pause_polling`, manual-status texts.
    // ---------------------------------------------------------------

    /// Pre-4.7 files keep loading: every new field carries a serde default, and
    /// the defaults describe the 4.6 behaviour (a rule with no schedule applies
    /// every day, quiet hours do not pause polling, and the two status texts
    /// render what 4.6 posted).
    #[test]
    fn test_rule_schedule_additions_default_on_old_files() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{
                "teams": {},
                "status_rules": {
                    "quiet_hours": [{"enabled": true}],
                    "track_rules": [{"enabled": true, "artist_substring": "lofi"}]
                }
            }"#,
        )
        .expect("a pre-4.7 document must still parse");

        let rule = &cfg.status_rules.track_rules[0];
        assert!(rule.days.is_empty(), "no weekday set means every day");
        assert_eq!(rule.start_minutes, 0);
        assert_eq!(rule.end_minutes, TRACK_RULE_DAY_MINUTES);
        assert!(!cfg.status_rules.quiet_hours[0].pause_polling);
        assert_eq!(cfg.teams.paused_status_format, "Paused");
        assert_eq!(
            cfg.teams.stopped_status_format,
            "Nothing playing on Spotify"
        );

        // The serde defaults and the hand-written `Default` impls agree, so a
        // fixture built with `..Default::default()` describes the same rule a
        // config file missing the fields does.
        assert_eq!(rule.days, TrackRuleEntry::default().days);
        assert_eq!(rule.start_minutes, TrackRuleEntry::default().start_minutes);
        assert_eq!(rule.end_minutes, TrackRuleEntry::default().end_minutes);
        assert!(!QuietHoursEntry::default().pause_polling);
    }

    /// The new fields round-trip through serde — load, save, load again.
    #[test]
    fn test_rule_schedule_additions_round_trip() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{
                "teams": {
                    "paused_status_format": "Back in 5",
                    "stopped_status_format": "Idle"
                },
                "status_rules": {
                    "quiet_hours": [{"enabled": true, "pause_polling": true}],
                    "track_rules": [{
                        "enabled": true,
                        "days": [1, 3],
                        "start_minutes": 480,
                        "end_minutes": 1020
                    }]
                }
            }"#,
        )
        .expect("must parse");
        assert!(cfg.status_rules.quiet_hours[0].pause_polling);
        assert_eq!(cfg.status_rules.track_rules[0].days, vec![1, 3]);
        assert_eq!(cfg.status_rules.track_rules[0].start_minutes, 480);
        assert_eq!(cfg.status_rules.track_rules[0].end_minutes, 1020);

        let json = serde_json::to_string(&cfg).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.teams.paused_status_format, "Back in 5");
        assert_eq!(back.teams.stopped_status_format, "Idle");
        assert!(back.status_rules.quiet_hours[0].pause_polling);
        assert_eq!(back.status_rules.track_rules[0].days, vec![1, 3]);
        assert_eq!(back.status_rules.track_rules[0].start_minutes, 480);
        assert_eq!(back.status_rules.track_rules[0].end_minutes, 1020);
    }

    /// `clamp_rules` normalizes the schedule too: the window is bounded to
    /// `0..=1440` and the weekday set to the documented ISO range `1..=7`,
    /// deduplicated — on load and on every save.
    #[test]
    fn test_clamp_rules_normalizes_the_track_rule_window() {
        let mut rules = StatusRulesConfig {
            extra: Default::default(),
            quiet_hours: Vec::new(),
            track_rules: vec![
                TrackRuleEntry {
                    start_minutes: 5000,
                    end_minutes: 9000,
                    days: vec![0, 7, 1, 9, 1, 8],
                    ..TrackRuleEntry::default()
                },
                TrackRuleEntry {
                    start_minutes: 480,
                    end_minutes: 1020,
                    days: vec![6, 7],
                    ..TrackRuleEntry::default()
                },
            ],
        };
        clamp_rules(&mut rules);
        assert_eq!(
            rules.track_rules[0].start_minutes,
            TRACK_RULE_DAY_MINUTES - 1,
            "a start of 1440 is unreachable, so it clamps to the last minute"
        );
        assert_eq!(rules.track_rules[0].end_minutes, TRACK_RULE_DAY_MINUTES);
        assert_eq!(
            rules.track_rules[0].days,
            vec![1, 7],
            "weekday bytes outside 1..=7 are dropped and duplicates collapse"
        );
        assert_eq!(rules.track_rules[1].start_minutes, 480);
        assert_eq!(rules.track_rules[1].end_minutes, 1020);
        assert_eq!(rules.track_rules[1].days, vec![6, 7]);

        // What is persisted is the normalized rule, not the raw file.
        let json = serde_json::to_string_pretty(&clamped_config(&AppConfig {
            status_rules: rules,
            ..AppConfig::default()
        }))
        .expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.status_rules.track_rules[0].days, vec![1, 7]);
        assert_eq!(
            back.status_rules.track_rules[0].end_minutes,
            TRACK_RULE_DAY_MINUTES
        );
    }

    /// An oversized user lexicon is bounded, not rejected: the filter must
    /// never take an unbounded amount of work from a hand-edited file.
    #[test]
    fn test_clamp_teams_bounds_extra_words() {
        let mut teams = TeamsConfig {
            profanity_extra_words: (0..100)
                .map(|i| {
                    if i == 0 {
                        "x".repeat(64)
                    } else {
                        format!("w{i}")
                    }
                })
                .collect(),
            ..TeamsConfig::default()
        };
        clamp_teams(&mut teams);
        assert_eq!(teams.profanity_extra_words.len(), 64);
        assert_eq!(teams.profanity_extra_words[0].chars().count(), 32);

        // Multi-byte truncation must stay on a char boundary.
        let mut teams = TeamsConfig {
            profanity_extra_words: vec!["ü".repeat(40)],
            ..TeamsConfig::default()
        };
        clamp_teams(&mut teams);
        assert_eq!(teams.profanity_extra_words[0].chars().count(), 32);
    }

    /// S4 (issue #672): the two manual-status texts are status lines, so
    /// `clamp_teams` bounds them like a rule's replacement text — and an EMPTY
    /// text is left empty (the renderer reads that as "use the default").
    #[test]
    fn test_clamp_teams_bounds_the_manual_status_texts() {
        let mut teams = TeamsConfig {
            paused_status_format: "ü".repeat(200),
            stopped_status_format: "z".repeat(200),
            ..TeamsConfig::default()
        };
        clamp_teams(&mut teams);
        assert_eq!(
            teams.paused_status_format.chars().count(),
            MAX_RULE_STATUS_CHARS
        );
        assert_eq!(
            teams.stopped_status_format.chars().count(),
            MAX_RULE_STATUS_CHARS
        );
        // Char-boundary safe (the truncation above would have panicked
        // otherwise) and short texts are untouched.
        let mut short = TeamsConfig {
            paused_status_format: "Back in 5".to_string(),
            stopped_status_format: String::new(),
            ..TeamsConfig::default()
        };
        clamp_teams(&mut short);
        assert_eq!(short.paused_status_format, "Back in 5");
        assert!(short.stopped_status_format.is_empty());
    }

    /// `clamped_config` is applied on load AND on save, so the bounded
    /// lexicon survives both directions.
    #[test]
    fn test_clamped_config_bounds_lexicon_on_save() {
        let mut cfg = AppConfig::default();
        cfg.teams.profanity_extra_words = vec!["y".repeat(50)];
        let json = serde_json::to_string_pretty(&clamped_config(&cfg)).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.teams.profanity_extra_words[0].chars().count(), 32);
    }

    // ---------------------------------------------------------------
    // Presence findings #634 / #635 / #637: the rule presence model, the
    // manual-status policy flag and the out-of-office gate flag.
    // ---------------------------------------------------------------

    /// Every new field is additive: a 4.5 file loads with the documented
    /// defaults (presence-untouched rules, manual status respected, OOO gating
    /// off).
    #[test]
    fn test_presence_additions_default_on_old_files() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{"teams": {}, "status_rules": {"quiet_hours": [{"enabled": true}], "track_rules": [{"enabled": true}]}}"#,
        )
        .expect("a 4.5 document must still parse");
        assert!(cfg.teams.respect_manual_status);
        assert!(!cfg.teams.gate_when_out_of_office);
        assert!(cfg.status_rules.quiet_hours[0]
            .presence_availability
            .is_empty());
        assert!(cfg.status_rules.quiet_hours[0].presence_activity.is_empty());
        assert!(cfg.status_rules.track_rules[0]
            .presence_availability
            .is_empty());
        assert!(cfg.status_rules.track_rules[0].presence_activity.is_empty());
    }

    /// The canonical pair set is exactly the five documented combinations:
    /// case-insensitive on input, canonical on output, and nothing else.
    #[test]
    fn test_normalize_presence_pair_accepts_only_documented_combinations() {
        assert_eq!(
            normalize_presence_pair("Available", "Available"),
            Some(PresencePair {
                availability: "Available".to_string(),
                activity: "Available".to_string(),
            })
        );
        // Case + whitespace from a hand-edited config normalize to canonical.
        assert_eq!(
            normalize_presence_pair(" doNotDisturb ", "presenting")
                .expect("case-insensitive match")
                .activity,
            "Presenting"
        );
        assert_eq!(
            normalize_presence_pair("Busy", "InAConferenceCall")
                .expect("documented combination")
                .availability,
            "Busy"
        );
        // Unsupported / half-filled pairs are rejected outright — including
        // the two the docs say setPresence does not honour.
        assert_eq!(
            normalize_presence_pair("DoNotDisturb", "DoNotDisturb"),
            None
        );
        assert_eq!(normalize_presence_pair("OutOfOffice", "InAMeeting"), None);
        assert_eq!(normalize_presence_pair("Available", ""), None);
        assert_eq!(normalize_presence_pair("", ""), None);
        assert_eq!(normalize_presence_pair("totally-made-up", "x"), None);
    }

    /// `clamp_rules` is the IPC-boundary normalizer: an unsupported pair is
    /// cleared (not rejected), and an over-long replacement text is truncated
    /// on a char boundary.
    #[test]
    fn test_clamp_rules_normalizes_pairs_and_bounds_text() {
        let mut rules = StatusRulesConfig {
            extra: Default::default(),
            quiet_hours: vec![QuietHoursEntry {
                enabled: true,
                presence_availability: "donotdisturb".to_string(),
                presence_activity: "presenting".to_string(),
                replacement_status: "ü".repeat(200),
                ..QuietHoursEntry::default()
            }],
            track_rules: vec![TrackRuleEntry {
                enabled: true,
                presence_availability: "DoNotDisturb".to_string(),
                presence_activity: "DoNotDisturb".to_string(),
                replacement_status: "Focus".to_string(),
                ..TrackRuleEntry::default()
            }],
        };
        clamp_rules(&mut rules);
        assert_eq!(rules.quiet_hours[0].presence_availability, "DoNotDisturb");
        assert_eq!(rules.quiet_hours[0].presence_activity, "Presenting");
        assert_eq!(
            rules.quiet_hours[0].replacement_status.chars().count(),
            MAX_RULE_STATUS_CHARS
        );
        // The unsupported pair is cleared on both sides, so the rule means
        // "don't touch presence" instead of sending a pair Graph drops.
        assert!(rules.track_rules[0].presence_availability.is_empty());
        assert!(rules.track_rules[0].presence_activity.is_empty());
        // A supported, already-canonical pair and a short text are untouched.
        assert_eq!(rules.track_rules[0].replacement_status, "Focus");
    }

    /// The rule model is normalized on the SAVE path too (`clamped_config` is
    /// what `save_config` persists and what `AppState` stores).
    #[test]
    fn test_clamped_config_normalizes_rules_on_save() {
        let mut cfg = AppConfig::default();
        cfg.status_rules.track_rules.push(TrackRuleEntry {
            enabled: true,
            presence_availability: "Busy".to_string(),
            presence_activity: "InACall".to_string(),
            ..TrackRuleEntry::default()
        });
        cfg.status_rules.quiet_hours.push(QuietHoursEntry {
            enabled: true,
            presence_availability: "Away".to_string(),
            presence_activity: "Away".to_string(),
            replacement_status: "z".repeat(500),
            ..QuietHoursEntry::default()
        });

        let json = serde_json::to_string_pretty(&clamped_config(&cfg)).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(
            back.status_rules.track_rules[0].presence_activity,
            "InACall"
        );
        assert_eq!(back.status_rules.quiet_hours[0].presence_activity, "Away");
        assert_eq!(
            back.status_rules.quiet_hours[0]
                .replacement_status
                .chars()
                .count(),
            MAX_RULE_STATUS_CHARS
        );
    }

    /// Round trip for the new Teams + rule fields (finding #634/#635/#637).
    #[test]
    fn test_presence_additions_round_trip() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{
                "teams": {"respect_manual_status": false, "gate_when_out_of_office": true},
                "status_rules": {"track_rules": [{
                    "enabled": true,
                    "presence_availability": "Away",
                    "presence_activity": "Away"
                }]}
            }"#,
        )
        .expect("must parse");
        assert!(!cfg.teams.respect_manual_status);
        assert!(cfg.teams.gate_when_out_of_office);
        assert_eq!(
            cfg.status_rules.track_rules[0].presence_availability,
            "Away"
        );

        let back: AppConfig =
            serde_json::from_str(&serde_json::to_string(&cfg).expect("must serialize"))
                .expect("must re-parse");
        assert!(!back.teams.respect_manual_status);
        assert!(back.teams.gate_when_out_of_office);
        assert_eq!(back.status_rules.track_rules[0].presence_activity, "Away");
    }

    // ---------------------------------------------------------------
    // Issue #869: presence-profile overlay (clamp + effective_config + export).
    // ---------------------------------------------------------------

    /// `clamp_presence_profiles` drops duplicates + empty names and clears the
    /// active id when its name no longer survives — the spec's hand-edit
    /// safety net.
    #[test]
    fn test_clamp_presence_profiles_dedupes_and_clears_orphan_active_id() {
        let mut profiles = vec![
            PresenceProfile {
                name: "Focus".to_string(),
                ..PresenceProfile::default()
            },
            // Empty name — must be dropped silently with a warn.
            PresenceProfile {
                name: "   ".to_string(),
                ..PresenceProfile::default()
            },
            // Duplicate name — second occurrence dropped, first wins.
            PresenceProfile {
                name: "Focus".to_string(),
                ..PresenceProfile::default()
            },
            // Distinct survivor.
            PresenceProfile {
                name: "Party".to_string(),
                ..PresenceProfile::default()
            },
        ];
        let mut active = Some("Nonexistent".to_string());
        clamp_presence_profiles(&mut profiles, &mut active);
        assert_eq!(
            profiles.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            vec!["Focus", "Party"],
            "the empty and duplicate profiles must be dropped in favour of the surviving list"
        );
        assert!(
            active.is_none(),
            "an active id that no longer matches must be cleared"
        );
        // The matched id survives.
        let mut active = Some("Party".to_string());
        clamp_presence_profiles(&mut profiles, &mut active);
        assert_eq!(active.as_deref(), Some("Party"));
    }

    /// `clamp_presence_profiles` caps `idle_away_after_seconds` at 86 400 so a
    /// hand-edited config cannot put the idle gate in a permanently-firing
    /// state (mirrors the rule duration clamp).
    #[test]
    fn test_clamp_presence_profiles_caps_idle_away_seconds() {
        let mut profiles = vec![PresenceProfile {
            name: "Focus".to_string(),
            idle_away_after_seconds: Some(7 * 86_400),
            ..PresenceProfile::default()
        }];
        let mut active = None;
        clamp_presence_profiles(&mut profiles, &mut active);
        assert_eq!(
            profiles[0].idle_away_after_seconds,
            Some(86_400),
            "idle_away_after_seconds must be capped at 24h"
        );
    }

    /// Spec acceptance: switching a profile changes the effective status
    /// format and gate flags while the on-disk base values stay untouched.
    /// The overlay is read-only — `effective_config` MUST NOT mutate the
    /// input. The base `AppConfig` is a clone here so the assertion is
    /// literal, but the production code path receives a borrow of
    /// `state.config` and so the in-memory store cannot be touched
    /// either.
    #[test]
    fn test_effective_config_overlays_without_mutating_base() {
        let base = AppConfig {
            teams: TeamsConfig {
                status_format: "🎵 {artist} - {track} 🎧".to_string(),
                gate_when_out_of_office: false,
                gate_when_presenting: false,
                idle_away_after_seconds: 600,
                availability_sync: true,
                ..TeamsConfig::default()
            },
            active_profile: Some("Focus".to_string()),
            presence_profiles: vec![PresenceProfile {
                name: "Focus".to_string(),
                status_format: Some("🎧 In focus — {artist}".to_string()),
                gate_when_out_of_office: Some(true),
                gate_when_presenting: Some(true),
                idle_away_after_seconds: Some(60),
                availability_sync: Some(false),
                ..PresenceProfile::default()
            }],
            ..AppConfig::default()
        };
        let snapshot = base.clone();
        let effective = effective_config(&base);
        // Base stays untouched.
        assert_eq!(base.teams.status_format, snapshot.teams.status_format);
        assert_eq!(
            base.teams.gate_when_out_of_office,
            snapshot.teams.gate_when_out_of_office
        );
        assert_eq!(
            base.teams.gate_when_presenting,
            snapshot.teams.gate_when_presenting
        );
        assert_eq!(
            base.teams.idle_away_after_seconds,
            snapshot.teams.idle_away_after_seconds
        );
        assert_eq!(
            base.teams.availability_sync,
            snapshot.teams.availability_sync
        );
        // Overlay applied to the effective copy.
        assert_eq!(effective.teams.status_format, "🎧 In focus — {artist}");
        assert!(effective.teams.gate_when_out_of_office);
        assert!(effective.teams.gate_when_presenting);
        assert_eq!(effective.teams.idle_away_after_seconds, 60);
        assert!(!effective.teams.availability_sync);
    }

    /// `effective_config` is a no-op when no profile is active.
    #[test]
    fn test_effective_config_returns_base_unchanged_when_no_profile_is_active() {
        let base = AppConfig::default();
        let effective = effective_config(&base);
        assert_eq!(effective.teams.status_format, base.teams.status_format);
        assert_eq!(
            effective.teams.gate_when_out_of_office,
            base.teams.gate_when_out_of_office
        );
        // An orphan active id (not in the list) also disables the overlay.
        let mut with_orphan = base.clone();
        with_orphan.active_profile = Some("Nonexistent".to_string());
        let effective_orphan = effective_config(&with_orphan);
        assert_eq!(
            effective_orphan.teams.status_format,
            base.teams.status_format
        );
    }

    /// A profile's `track_rules` overlay REPLACES the base list (not
    /// appends); `Some(vec![])` is a legitimate "no rules while this
    /// profile is active" shape — a silent profile that only flips the
    /// status format.
    #[test]
    fn test_effective_config_replaces_track_rules_when_overlay_uses_empty_list() {
        let base_rules = vec![TrackRuleEntry {
            enabled: true,
            artist_substring: "Foo".to_string(),
            ..TrackRuleEntry::default()
        }];
        let mut base = AppConfig::default();
        base.status_rules.track_rules = base_rules.clone();
        base.active_profile = Some("Silent".to_string());
        base.presence_profiles = vec![PresenceProfile {
            name: "Silent".to_string(),
            track_rules: Some(Vec::new()),
            ..PresenceProfile::default()
        }];
        let effective = effective_config(&base);
        assert!(
            effective.status_rules.track_rules.is_empty(),
            "Some(vec![]) must REPLACE the base list, not append"
        );
        assert_eq!(base.status_rules.track_rules.len(), 1, "base unchanged");
    }

    /// Round-trip through export + import preserves both the profile list
    /// and the active id. The export code path is the same one
    /// `export_config` uses, so this pins the on-disk contract for the
    /// user's `--profile` switches.
    #[test]
    fn test_presence_profiles_and_active_id_round_trip_through_export_import() {
        let cfg = AppConfig {
            presence_profiles: vec![
                PresenceProfile {
                    name: "Focus".to_string(),
                    status_format: Some("🎧 {artist}".to_string()),
                    ..PresenceProfile::default()
                },
                PresenceProfile {
                    name: "Party".to_string(),
                    availability_sync: Some(true),
                    ..PresenceProfile::default()
                },
            ],
            active_profile: Some("Focus".to_string()),
            ..AppConfig::default()
        };

        let exported = serde_json::to_string(&cfg).expect("profile overlay must serialise");
        let back: AppConfig = serde_json::from_str(&exported).expect("profile overlay must parse");

        assert_eq!(back.presence_profiles.len(), 2);
        assert_eq!(back.presence_profiles[0].name, "Focus");
        assert_eq!(
            back.presence_profiles[0].status_format.as_deref(),
            Some("🎧 {artist}")
        );
        assert_eq!(back.presence_profiles[1].name, "Party");
        assert_eq!(back.presence_profiles[1].availability_sync, Some(true));
        assert_eq!(back.active_profile.as_deref(), Some("Focus"));
    }

    /// `prepare_import` survives a malformed / over-long profile name the
    /// same way `clamp_presence_profiles` does on load: it drops the
    /// empty entries and clears the active id, leaving the rest of the
    /// config (and the surviving profiles) intact.
    #[test]
    fn test_prepare_import_clears_orphan_active_profile_id() {
        let raw = r#"{
            "schema_version": 3,
            "presence_profiles": [
                {"name": "Focus"},
                {"name": ""}
            ],
            "active_profile": "Vanished",
            "autostart": false
        }"#;
        let prepared = prepare_import(raw).expect("must accept a valid shape");
        assert_eq!(prepared.config.active_profile, None);
        assert_eq!(prepared.config.presence_profiles.len(), 1);
        assert_eq!(prepared.config.presence_profiles[0].name, "Focus");
    }

    // ---------------------------------------------------------------
    // CfgDiag#2 (#537): the quarantine backup is name-only.
    // ---------------------------------------------------------------

    /// The diagnostics snapshot must be able to say "config.json.bak"
    /// without leaking the user's home directory (#409).
    #[test]
    fn test_quarantine_backup_name_is_a_bare_file_name() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-bakname-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        assert_eq!(
            quarantine_backup_name_for(&path),
            None,
            "no backup on disk means no name to report"
        );

        std::fs::write(quarantine_backup_path(&path), b"OLD").unwrap();
        let name = quarantine_backup_name_for(&path).expect("backup must be reported");
        assert_eq!(name, "config.json.bak");
        assert!(
            !name.contains('/') && !name.contains('\\'),
            "the reported name must carry no directory component, got {name}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---------------------------------------------------------------
    // IssueTriage#3 (#478): the config.rs log-tag guard.
    // ---------------------------------------------------------------

    /// Extract the argument region of the macro call whose opening `(` is
    /// at `open`, i.e. everything up to the matching `)`. String literals,
    /// char literals and `//` comments are skipped, so a `)` inside a format
    /// string cannot end the region early.
    fn macro_arg_region(src: &str, open: usize) -> Option<&str> {
        let bytes = src.as_bytes();
        if bytes.get(open) != Some(&b'(') {
            return None;
        }
        let mut depth: i32 = 0;
        let mut i = open;
        while i < bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&src[open + 1..i]);
                    }
                }
                b'"' => {
                    i += 1;
                    while i < bytes.len() {
                        match bytes[i] {
                            b'\\' => i += 1,
                            b'"' => break,
                            _ => {}
                        }
                        i += 1;
                    }
                }
                b'\'' => {
                    // A char literal (`'x'`, `'\n'`) — a lifetime never
                    // appears in a log macro argument.
                    let mut j = i + 1;
                    if bytes.get(j) == Some(&b'\\') {
                        j += 2;
                    } else {
                        j += 1;
                    }
                    if bytes.get(j) == Some(&b'\'') {
                        i = j;
                    }
                }
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// IssueTriage#2/#3 (#393/#478): every log macro in this file must carry
    /// the `[CFG]` module tag.
    ///
    /// Macro-aware on purpose: a per-line scan is red on green code, because
    /// ten of the call sites put the format string on the line AFTER the
    /// `log::warn!` opener. Whole argument regions are extracted instead.
    ///
    /// The needles are assembled with `concat!` so this test's own source
    /// never contains the literal it searches for — otherwise an
    /// `include_str!` scan would match the test itself and pass vacuously.
    #[test]
    fn test_config_log_tags_use_cfg_prefix() {
        let src = concat!(
            include_str!("schema.rs"),
            include_str!("clamp.rs"),
            include_str!("snooze.rs"),
            include_str!("patch.rs"),
            include_str!("migrate.rs"),
            include_str!("io.rs"),
            include_str!("transfer.rs")
        );
        let needles = [
            concat!("log::", "info!("),
            concat!("log::", "warn!("),
            concat!("log::", "error!("),
            concat!("log::", "debug!("),
            concat!("log::", "trace!("),
        ];
        let mut checked = 0usize;
        for needle in needles {
            let mut from = 0usize;
            while let Some(rel) = src[from..].find(needle) {
                let i = from + rel;
                let open = i + needle.len() - 1;
                let region = macro_arg_region(src, open)
                    .unwrap_or_else(|| panic!("unbalanced macro arguments at byte {i}"));
                assert!(
                    region.contains("[CFG]"),
                    "config.rs log at byte {i} lacks the [CFG] tag: {}",
                    region.replace('\n', " ")
                );
                checked += 1;
                from = open;
            }
        }
        assert!(
            checked >= 20,
            "the scan found only {checked} log macros — the needles are wrong"
        );
    }

    // ---------------------------------------------------------------
    // 4.7.0 (S5): log rotation defaults/clamps + config export/import
    // ---------------------------------------------------------------

    /// The rotation fields are additive and `#[serde(default)]`, so a
    /// `config.json` written before 4.7.0 has neither key and must load with
    /// the shipped defaults — a `0` here would build `KeepSome(0)` and make
    /// the plugin's archive pass underflow.
    #[test]
    fn pre_4_7_logging_config_gets_rotation_defaults() {
        let legacy: LoggingConfig = serde_json::from_str(r#"{"enabled":true,"log_level":"Debug"}"#)
            .expect("a logging section written before 4.7.0 must still deserialize");
        assert_eq!(legacy.max_file_size_mb, 10);
        assert_eq!(legacy.keep_files, 3);
        assert_eq!(LoggingConfig::default().max_file_size_mb, 10);
        assert_eq!(LoggingConfig::default().keep_files, 3);
    }

    /// Out-of-band rotation values — typed past the number inputs' `min`/
    /// `max`, or hand-edited — clamp to the band the UI offers.
    #[test]
    fn clamp_logging_bounds_rotation_fields() {
        let bounded = |max_file_size_mb: u64, keep_files: u32| {
            let mut logging = LoggingConfig {
                enabled: true,
                log_level: "Info".into(),
                max_file_size_mb,
                keep_files,
                ..LoggingConfig::default()
            };
            clamp_logging(&mut logging);
            (logging.max_file_size_mb, logging.keep_files)
        };
        assert_eq!(bounded(0, 0), (1, 1), "below the floor");
        assert_eq!(bounded(9000, 99), (500, 20), "above the ceiling");
        assert_eq!(bounded(25, 7), (25, 7), "inside the band is untouched");
    }

    /// `clamped_config` is what `save_config` writes, so an out-of-band
    /// rotation value must never reach disk.
    #[test]
    fn clamped_config_bounds_logging() {
        let mut config = AppConfig::default();
        config.logging.keep_files = 0;
        config.logging.max_file_size_mb = 10_000;
        let clamped = clamped_config(&config);
        assert_eq!(clamped.logging.keep_files, 1);
        assert_eq!(clamped.logging.max_file_size_mb, 500);
    }

    /// The export file name carries the app version and a sortable UTC stamp.
    #[test]
    fn export_file_name_is_versioned_and_timestamped() {
        let at = chrono::DateTime::parse_from_rfc3339("2026-09-17T04:05:06Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(
            export_file_name("4.7.0", at),
            "presencejam-config-4.7.0-20260917-040506.json"
        );
    }

    /// An export must never carry the plaintext secret, wherever a legacy or
    /// hand-edited file parked it — including the unknown-key retention map,
    /// which the typed schema knows nothing about.
    #[test]
    fn export_document_strips_client_secret() {
        let mut config = AppConfig::default();
        config.spotify.client_id = "abc".into();
        config
            .extra
            .insert("client_secret".into(), serde_json::json!("SEKRIT"));

        let json = export_document(&config).unwrap();

        assert!(!json.contains("SEKRIT"), "exported document: {json}");
        assert!(!json.contains("client_secret"), "exported document: {json}");
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["spotify"]["client_id"], "abc");
    }

    /// A plaintext `client_secret` is refused, and the refusal names the path
    /// it found: importing it "successfully" while silently dropping the
    /// credential would look like a completed sign-in.
    #[test]
    fn import_rejects_document_with_client_secret() {
        let err = prepare_import(r#"{"spotify":{"client_id":"a","client_secret":"SEKRIT"}}"#)
            .expect_err("a plaintext client_secret must be refused");
        assert!(
            err.contains("spotify.client_secret"),
            "refusal must name the offending path: {err}"
        );

        // The reject is on the key, not on the value: an explicit null is
        // still a client_secret field.
        assert!(prepare_import(r#"{"spotify":{"client_secret":null}}"#).is_err());

        // Nested inside the unknown-key retention map is the same answer.
        assert!(prepare_import(r#"{"schema_version":2,"client_secret":"SEKRIT"}"#).is_err());

        // A document without the key is not falsely rejected.
        assert!(prepare_import(r#"{"spotify":{"client_id":"a"}}"#).is_ok());
    }

    /// A document that is not a JSON object never reaches the schema parser.
    #[test]
    fn import_rejects_non_object_document() {
        assert!(prepare_import("[1, 2, 3]").is_err());
        assert!(prepare_import("not json at all").is_err());
    }

    /// Export → import keeps every value, including unknown top-level keys,
    /// and re-importing the written document is stable.
    #[test]
    fn export_import_round_trips_a_config() {
        let mut config = AppConfig::default();
        config.spotify.client_id = "abc123".into();
        config.teams.status_format = "🎧 {track}".into();
        config.polling.default_interval_seconds = 45;
        config.logging.max_file_size_mb = 25;
        config.logging.keep_files = 7;
        config
            .extra
            .insert("future_key".into(), serde_json::json!({"nested": [1, 2]}));

        let exported = export_document(&config).unwrap();
        let imported = prepare_import(&exported).unwrap();

        assert_eq!(imported.config.spotify.client_id, "abc123");
        assert_eq!(imported.config.teams.status_format, "🎧 {track}");
        assert_eq!(imported.config.polling.default_interval_seconds, 45);
        assert_eq!(imported.config.logging.max_file_size_mb, 25);
        assert_eq!(imported.config.logging.keep_files, 7);
        assert_eq!(
            imported.config.extra.get("future_key"),
            Some(&serde_json::json!({"nested": [1, 2]}))
        );

        let again = prepare_import(&imported.document).unwrap();
        assert_eq!(
            again.document, imported.document,
            "the written document must be a fixed point of prepare_import"
        );
    }

    /// The whole pipeline the two commands run, minus the file dialogs: export
    /// writes the document with the same crash-safe helper `save_config` uses,
    /// and importing that file lands the same settings with no secret in it.
    #[test]
    fn exported_file_imports_back_to_the_same_settings() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-export-import-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let export_path = dir.join(export_file_name(
            "4.7.0",
            chrono::DateTime::parse_from_rfc3339("2026-09-17T04:05:06Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        ));
        let live_path = dir.join("config.json");

        let mut config = AppConfig::default();
        config.spotify.client_id = "abc123".into();
        config.teams.status_format = "🎧 {track}".into();
        config.logging.max_file_size_mb = 25;
        config.logging.keep_files = 7;
        config
            .extra
            .insert("client_secret".into(), serde_json::json!("SEKRIT"));

        // export_config: serialize, then write through the shared helper.
        atomic_write_json(&export_path, &export_document(&config).unwrap()).unwrap();
        assert_eq!(
            export_path.file_name().unwrap().to_string_lossy(),
            "presencejam-config-4.7.0-20260917-040506.json"
        );
        let exported = std::fs::read_to_string(&export_path).unwrap();
        assert!(!exported.contains("SEKRIT"));

        // import_config: read the chosen file, validate, confirm, replace.
        let imported = import_config_document(&exported, &live_path, || true)
            .unwrap()
            .expect("the overwrite was confirmed");

        assert_eq!(imported.spotify.client_id, "abc123");
        assert_eq!(imported.teams.status_format, "🎧 {track}");
        assert_eq!(imported.logging.max_file_size_mb, 25);
        assert_eq!(imported.logging.keep_files, 7);
        assert_eq!(
            imported.extra.get("client_secret"),
            None,
            "the stripped key must not come back through an import"
        );
        assert!(!std::fs::read_to_string(&live_path)
            .unwrap()
            .contains("SEKRIT"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Declining the overwrite confirmation is a clean no-op: the live file is
    /// left byte-identical and no `.bak` appears. This is the cancel path of the
    /// import confirmation, and it is the reason the prompt is a parameter
    /// rather than a dialog buried inside the command — the decision is
    /// testable against real files without a desktop.
    #[test]
    fn declined_import_touches_nothing() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-import-decline-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let previous = r#"{"spotify":{"client_id":"LIVE"},"logging":{"keep_files":7}}"#;
        std::fs::write(&path, previous).unwrap();

        let outcome =
            import_config_document(r#"{"spotify":{"client_id":"NEW"}}"#, &path, || false).unwrap();

        assert!(
            outcome.is_none(),
            "declining must report 'nothing happened'"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            previous,
            "the live config must be byte-identical after a decline"
        );
        assert!(
            !dir.join("config.json.bak").exists(),
            "a decline must not quarantine the file it did not replace"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Importing over an existing config moves it to `<path>.bak` (the same
    /// backup the corrupt-file quarantine uses) and writes the imported
    /// document clamped, not raw.
    #[test]
    fn import_quarantines_the_outgoing_config() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-import-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let previous = r#"{"spotify":{"client_id":"OLD"}}"#;
        std::fs::write(&path, previous).unwrap();

        let imported = import_config_document(
            r#"{"spotify":{"client_id":"NEW"},"logging":{"keep_files":900}}"#,
            &path,
            || true,
        )
        .unwrap()
        .expect("the overwrite was confirmed");

        assert_eq!(
            std::fs::read_to_string(dir.join("config.json.bak")).unwrap(),
            previous,
            "the outgoing config must be preserved next to the new one"
        );
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["spotify"]["client_id"], "NEW");
        assert_eq!(
            written["logging"]["keep_files"], 20,
            "an imported out-of-range value must land clamped on disk"
        );
        assert_eq!(imported.logging.keep_files, 20);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A refused import must not touch the live config or leave a `.bak`
    /// behind — refusal happens before anything is moved.
    #[test]
    fn rejected_import_leaves_the_config_untouched() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-import-reject-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let previous = r#"{"spotify":{"client_id":"LIVE"}}"#;
        std::fs::write(&path, previous).unwrap();

        let asked = std::cell::Cell::new(false);
        assert!(import_config_document(
            r#"{"spotify":{"client_id":"NEW","client_secret":"SEKRIT"}}"#,
            &path,
            || {
                asked.set(true);
                true
            }
        )
        .is_err());
        assert!(
            !asked.get(),
            "a refused document must be rejected before the user is asked anything"
        );

        assert_eq!(std::fs::read_to_string(&path).unwrap(), previous);
        assert!(!dir.join("config.json.bak").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A fresh install has no `config.json`; an import must create one
    /// rather than fail on the missing backup step.
    #[test]
    fn import_into_a_missing_config_creates_it() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-import-fresh-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let asked = std::cell::Cell::new(false);
        import_config_document(r#"{"spotify":{"client_id":"FRESH"}}"#, &path, || {
            asked.set(true);
            true
        })
        .unwrap()
        .expect("a fresh install has nothing to decline");
        assert!(
            !asked.get(),
            "with no current file there is nothing to overwrite, so nothing to confirm"
        );

        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["spotify"]["client_id"], "FRESH");
        assert!(!dir.join("config.json.bak").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // S9 (issue #677): snooze deadlines.
    //
    // The feature is judged on one thing above all: "until tomorrow" must mean
    // the next LOCAL midnight, so a snooze never silently stretches or shrinks
    // by the machine's UTC offset — nor by a DST transition it spans.
    // -----------------------------------------------------------------------

    /// A fixed-offset zone at `hours` east of UTC. `FixedOffset` is the
    /// timezone-typed stand-in for "the user's machine is here", which is what
    /// makes these boundary tests independent of the test runner's TZ.
    fn zone(hours: i32) -> chrono::FixedOffset {
        chrono::FixedOffset::east_opt(hours * 3600).expect("valid offset")
    }

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, s)
            .unwrap()
            .and_utc()
    }

    /// "Until tomorrow" is the START of the next local calendar day, converted
    /// to UTC — never `now + 24 h`.
    ///
    /// Each row pins a boundary: a second before midnight (the old second must
    /// still land on tomorrow), exactly midnight (a full day, not zero) and a
    /// time where the UTC day and the local day differ, in both directions of
    /// offset.
    #[test]
    fn until_tomorrow_is_the_next_local_midnight() {
        // 23:59:59 in UTC+2 → one second later is tomorrow's midnight.
        let late = utc(2026, 1, 5, 21, 59, 59).with_timezone(&zone(2));
        let deadline = next_local_midnight_utc(late);
        assert_eq!(deadline, utc(2026, 1, 5, 22, 0, 0));
        assert_eq!(
            (deadline - late.with_timezone(&chrono::Utc)).num_seconds(),
            1
        );

        // Exactly local midnight: the deadline is the START of the FOLLOWING
        // day (a full day), never "today" (which would already be in the past).
        let midnight = utc(2026, 1, 5, 22, 0, 0).with_timezone(&zone(2));
        assert_eq!(next_local_midnight_utc(midnight), utc(2026, 1, 6, 22, 0, 0));

        // 00:05 in UTC−11: local date 2026-01-05, so the deadline is
        // 2026-01-06T00:00−11:00 = 11:00Z — ~23 h away in real time.
        let early = utc(2026, 1, 5, 11, 5, 0).with_timezone(&zone(-11));
        let far = next_local_midnight_utc(early);
        assert_eq!(far, utc(2026, 1, 6, 11, 0, 0));

        // The same instant seen from UTC+13 is 2026-01-06 00:05 locally, so the
        // deadline is the NEXT midnight — a timezone read of the same UTC value
        // must not reuse the previous day's boundary.
        let east = utc(2026, 1, 5, 11, 5, 0).with_timezone(&zone(13));
        assert_eq!(next_local_midnight_utc(east), utc(2026, 1, 6, 11, 0, 0));
    }

    /// An "until tomorrow" snooze is the local MIDNIGHT that follows `now`, in
    /// every offset — and nothing about `now`'s own wall clock leaks into it.
    ///
    /// Both halves are asserted per row because the second is what makes this
    /// test able to fail: the `(0, 24h]` duration bound alone does NOT kill a
    /// `now + 24 h` implementation (that returns exactly 24 h, which is inside
    /// the bound). The wall-clock assertion does — under `now + 24 h` the
    /// deadline's local time is `now`'s local time, e.g. 12:30, not 00:00.
    /// These rows sit at 23:59 / 00:01 / 12:30 precisely so that a same-wall-
    /// clock deadline is distinguishable from midnight in every row.
    ///
    /// Driven through `snooze_preset_deadline` (the table the tray actually
    /// calls) rather than `next_local_midnight_utc` directly, so the sweep also
    /// covers the DISPATCH arm: a mutant that replaced
    /// `SnoozePreset::UntilTomorrow` with `now + 24 h` used to survive this test
    /// because the test bypassed the arm it lived in.
    #[test]
    fn until_tomorrow_never_shifts_by_the_offset() {
        use chrono::Timelike;
        for hours in [-12, -11, -5, -1, 0, 1, 2, 5, 12, 13, 14] {
            let tz = zone(hours);
            for (h, mi) in [(0, 0), (0, 1), (12, 30), (23, 59)] {
                let now = utc(2026, 3, 4, h, mi, 0).with_timezone(&tz);
                let now_utc = now.with_timezone(&chrono::Utc);
                let deadline =
                    snooze_preset_deadline(SnoozePreset::UntilTomorrow, now_utc, now, None);
                let secs = (deadline - now_utc).num_seconds();
                assert!(
                    secs > 0 && secs <= 24 * 3600,
                    "offset {}h at {:02}:{:02} produced a {} s snooze",
                    hours,
                    h,
                    mi,
                    secs
                );

                // The boundary itself: tomorrow's local date, at exactly local
                // midnight. `FixedOffset` has no DST, so midnight always exists.
                let local = deadline.with_timezone(&tz);
                assert_eq!(
                    (local.hour(), local.minute(), local.second()),
                    (0, 0, 0),
                    "offset {}h at {:02}:{:02} did not land on local midnight \
                     (a `now + 24 h` deadline would keep the current wall clock)",
                    hours,
                    h,
                    mi
                );
                assert_eq!(
                    local.date_naive(),
                    now.date_naive() + chrono::Days::new(1),
                    "offset {}h at {:02}:{:02} did not land on tomorrow",
                    hours,
                    h,
                    mi
                );
            }
        }
    }

    /// A DST transition inside the window changes the real duration, never the
    /// wall-clock boundary (issue #677): `next_local_midnight_utc` reads the
    /// zone's rules rather than adding 24 h.
    #[test]
    fn until_tomorrow_keeps_the_calendar_boundary_across_dst() {
        // The deadline is the next LOCAL calendar day whatever the zone's
        // offset does in between. `chrono-tz` is not a dependency, so this runs
        // against the machine's own `Local` and asserts only what holds in
        // EVERY zone — including one whose spring-forward swallows local
        // midnight (America/Santiago, Asia/Beirut in some years), where the
        // correct answer is the first instant the new day exists (01:00), not
        // 00:00. Asserting `00:00:00` outright would therefore fail on those
        // machines while passing on a UTC CI runner.
        let now = chrono::Local::now();
        let deadline = next_local_midnight_utc(now);
        let local = deadline.with_timezone(&chrono::Local);
        use chrono::Timelike;

        assert!(
            deadline > now.with_timezone(&chrono::Utc),
            "a deadline in the past would make the snooze end instantly"
        );
        assert_eq!(
            local.date_naive(),
            now.date_naive() + chrono::Days::new(1),
            "the deadline must be TOMORROW's local date"
        );
        if local.hour() == 0 {
            assert_eq!(
                (local.minute(), local.second()),
                (0, 0),
                "a midnight that exists is exactly 00:00:00"
            );
        } else {
            // A gap swallowed midnight: the deadline is the first instant of
            // the new day that exists, which is what `resolve_local_forward`
            // walks to. It must still be well inside the new day and short of
            // its end.
            assert_eq!(
                (local.hour(), local.minute(), local.second()),
                (1, 0, 0),
                "a swallowed midnight resolves to the first existing instant"
            );
        }
        assert!(
            local.date_naive() == now.date_naive() + chrono::Days::new(1),
            "the boundary is the new day's start, never the current day's"
        );
    }

    /// A spring-forward that swallows local midnight (some zones transition at
    /// 00:00) leaves no wall clock to resolve. The walk must land on the first
    /// instant that exists rather than panicking or producing a past deadline.
    ///
    /// `FixedOffset` cannot express a gap, so the branch is pinned through a
    /// minimal stateful zone: offset +1 h before the gap, +2 h after it, with
    /// the local 00:00 of the target day inside the transition.
    #[test]
    fn resolve_local_forward_walks_out_of_a_dst_gap() {
        /// +1 h until 23:00 UTC on 2026-03-28, +2 h from then on (a zone whose
        /// spring-forward is at local midnight).
        #[derive(Debug, Clone, Copy)]
        struct GapZone;
        impl chrono::TimeZone for GapZone {
            type Offset = chrono::FixedOffset;
            fn from_offset(_: &Self::Offset) -> Self {
                GapZone
            }
            fn offset_from_local_date(
                &self,
                _: &chrono::NaiveDate,
            ) -> chrono::LocalResult<Self::Offset> {
                // Never used by the code under test; a single offset keeps the
                // provided `ymd` helpers well-defined.
                chrono::LocalResult::Single(zone(1))
            }
            fn offset_from_local_datetime(
                &self,
                local: &chrono::NaiveDateTime,
            ) -> chrono::LocalResult<Self::Offset> {
                // The gap: local wall clock 2026-03-29T00:00..00:59 never
                // happens.
                let gap_start = chrono::NaiveDate::from_ymd_opt(2026, 3, 29)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap();
                let gap_end = gap_start + chrono::TimeDelta::minutes(60);
                if *local >= gap_start && *local < gap_end {
                    return chrono::LocalResult::None;
                }
                let offset = if *local < gap_start { zone(1) } else { zone(2) };
                chrono::LocalResult::Single(offset)
            }
            fn offset_from_utc_date(&self, utc: &chrono::NaiveDate) -> Self::Offset {
                if *utc < chrono::NaiveDate::from_ymd_opt(2026, 3, 28).unwrap() {
                    zone(1)
                } else {
                    zone(2)
                }
            }
            fn offset_from_utc_datetime(&self, utc: &chrono::NaiveDateTime) -> Self::Offset {
                if utc.date() < chrono::NaiveDate::from_ymd_opt(2026, 3, 29).unwrap() {
                    zone(1)
                } else {
                    zone(2)
                }
            }
        }

        // 2026-03-28T23:30 local (+1 h) → tomorrow's midnight is inside the gap.
        let now = chrono::NaiveDate::from_ymd_opt(2026, 3, 28)
            .unwrap()
            .and_hms_opt(23, 30, 0)
            .unwrap();
        let deadline = next_local_midnight_utc(
            chrono::TimeZone::from_local_datetime(&GapZone, &now)
                .single()
                .expect("23:30 is not in the gap"),
        );
        // Local 00:00..00:59 on the 29th never happens, so the first instant of
        // the new day is local 01:00 at +2 h — which is 23:00Z on the 28th, i.e.
        // 30 minutes after this `now`. Not panicking, and not a deadline in the
        // past, is the property that matters: the snooze still ends just after
        // the calendar day turns.
        assert_eq!(deadline, utc(2026, 3, 28, 23, 0, 0));
        assert!(deadline > utc(2026, 3, 28, 22, 30, 0), "never in the past");
    }

    /// An ambiguous local midnight (a fall-back repeats it) resolves to the
    /// EARLIER instant: the conservative, shorter snooze.
    #[test]
    fn resolve_local_forward_prefers_the_earlier_ambiguous_instant() {
        #[derive(Debug, Clone, Copy)]
        struct FallBackAtMidnight;
        impl chrono::TimeZone for FallBackAtMidnight {
            type Offset = chrono::FixedOffset;
            fn from_offset(_: &Self::Offset) -> Self {
                FallBackAtMidnight
            }
            fn offset_from_local_date(
                &self,
                _: &chrono::NaiveDate,
            ) -> chrono::LocalResult<Self::Offset> {
                chrono::LocalResult::Single(zone(2))
            }
            fn offset_from_local_datetime(
                &self,
                local: &chrono::NaiveDateTime,
            ) -> chrono::LocalResult<Self::Offset> {
                let ambiguous = chrono::NaiveDate::from_ymd_opt(2026, 10, 25)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap();
                if *local == ambiguous {
                    return chrono::LocalResult::Ambiguous(zone(1), zone(2));
                }
                chrono::LocalResult::Single(zone(2))
            }
            fn offset_from_utc_date(&self, _: &chrono::NaiveDate) -> Self::Offset {
                zone(2)
            }
            fn offset_from_utc_datetime(&self, _: &chrono::NaiveDateTime) -> Self::Offset {
                zone(2)
            }
        }

        let resolved = resolve_local_forward(
            &FallBackAtMidnight,
            chrono::NaiveDate::from_ymd_opt(2026, 10, 25)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        );
        assert_eq!(
            resolved,
            utc(2026, 10, 24, 23, 0, 0),
            "the earlier of the two midnights is the shorter, safer snooze"
        );
    }

    /// The preset table: the two duration presets are pure instant offsets (no
    /// timezone can move them), "until tomorrow" is the local boundary, and
    /// "until this meeting ends" (issue #867) reads the supplied
    /// `next_meeting_end`, falling back to tomorrow when no meeting is
    /// active.
    #[test]
    fn snooze_presets_map_to_deadlines() {
        let now_utc = utc(2026, 6, 1, 9, 0, 0);
        let now_local = now_utc.with_timezone(&zone(2));
        assert_eq!(
            snooze_preset_deadline(SnoozePreset::ThirtyMinutes, now_utc, now_local, None),
            utc(2026, 6, 1, 9, 30, 0)
        );
        assert_eq!(
            snooze_preset_deadline(SnoozePreset::OneHour, now_utc, now_local, None),
            utc(2026, 6, 1, 10, 0, 0)
        );
        assert_eq!(
            snooze_preset_deadline(SnoozePreset::UntilTomorrow, now_utc, now_local, None),
            utc(2026, 6, 1, 22, 0, 0),
            "11:00 local → the midnight that follows it, in UTC"
        );
        // Issue #867: an active meeting end is the deadline.
        let meeting_end = utc(2026, 6, 1, 10, 30, 0);
        assert_eq!(
            snooze_preset_deadline(
                SnoozePreset::UntilNextMeetingEnds,
                now_utc,
                now_local,
                Some(meeting_end)
            ),
            meeting_end,
            "an active meeting end is the deadline"
        );
        // Issue #867: no active meeting falls back to tomorrow.
        assert_eq!(
            snooze_preset_deadline(SnoozePreset::UntilNextMeetingEnds, now_utc, now_local, None),
            utc(2026, 6, 1, 22, 0, 0),
            "no meeting in progress → fall back to the local-midnight deadline"
        );
    }

    /// The stored spelling round-trips through the parser the clamp uses, and
    /// the persisted value is UTC regardless of what the machine reads.
    #[test]
    fn snooze_deadline_round_trips_and_survives_an_offset_spelling() {
        let deadline = utc(2026, 9, 17, 13, 45, 0);
        let stored = snooze_store_form(deadline);
        assert_eq!(stored, "2026-09-17T13:45:00Z");
        assert_eq!(
            snooze_deadline(&stored, deadline - chrono::TimeDelta::seconds(1)),
            Some(deadline)
        );

        // A value re-serialized by another tool as +02:00 denotes the same
        // instant and must not be discarded.
        assert_eq!(
            snooze_deadline(
                "2026-09-17T15:45:00+02:00",
                deadline - chrono::TimeDelta::seconds(1)
            ),
            Some(deadline),
            "an offset-shifted spelling of the same instant is still the deadline"
        );
        // Whitespace from a hand edit is tolerated.
        assert_eq!(
            snooze_deadline(
                "  2026-09-17T13:45:00Z ",
                deadline - chrono::TimeDelta::seconds(1)
            ),
            Some(deadline)
        );
    }

    /// The boundaries of "active": the instant of the deadline itself is
    /// already over, and anything unparsable is never a snooze.
    #[test]
    fn snooze_deadline_is_exclusive_at_its_own_instant() {
        let deadline = utc(2026, 9, 17, 13, 45, 0);
        let stored = snooze_store_form(deadline);
        assert!(snooze_deadline(&stored, deadline - chrono::TimeDelta::seconds(1)).is_some());
        assert!(
            snooze_deadline(&stored, deadline).is_none(),
            "at the deadline the snooze is over — otherwise it would never end"
        );
        assert!(snooze_deadline(&stored, deadline + chrono::TimeDelta::seconds(1)).is_none());
        assert!(snooze_deadline("", deadline).is_none());
        assert!(snooze_deadline("not a timestamp", deadline).is_none());
        assert!(snooze_deadline("2026-09-17", deadline).is_none());
    }

    /// The startup clamp: a deadline that passed while the app was closed is
    /// cleared (and reported), a future one and an absent one are untouched.
    #[test]
    fn clamp_snooze_clears_only_expired_deadlines() {
        let now = utc(2026, 9, 17, 13, 45, 0);
        let with = |value: Option<&str>| AppConfig {
            snooze_until: value.map(str::to_string),
            ..AppConfig::default()
        };

        let mut future = with(Some("2026-09-17T14:00:00Z"));
        assert!(!clamp_snooze(&mut future, now));
        assert_eq!(future.snooze_until.as_deref(), Some("2026-09-17T14:00:00Z"));

        let mut expired = with(Some("2026-09-17T13:00:00Z"));
        assert!(
            clamp_snooze(&mut expired, now),
            "an expired snooze is dropped"
        );
        assert_eq!(expired.snooze_until, None);

        let mut garbage = with(Some("yesterday-ish"));
        assert!(
            clamp_snooze(&mut garbage, now),
            "an unparsable value is dropped"
        );
        assert_eq!(garbage.snooze_until, None);

        let mut absent = with(None);
        assert!(!clamp_snooze(&mut absent, now));

        // Idempotent: a second pass finds nothing to clear, which is what keeps
        // the one-line log from repeating on every load and save.
        assert!(!clamp_snooze(&mut expired, now));

        // The reader's twin reports the same states WITHOUT mutating, which is
        // what lets `load_config` log the expiry while leaving the stored value
        // visible to the writers that can correct `config.json`.
        let reported = with(Some("2026-09-17T13:00:00Z"));
        assert!(snooze_expired_deadline(&reported, now));
        assert_eq!(
            reported.snooze_until.as_deref(),
            Some("2026-09-17T13:00:00Z"),
            "the reader must not touch the field"
        );
        assert!(snooze_expired_deadline(&with(Some("yesterday-ish")), now));
        assert!(!snooze_expired_deadline(
            &with(Some("2026-09-17T14:00:00Z")),
            now
        ));
        assert!(
            !snooze_expired_deadline(&with(None), now),
            "an absent field is 'not snoozed', not something to clean or report"
        );
        // …and the writer's verdict agrees with the reader's on every row, so a
        // value the load path reports is always one a save will remove.
        for value in [
            Some("2026-09-17T13:00:00Z"),
            Some("yesterday-ish"),
            Some("2026-09-17T14:00:00Z"),
            None,
        ] {
            let mut cfg = with(value);
            let cleared = clamp_snooze(&mut cfg, now);
            assert_eq!(
                cleared,
                snooze_expired_deadline(&with(value), now),
                "clamp and report must agree for {:?}",
                value
            );
        }
    }

    /// The countdown rounds UP and never reads zero while the snooze is live —
    /// the difference between "30 min left" the moment it is set and "29".
    #[test]
    fn snooze_minutes_left_rounds_up() {
        assert_eq!(snooze_minutes_left(30 * 60), 30);
        assert_eq!(snooze_minutes_left(30 * 60 - 1), 30);
        assert_eq!(snooze_minutes_left(29 * 60), 29);
        assert_eq!(snooze_minutes_left(60), 1);
        assert_eq!(snooze_minutes_left(1), 1);
        // Never zero, never negative — and bounded for an absurd stored value.
        assert_eq!(snooze_minutes_left(0), 1);
        assert_eq!(snooze_minutes_left(-5), 1);
        assert_eq!(snooze_minutes_left(i64::MAX), 24 * 60);
    }

    /// `snooze_status` is the single read the tray and the driver share: it
    /// reports the remaining seconds and refuses everything else.
    #[test]
    fn snooze_status_reports_only_a_live_deadline() {
        let now = utc(2026, 9, 17, 13, 45, 0);
        let cfg = |value: Option<&str>| AppConfig {
            snooze_until: value.map(str::to_string),
            ..AppConfig::default()
        };
        let status = snooze_status(&cfg(Some("2026-09-17T14:15:00Z")), now).expect("live snooze");
        assert_eq!(status.remaining_seconds, 30 * 60);
        assert_eq!(snooze_store_form(status.deadline), "2026-09-17T14:15:00Z");

        assert!(snooze_status(&cfg(Some("2026-09-17T13:45:00Z")), now).is_none());
        assert!(snooze_status(&cfg(Some("nope")), now).is_none());
        assert!(snooze_status(&cfg(None), now).is_none());
    }

    /// A pre-4.7 config file has no `snooze_until` key and must keep loading —
    /// the field is additive with a serde default, and the default is "not
    /// snoozed".
    #[test]
    fn snooze_until_is_additive_for_pre_4_7_configs() {
        let cfg: AppConfig =
            serde_json::from_str(r#"{"spotify":{"client_id":"X"},"schema_version":2}"#).unwrap();
        assert_eq!(cfg.snooze_until, None);
        assert!(snooze_status(&cfg, chrono::Utc::now()).is_none());
        // …and it serializes back out, so a snooze survives a restart.
        let json = serde_json::to_value(AppConfig {
            snooze_until: Some("2026-09-17T14:15:00Z".to_string()),
            ..AppConfig::default()
        })
        .unwrap();
        assert_eq!(json["snooze_until"], "2026-09-17T14:15:00Z");
    }

    /// Issue #676: every config file written before 4.7.0 lacks the
    /// `shortcuts` section entirely, and it must load with the documented
    /// defaults rather than with no global shortcuts at all.
    #[test]
    fn pre_4_7_config_json_still_loads_with_default_shortcuts() {
        let legacy: AppConfig = serde_json::from_str(
            r#"{"spotify":{"client_id":"abc"},"teams":{"status_format":"x"},"autostart":true}"#,
        )
        .expect("a config.json written before #676 must still deserialize");
        assert_eq!(legacy.shortcuts, ShortcutsConfig::default());
        assert_eq!(
            legacy.shortcuts.toggle_playback.as_deref(),
            Some(DEFAULT_TOGGLE_PLAYBACK_SHORTCUT)
        );
        assert_eq!(
            legacy.shortcuts.toggle_sync.as_deref(),
            Some(DEFAULT_TOGGLE_SYNC_SHORTCUT)
        );
        // The rest of the file is untouched by the additive section.
        assert_eq!(legacy.spotify.client_id, "abc");
        assert!(legacy.autostart);
    }

    /// An *absent* key takes the default, an explicit `null` is the user's
    /// deliberate unbinding. Collapsing the two would make a user who cleared
    /// one row find it re-bound at the next launch.
    #[test]
    fn absent_shortcut_takes_the_default_and_null_unbinds() {
        let absent: ShortcutsConfig = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(absent, ShortcutsConfig::default());

        let nulled: ShortcutsConfig = serde_json::from_str(r#"{"toggle_playback":null}"#).unwrap();
        assert_eq!(nulled.toggle_playback, None);
        assert_eq!(
            nulled.toggle_sync.as_deref(),
            Some(DEFAULT_TOGGLE_SYNC_SHORTCUT),
            "clearing one row must not clear the other"
        );
    }

    /// The bindings round-trip through JSON unchanged, blank string included
    /// (the registration planner, not the loader, decides that blank means
    /// "unbound" — a loader that silently rewrote it would make the Settings
    /// field lie about what is stored).
    #[test]
    fn shortcut_bindings_round_trip_through_json() {
        let cfg = ShortcutsConfig {
            extra: Default::default(),
            toggle_playback: Some("CmdOrCtrl+Shift+P".to_string()),
            toggle_sync: Some("  ".to_string()),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(serde_json::from_str::<ShortcutsConfig>(&json).unwrap(), cfg);
    }

    /// Issue #678: a `config.json` written before 4.7.0 has no `updates`
    /// key, and must keep loading on the stable channel (additive serde
    /// default — the schema version does not move for it).
    #[test]
    fn test_updates_channel_defaults_to_stable_for_pre_4_7_configs() {
        let cfg: AppConfig = serde_json::from_str("{}").expect("empty object must parse");
        assert_eq!(cfg.updates.channel, UpdateChannel::Stable);
        assert_eq!(AppConfig::default().updates.channel, UpdateChannel::Stable);
    }

    /// The persisted spelling is what the Settings picker round-trips across
    /// a relaunch, so both directions must hold the lowercase wire form.
    #[test]
    fn test_update_channel_wire_spelling() {
        for (channel, wire) in [
            (UpdateChannel::Stable, "\"stable\""),
            (UpdateChannel::Beta, "\"beta\""),
        ] {
            assert_eq!(serde_json::to_string(&channel).unwrap(), wire);
            assert_eq!(
                serde_json::from_str::<UpdateChannel>(wire).unwrap(),
                channel
            );
        }
        let stored: AppConfig =
            serde_json::from_str(r#"{"updates": {"channel": "beta"}}"#).expect("must parse");
        assert_eq!(stored.updates.channel, UpdateChannel::Beta);
    }

    // The lenient-read tests below assert a `log::warn!`, and no logger is
    // installed in a unit-test process by default. Capturing is process-wide,
    // so they serialise on the same lock the quarantine tests use.
    static LOG_LINES: parking_lot::Mutex<Vec<String>> = parking_lot::Mutex::new(Vec::new());
    static LOGGER: std::sync::Once = std::sync::Once::new();

    struct CapturingLogger;

    impl log::Log for CapturingLogger {
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            LOG_LINES.lock().push(record.args().to_string());
        }
        fn flush(&self) {}
    }

    /// The channel a value reads as, and the warning to log for it.
    ///
    /// Issue #678: an unknown spelling reads as `stable` and names the
    /// offending value, instead of failing the whole document.
    #[test]
    fn test_unknown_update_channel_names_the_offending_value() {
        let (channel, warning) = lenient_update_channel(&serde_json::json!("nightly"));
        assert_eq!(channel, UpdateChannel::Stable);
        let warning = warning.expect("an unrecognised value must warn");
        assert!(
            warning.contains("nightly"),
            "the warning must name the offending value: {warning}"
        );
        assert!(
            !warning.contains("[CFG]"),
            "the [CFG] tag belongs at the log site, not in the message: {warning}"
        );

        // The known spellings take no fallback path and never warn.
        for (raw, expected) in [
            ("stable", UpdateChannel::Stable),
            ("beta", UpdateChannel::Beta),
        ] {
            let (channel, warning) = lenient_update_channel(&serde_json::json!(raw));
            assert_eq!(channel, expected);
            assert!(warning.is_none(), "{raw} must not warn");
        }
    }

    /// Issue #678: the point of the lenient read — a document carrying a
    /// channel spelling this binary does not know still LOADS, with the rest of
    /// the config intact, and says so in the log.
    ///
    /// Fails before the lenient read: serde rejects the whole document, which
    /// `load_config` answers by quarantining `config.json` to `config.json.bak`
    /// and booting on defaults — so `from_str` returned `Err`, every other
    /// setting was lost, and nothing was logged.
    #[test]
    fn test_unknown_update_channel_keeps_the_rest_of_the_config_and_warns() {
        let _guard = LOG_CAPTURE_TEST_LOCK.lock();
        LOGGER.call_once(|| {
            // Best-effort: another test may have installed a logger first.
            let _ = log::set_boxed_logger(Box::new(CapturingLogger));
            log::set_max_level(log::LevelFilter::Warn);
        });
        LOG_LINES.lock().clear();

        let cfg: AppConfig = serde_json::from_str(
            r#"{"autostart": true, "updates": {"channel": "nightly"},
                "teams": {"status_format": "🎧 {track}"}}"#,
        )
        .expect("an unrecognised channel value must not reject the config document");

        assert_eq!(cfg.updates.channel, UpdateChannel::Stable);
        assert!(
            cfg.autostart,
            "the other settings must survive the fallback"
        );
        assert_eq!(cfg.teams.status_format, "🎧 {track}");

        let logged = LOG_LINES.lock().clone();
        let warned = logged
            .iter()
            .find(|line| line.contains("nightly"))
            .unwrap_or_else(|| panic!("the fallback must be logged: {logged:?}"));
        assert!(
            warned.contains("[CFG]"),
            "the logged line carries the module tag: {warned}"
        );
    }

    // -----------------------------------------------------------------
    // Issue #926: the config document loads field by field, so one bad
    // section cannot take the rest of the document down with it.
    // -----------------------------------------------------------------

    /// Create a unique temp config directory holding a `config.json` with
    /// `contents`; returns `(dir, path)`. The caller removes the dir.
    fn temp_config_file(tag: &str, contents: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-{tag}-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    /// The list `config_from_sections` partitions on has to match the schema:
    /// a key missing from it would be read BOTH as its typed field and into
    /// `extra`, so a save would write the file's own value back twice.
    #[test]
    fn typed_config_keys_match_the_serialized_schema() {
        let serialized = serde_json::to_value(AppConfig::default()).expect("must serialize");
        let mut keys: Vec<&str> = serialized
            .as_object()
            .expect("the schema serializes to an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut known = TYPED_CONFIG_KEYS.to_vec();
        known.sort_unstable();
        assert_eq!(
            keys, known,
            "TYPED_CONFIG_KEYS must list exactly the top-level keys AppConfig owns"
        );
    }

    /// Issue #926: sections are independent. One wrong-typed field used to fail
    /// the single `serde_json::from_str::<AppConfig>` call, which `load_config`
    /// answered by quarantining the file and booting on defaults — so a
    /// hand-edited `teams` value cost the user their client id, quiet hours,
    /// polling tuning and everything else too.
    #[test]
    fn test_one_bad_section_keeps_every_other_section() {
        // Only the shared log sink needs serialising (issue #758 slice 2); the
        // quarantine flag is this test's own `caches`.
        let _guard = LOG_CAPTURE_TEST_LOCK.lock();
        LOGGER.call_once(|| {
            // Best-effort: another test may have installed a logger first.
            let _ = log::set_boxed_logger(Box::new(CapturingLogger));
            log::set_max_level(log::LevelFilter::Warn);
        });
        LOG_LINES.lock().clear();
        let caches = crate::state::AppCaches::new();

        let (dir, path) = temp_config_file(
            "sections",
            r#"{
                "spotify": {"client_id": "abc"},
                "teams": {"status_format": 5},
                "polling": {"default_interval_seconds": 42, "max_interval_seconds": 55},
                "logging": {"log_level": "Debug", "max_file_size_mb": 7},
                "updates": {"channel": "beta"},
                "autostart": true,
                "notifications": {"track_change": false},
                "status_rules": {"quiet_hours": [{"enabled": true, "start_minutes": 1320, "end_minutes": 420, "days": [2]}]},
                "future_top_level": {"kept": true}
            }"#,
        );

        let cfg =
            load_config_from(&caches, &path).expect("a partially invalid document must still load");

        assert_eq!(
            cfg.teams.status_format,
            default_status_format(),
            "the invalid section takes its default"
        );
        assert_eq!(cfg.spotify.client_id, "abc", "a sibling section is kept");
        assert_eq!(cfg.polling.default_interval_seconds, 42);
        assert_eq!(cfg.polling.max_interval_seconds, 55);
        assert_eq!(cfg.logging.log_level, "Debug");
        assert_eq!(cfg.logging.max_file_size_mb, 7);
        assert_eq!(cfg.updates.channel, UpdateChannel::Beta);
        assert!(cfg.autostart);
        assert!(!cfg.notifications.track_change);
        assert_eq!(cfg.status_rules.quiet_hours.len(), 1);
        assert_eq!(cfg.status_rules.quiet_hours[0].start_minutes, 1320);
        assert_eq!(
            cfg.extra.get("future_top_level"),
            Some(&serde_json::json!({"kept": true})),
            "unknown top-level keys still land in `extra`"
        );

        assert!(
            !quarantine_backup_path(&path).exists(),
            "one bad section must not quarantine the whole file"
        );
        assert!(!config_was_quarantined(&caches));
        let logged = LOG_LINES.lock().clone();
        assert!(
            logged
                .iter()
                .any(|line| line.contains("[CFG]") && line.contains("teams")),
            "the fallback must name the field it replaced: {logged:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #926: `client_id` was the only persisted field without a serde
    /// default, so `{"spotify": {}}` — and equally `"client_id": null` — failed
    /// the whole document. Neither quarantines the file now: the first loads as
    /// the section's defaults, the second as those defaults plus a warning.
    #[test]
    fn test_empty_or_null_client_id_loads_without_quarantining() {
        let caches = crate::state::AppCaches::new();
        for contents in [
            r#"{"spotify": {}, "autostart": true}"#,
            r#"{"spotify": {"client_id": null}, "autostart": true}"#,
        ] {
            let (dir, path) = temp_config_file("client-id", contents);
            let cfg = load_config_from(&caches, &path).expect("must load");
            assert_eq!(cfg.spotify.client_id, "", "{contents}");
            assert_eq!(
                cfg.spotify.redirect_uri,
                default_redirect_uri(),
                "{contents}"
            );
            assert!(
                cfg.autostart,
                "the rest of the document is kept: {contents}"
            );
            assert!(!quarantine_backup_path(&path).exists(), "{contents}");
            assert!(!config_was_quarantined(&caches), "{contents}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// Issue #926: the field-by-field loader is for documents that ARE an
    /// object. Anything else is not a config — a bare array/string/null root,
    /// or text that is not JSON — and is still quarantined to `config.json.bak`
    /// with the app booting on defaults.
    #[test]
    fn test_non_object_root_is_still_quarantined() {
        for contents in ["[1, 2, 3]", "\"spotify\"", "null", "{ NOT VALID JSON !!!"] {
            // A fresh `AppCaches` per iteration, so each case starts from an
            // un-raised flag without any save/restore dance (issue #758).
            let caches = crate::state::AppCaches::new();
            let (dir, path) = temp_config_file("non-object", contents);
            let cfg = load_config_from(&caches, &path).expect("quarantine still yields defaults");
            assert_eq!(cfg.schema_version, default_schema_version());
            assert!(cfg.spotify.client_id.is_empty());
            assert!(
                quarantine_backup_path(&path).exists(),
                "{contents} must be quarantined"
            );
            assert!(!path.exists(), "{contents}");
            assert!(config_was_quarantined(&caches), "{contents}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    // -----------------------------------------------------------------
    // Issue #802: the 0600 hardening is best-effort.
    // -----------------------------------------------------------------

    /// The body of the function `signature` starts — from its opening `{` to
    /// the matching `}` — found by brace counting, so a guard over it survives
    /// reordering / splitting / renaming of the code around it. Panics when the
    /// function itself is gone, which is a failure of the guard's premise.
    fn fn_body<'a>(src: &'a str, signature: &str) -> &'a str {
        let sig_idx = src
            .find(signature)
            .unwrap_or_else(|| panic!("{signature} must exist"));
        let brace_open_rel = src[sig_idx..]
            .find('{')
            .unwrap_or_else(|| panic!("{signature} must have an opening brace"));
        let body_start = sig_idx + brace_open_rel;
        let mut depth: u32 = 0;
        let mut i = body_start;
        let body_end = loop {
            match src.as_bytes()[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break i;
                    }
                }
                _ => {}
            }
            i += 1;
            if i >= src.len() {
                panic!("{signature} has unbalanced braces");
            }
        };
        &src[body_start + 1..body_end]
    }

    /// Issue #802: tightening the mode is a courtesy, never a precondition for
    /// reading a config the user's account can open. Both errors on that path
    /// were `?`-propagated, so an unreadable or unwritable mode failed the whole
    /// load — startup logged "no config found", `AppState.config` stayed `None`,
    /// the stores fell back to built-in defaults, and the next whole-document
    /// save persisted those defaults over the real file.
    ///
    /// A `?` cannot appear in a function that has no `Result` to return, so the
    /// guard is exact rather than stylistic. It cannot be replaced by a real
    /// failing `chmod`: that needs a file the test does not own (or an immutable
    /// / read-only mount), and `chmod` is gated on ownership of the file, not on
    /// write access to its directory — so the "0o555 temp dir" shape suggested
    /// in the issue does not deny it, for an unprivileged user or for root.
    #[test]
    fn test_config_mode_tightening_cannot_abort_the_load() {
        let body = fn_body(
            concat!(
                include_str!("schema.rs"),
                include_str!("clamp.rs"),
                include_str!("snooze.rs"),
                include_str!("patch.rs"),
                include_str!("migrate.rs"),
                include_str!("io.rs"),
                include_str!("transfer.rs")
            ),
            "fn tighten_config_permissions(",
        );
        assert!(
            !body.contains('?'),
            "tighten_config_permissions must not propagate an error: a mode that \
             cannot be read or set must not stop load_config from reading a file it \
             can open (issue #802)"
        );
    }

    /// Issue #802, the happy path: a loose `config.json` is still tightened to
    /// 0600 while it loads, and its stored values come back.
    #[cfg(unix)]
    #[test]
    fn test_loose_config_is_still_tightened_on_load() {
        let (dir, path) = temp_config_file("tighten", r#"{"autostart": true}"#);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let caches = crate::state::AppCaches::new();
        let cfg = load_config_from(&caches, &path).expect("a readable config must load");
        assert!(cfg.autostart);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600,
            "the #135 tightening must still run"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------
    // Issue #821: quiet hours normalize their schedule like track rules.
    // -----------------------------------------------------------------

    /// Issue #821: a quiet-hours window normalizes its schedule the way the
    /// track-rule half already did. `days: [0, 9, 2, 2]` used to load unchanged
    /// and match NO weekday at all, so the window — its `pause_polling` arm
    /// included — silently never fired.
    #[test]
    fn test_clamp_rules_normalizes_the_quiet_hours_window() {
        let mut rules = StatusRulesConfig {
            extra: Default::default(),
            quiet_hours: vec![
                QuietHoursEntry {
                    enabled: true,
                    start_minutes: 60000,
                    end_minutes: 60000,
                    days: vec![0, 9, 2, 2],
                    ..QuietHoursEntry::default()
                },
                QuietHoursEntry {
                    enabled: true,
                    start_minutes: 1320,
                    end_minutes: 420,
                    days: vec![7, 3, 3],
                    ..QuietHoursEntry::default()
                },
            ],
            track_rules: Vec::new(),
        };
        clamp_rules(&mut rules);

        assert_eq!(
            rules.quiet_hours[0].days,
            vec![2],
            "out-of-range weekdays are dropped, then sorted and deduplicated"
        );
        assert_eq!(rules.quiet_hours[0].start_minutes, 1439);
        assert_eq!(rules.quiet_hours[0].end_minutes, 1440);
        // A window already inside the day is untouched: the wrap-around pair,
        // both minutes and the out-of-order duplicates survive as documented.
        assert_eq!(rules.quiet_hours[1].days, vec![3, 7]);
        assert_eq!(rules.quiet_hours[1].start_minutes, 1320);
        assert_eq!(rules.quiet_hours[1].end_minutes, 420);
    }

    /// Issue #821, through the loader: the same normalization runs on load, so a
    /// hand-edited `days` cannot reach the evaluator as "no weekday matches" —
    /// an emptied list means EVERY day, which is why dropping the out-of-range
    /// values can only widen the window.
    #[test]
    fn test_quiet_hours_window_is_normalized_on_load() {
        let (dir, path) = temp_config_file(
            "quiet-window",
            r#"{"status_rules": {"quiet_hours": [
                {"enabled": true, "days": [0, 9, 2, 2], "start_minutes": 60000, "end_minutes": 60000},
                {"enabled": true, "days": [0]}
            ]}}"#,
        );
        let caches = crate::state::AppCaches::new();
        let cfg = load_config_from(&caches, &path).expect("must load");
        let first = &cfg.status_rules.quiet_hours[0];
        assert_eq!(first.days, vec![2]);
        assert_eq!(first.start_minutes, 1439);
        assert_eq!(first.end_minutes, 1440);
        assert!(
            cfg.status_rules.quiet_hours[1].days.is_empty(),
            "an all-out-of-range list becomes empty, which means every day"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------
    // Issue #938: a section's unknown keys survive, and `schema_version`
    // is a floor that never comes back down.
    // -----------------------------------------------------------------

    /// Issue #938: a key a NEWER build nested inside a section used to be
    /// dropped by the next save from this build — `AppConfig::extra` retained
    /// only the top level. The nested case now rides through the same load→save
    /// path the app uses.
    #[test]
    fn test_nested_unknown_keys_survive_load_then_save() {
        let (dir, path) = temp_config_file(
            "nested-extra",
            r#"{"autostart": true,
                "teams": {"status_format": "🎧 {track}", "future_flag": true,
                          "future_block": {"a": [1, 2]}},
                "spotify": {"client_id": "abc", "future_spotify": "x"},
                "logging": {"future_logging": 1},
                "polling": {"future_polling": 2},
                "updates": {"future_updates": "u"},
                "notifications": {"future_notifications": false},
                "status_rules": {"quiet_hours": [], "track_rules": [],
                                 "future_rule_flag": "r"},
                "shortcuts": {"future_shortcut": "s"}}"#,
        );
        let caches = crate::state::AppCaches::new();
        let cfg = load_config_from(&caches, &path).expect("must load");
        assert_eq!(
            cfg.teams.extra.get("future_flag"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            cfg.spotify.extra.get("future_spotify"),
            Some(&serde_json::json!("x"))
        );
        assert_eq!(
            cfg.logging.extra.get("future_logging"),
            Some(&serde_json::json!(1))
        );
        assert_eq!(
            cfg.polling.extra.get("future_polling"),
            Some(&serde_json::json!(2))
        );
        assert_eq!(
            cfg.updates.extra.get("future_updates"),
            Some(&serde_json::json!("u"))
        );
        assert_eq!(
            cfg.notifications.extra.get("future_notifications"),
            Some(&serde_json::json!(false))
        );
        assert_eq!(
            cfg.status_rules.extra.get("future_rule_flag"),
            Some(&serde_json::json!("r"))
        );
        assert_eq!(
            cfg.shortcuts.extra.get("future_shortcut"),
            Some(&serde_json::json!("s"))
        );

        save_config_to(&path, &cfg).expect("save must succeed");

        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            written["teams"]["future_flag"], true,
            "a nested unknown key must survive a load-then-save round trip"
        );
        assert_eq!(
            written["teams"]["future_block"],
            serde_json::json!({"a": [1, 2]})
        );
        assert_eq!(written["spotify"]["future_spotify"], "x");
        assert_eq!(written["logging"]["future_logging"], 1);
        assert_eq!(written["polling"]["future_polling"], 2);
        assert_eq!(written["updates"]["future_updates"], "u");
        assert_eq!(written["notifications"]["future_notifications"], false);
        assert_eq!(
            written["status_rules"]["future_rule_flag"], "r",
            "a nested key in the rules section survives too"
        );
        assert_eq!(written["shortcuts"]["future_shortcut"], "s");
        assert_eq!(written["teams"]["status_format"], "🎧 {track}");
        assert_eq!(written["autostart"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #938: `save_config` refuses to rewrite a document a newer binary
    /// wrote. The marker is the only record of which migrations have run, and
    /// this build cannot see the keys those migrations read — so the file is
    /// left byte-identical and the caller gets the error to surface.
    #[test]
    fn test_save_refuses_a_newer_document_and_never_lowers_the_marker() {
        let contents = r#"{"schema_version": 99, "future_key": {"kept": true}, "autostart": true}"#;
        let (dir, path) = temp_config_file("newer-than-us", contents);
        let caches = crate::state::AppCaches::new();
        let cfg = load_config_from(&caches, &path).expect("a newer document must still load");
        assert_eq!(
            cfg.schema_version, 99,
            "a newer document keeps its own version instead of being relabelled"
        );
        assert!(
            cfg.extra.contains_key("future_key"),
            "its unknown keys are read"
        );

        let err =
            save_config_to(&path, &cfg).expect_err("saving over a newer document must be refused");
        assert!(err.contains("newer"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            contents,
            "the newer document must be left byte-identical"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #938: stamping is a FLOOR, not an assignment — a document already
    /// at a higher version must not be relabelled downward, while a stale
    /// client payload still cannot lower the running build's own marker.
    #[test]
    fn test_stamp_schema_version_never_lowers_a_newer_document() {
        let mut newer = AppConfig {
            schema_version: SCHEMA_VERSION + 1,
            ..AppConfig::default()
        };
        stamp_schema_version(&mut newer);
        assert_eq!(newer.schema_version, SCHEMA_VERSION + 1);

        let mut stale = AppConfig {
            schema_version: 1,
            ..AppConfig::default()
        };
        stamp_schema_version(&mut stale);
        assert_eq!(stale.schema_version, SCHEMA_VERSION);
    }

    /// Issue #938: `teams.preferred_presence` is a config section in its own
    /// right, and it was the one nested object still missing the unknown-key
    /// retention map every one of its siblings carries. A key a newer build
    /// nested there was dropped by the next save from this build.
    #[test]
    fn test_preferred_presence_nested_keys_survive_load_then_save() {
        let (dir, path) = temp_config_file(
            "preferred-presence-extra",
            r#"{"teams": {"preferred_presence": {"enabled": true,
                     "availability": "Busy", "activity": "InACall",
                     "future_pp_flag": true, "future_pp_block": {"a": [1, 2]}}}}"#,
        );
        let caches = crate::state::AppCaches::new();
        let cfg = load_config_from(&caches, &path).expect("must load");
        assert_eq!(
            cfg.teams.preferred_presence.extra.get("future_pp_flag"),
            Some(&serde_json::json!(true)),
            "a nested unknown key must be retained, not dropped"
        );

        save_config_to(&path, &cfg).expect("save must succeed");
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            written["teams"]["preferred_presence"]["future_pp_flag"], true,
            "it must survive the load-then-save round trip"
        );
        assert_eq!(
            written["teams"]["preferred_presence"]["future_pp_block"],
            serde_json::json!({"a": [1, 2]})
        );
        assert_eq!(written["teams"]["preferred_presence"]["enabled"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #938/#916: the `client_secret` strip must cover EVERY unknown-key
    /// bucket. `playback`, each `presence_profiles` entry and
    /// `teams.preferred_presence` were all missed, so a secret in one of them
    /// was deserialized, handed to the webview by the `load_config` command,
    /// and re-serialized on the next save — the IPC crossing SECURITY.md
    /// promises cannot happen, reachable through a section that only gained
    /// its retention map later.
    #[test]
    fn test_client_secret_is_stripped_from_every_retained_bucket() {
        let (dir, path) = temp_config_file(
            "extras-secret-coverage",
            r#"{"autostart": true,
                "playback": {"source": "auto", "client_secret": "PLAYBACK-SENTINEL",
                             "kept_playback": 1},
                "presence_profiles": [{"name": "Focus", "client_secret": "PROFILE-SENTINEL",
                                       "kept_profile": 2}],
                "teams": {"preferred_presence": {"client_secret": "PP-SENTINEL",
                                                  "kept_pp": 3}}}"#,
        );
        let caches = crate::state::AppCaches::new();
        let cfg = load_config_from(&caches, &path).expect("must load");

        let serialized = serde_json::to_string(&cfg).expect("must serialize");
        assert!(
            !serialized.contains("SENTINEL"),
            "no client_secret may reach the webview: {serialized}"
        );
        // A sibling key of a stripped secret is kept — the strip is targeted.
        assert_eq!(
            cfg.playback.extra.get("kept_playback"),
            Some(&serde_json::json!(1))
        );
        assert_eq!(
            cfg.presence_profiles[0].extra.get("kept_profile"),
            Some(&serde_json::json!(2))
        );
        assert_eq!(
            cfg.teams.preferred_presence.extra.get("kept_pp"),
            Some(&serde_json::json!(3))
        );

        save_config_to(&path, &cfg).expect("save must succeed");
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            !written.contains("SENTINEL"),
            "the next save must not write a credential back: {written}"
        );
        assert!(written.contains("kept_playback"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #939: a failed import must leave the previous `config.json` in
    /// place, and it must still load.
    ///
    /// `import_config_document` used to rename the live file to `.bak` FIRST
    /// and write the imported document afterwards, so a write that failed —
    /// disk full, quota, permission denied — or a process death between the
    /// two left the user with NO live config: the next launch logged "Config
    /// file not found", booted on defaults, and their settings existed only in
    /// a `.bak` that nothing in the app restores.
    ///
    /// The failure is induced by occupying the staged sidecar path with a
    /// directory, so the staging write cannot succeed. That is deterministic
    /// and — unlike a `0o555` directory, which root writes through anyway — it
    /// behaves identically whatever uid the suite runs under.
    #[test]
    fn test_failed_import_keeps_the_previous_config_loadable() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-import-survives-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let previous = r#"{"spotify":{"client_id":"LIVE"},"logging":{"keep_files":7}}"#;
        std::fs::write(&path, previous).unwrap();
        // A non-empty directory at the sidecar path: `remove_file` refuses it
        // with something other than NotFound, which is exactly the "a staged
        // import cannot be written" case.
        let blocked = staged_import_path(&path);
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("occupant"), b"x").unwrap();

        let outcome = import_config_document(r#"{"spotify":{"client_id":"NEW"}}"#, &path, || true);

        let err = outcome.expect_err("an unwritable sidecar must fail the import");
        assert!(!err.is_empty());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            previous,
            "the previous config must survive a failed import byte-for-byte"
        );
        assert!(
            !quarantine_backup_path(&path).exists(),
            "the live file must never have been moved aside"
        );
        // And it is still a usable config: the next launch must not boot on
        // defaults.
        let caches = crate::state::AppCaches::new();
        let loaded = load_config_from(&caches, &path).expect("the surviving config must load");
        assert_eq!(loaded.spotify.client_id, "LIVE");
        assert_eq!(loaded.logging.keep_files, 7);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #939: the imported bytes are staged BEFORE the live file is moved
    /// aside. A `.bak` can therefore only ever appear next to a config that was
    /// successfully replaced, never as the user's sole surviving copy, and the
    /// sidecar is consumed on success rather than left beside the file.
    #[test]
    fn test_import_stages_before_moving_the_live_file() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-import-stage-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"spotify":{"client_id":"OLD"}}"#).unwrap();

        let imported = import_config_document(r#"{"spotify":{"client_id":"NEW"}}"#, &path, || true)
            .unwrap()
            .expect("the overwrite was confirmed");

        assert_eq!(imported.spotify.client_id, "NEW");
        assert_eq!(
            std::fs::read_to_string(quarantine_backup_path(&path)).unwrap(),
            r#"{"spotify":{"client_id":"OLD"}}"#,
            "the outgoing document is still preserved"
        );
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["spotify"]["client_id"], "NEW");
        assert!(
            !staged_import_path(&path).exists(),
            "a successful import must consume its sidecar, leaving no stray"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------
    // Issue #916: a `client_secret` in an unknown-key bucket never crosses
    // the IPC boundary and leaves the file on the next save.
    // -----------------------------------------------------------------

    /// Issue #916: a `client_secret` an outside writer left at the top level —
    /// or nested inside a section, which #938's retention map would otherwise
    /// carry — must not appear anywhere in the `AppConfig` a load returns (that
    /// document goes straight to the webview), and must be gone from the file
    /// after the next save.
    #[test]
    fn test_client_secret_keys_never_reach_ipc_and_leave_the_file() {
        let (dir, path) = temp_config_file(
            "top-level-secret",
            r#"{"autostart": true,
                "client_secret": "TOP-LEVEL-SENTINEL",
                "future": {"client_secret": "NESTED-SENTINEL", "kept": 1},
                "spotify": {"client_id": "abc", "client_secret": "SPOTIFY-SENTINEL"},
                "status_rules": {"quiet_hours": [], "track_rules": [],
                                 "client_secret": "RULES-SENTINEL"},
                "shortcuts": {"client_secret": "SHORTCUT-SENTINEL"}}"#,
        );
        let caches = crate::state::AppCaches::new();
        let cfg = load_config_from(&caches, &path).expect("must load");

        let serialized = serde_json::to_string(&cfg).expect("must serialize");
        assert!(
            !serialized.contains("SENTINEL"),
            "no client_secret may reach the webview: {serialized}"
        );
        assert_eq!(
            cfg.extra.get("future"),
            Some(&serde_json::json!({"kept": 1})),
            "a sibling key of a stripped secret is kept"
        );
        assert_eq!(cfg.spotify.client_id, "abc");
        assert!(cfg.autostart);

        save_config_to(&path, &cfg).expect("save must succeed");
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            !written.contains("SENTINEL"),
            "the next save must not write a credential back: {written}"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&written).unwrap()["autostart"],
            true
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #916, write side: a payload POSTed back by a script in the webview
    /// can carry whatever it likes in an unknown-key bucket, so the write path
    /// strips as well — nothing a caller hands `save_config` can put a
    /// credential into `config.json`.
    #[test]
    fn test_save_strips_a_client_secret_a_payload_carries() {
        let (dir, path) = temp_config_file("save-secret", r#"{"autostart": true}"#);
        let mut cfg = AppConfig {
            autostart: true,
            ..AppConfig::default()
        };
        cfg.extra.insert(
            "client_secret".to_string(),
            serde_json::json!("PAYLOAD-SENTINEL"),
        );
        cfg.teams.extra.insert(
            "client_secret".to_string(),
            serde_json::json!("SECTION-SENTINEL"),
        );
        cfg.status_rules.extra.insert(
            "client_secret".to_string(),
            serde_json::json!("RULES-SENTINEL"),
        );
        cfg.shortcuts.extra.insert(
            "client_secret".to_string(),
            serde_json::json!("SHORTCUT-SENTINEL"),
        );

        save_config_to(&path, &cfg).expect("save must succeed");

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            !written.contains("SENTINEL"),
            "a save must never write a caller-supplied credential: {written}"
        );
        assert!(written.contains("\"autostart\": true"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #916: the migration reads the documented pre-2.6.0 path when the
    /// document has one, and any other `client_secret` path otherwise. Reading
    /// only `spotify.client_secret` meant a credential at another path was
    /// neither migrated nor stripped, just dropped.
    #[test]
    fn test_legacy_client_secret_reads_every_path() {
        assert_eq!(
            legacy_client_secret(&serde_json::json!({"spotify": {"client_secret": "A"}}))
                .as_deref(),
            Some("A")
        );
        assert_eq!(
            legacy_client_secret(&serde_json::json!({"future": {"client_secret": "B"}})).as_deref(),
            Some("B")
        );
        assert_eq!(
            legacy_client_secret(&serde_json::json!({"items": [{"client_secret": "C"}]}))
                .as_deref(),
            Some("C")
        );
        // The documented path wins when several are present (`serde_json`'s map
        // order is what makes this a preference rather than an accident).
        assert_eq!(
            legacy_client_secret(&serde_json::json!({
                "spotify": {"client_secret": "A"}, "other": {"client_secret": "B"}
            }))
            .as_deref(),
            Some("A")
        );
        // Not a credential: an empty string, a null, a non-string, or no key.
        for absent in [
            serde_json::json!({"spotify": {"client_secret": ""}}),
            serde_json::json!({"client_secret": null, "spotify": {}}),
            serde_json::json!({"future": {"client_secret": {"nested": 1}}}),
            serde_json::json!({"spotify": {}}),
        ] {
            assert_eq!(legacy_client_secret(&absent), None, "{absent}");
        }
    }

    /// Issue #916 (the traversal both readers share): every `client_secret` key
    /// is found, at any depth, including one inside an array element — the
    /// import refusal and the migration must never disagree about where a
    /// credential can hide.
    #[test]
    fn test_client_secret_paths_walks_objects_and_arrays() {
        let paths = |value: &serde_json::Value| {
            let mut sorted = client_secret_paths(value);
            sorted.sort();
            sorted
        };
        assert_eq!(
            paths(&serde_json::json!({"a": {"client_secret": 1}, "client_secret": 2})),
            vec!["a.client_secret".to_string(), "client_secret".to_string()]
        );
        assert_eq!(
            paths(&serde_json::json!({"list": [{"client_secret": 1}]})),
            vec!["list[0].client_secret".to_string()]
        );
        assert!(paths(&serde_json::json!({"spotify": {"client_id": "abc"}})).is_empty());
    }

    // -----------------------------------------------------------------
    // Issue #975: the snooze deadline is runtime state, never a setting.
    // -----------------------------------------------------------------

    /// Issue #975: an export is advertised as a shareable settings copy, so it
    /// must not carry "pause sync until then" from the exporting machine — the
    /// recipient reads a live deadline as "sync is broken".
    #[test]
    fn test_export_never_carries_the_snooze_deadline() {
        let cfg = AppConfig {
            snooze_until: Some("2999-01-01T00:00:00Z".to_string()),
            ..AppConfig::default()
        };
        let document = export_document(&cfg).expect("must export");
        assert!(
            !document.contains("snooze_until"),
            "an export must not carry the deadline: {document}"
        );
        assert!(!document.contains("2999"), "{document}");
        // Still a settings copy.
        let value: serde_json::Value = serde_json::from_str(&document).unwrap();
        assert!(value.get("teams").is_some(), "{document}");
        assert_eq!(value["schema_version"], SCHEMA_VERSION);
    }

    /// Issue #975, the other direction: a document that still carries a FUTURE
    /// deadline (an older export, a hand-edit) must leave the importing machine
    /// syncing, and the rewritten document must not carry the key either.
    #[test]
    fn test_import_clears_a_future_snooze_deadline() {
        let prepared = prepare_import(
            r#"{"snooze_until": "2999-01-01T00:00:00Z", "autostart": true,
                "teams": {"status_format": "🎧 {track}"}}"#,
        )
        .expect("must import");
        assert_eq!(
            prepared.config.snooze_until, None,
            "an imported document must never start life paused"
        );
        assert!(
            !prepared.document.contains("snooze_until"),
            "{}",
            prepared.document
        );
        let written: serde_json::Value = serde_json::from_str(&prepared.document).unwrap();
        assert_eq!(written["autostart"], true);
        assert_eq!(written["teams"]["status_format"], "🎧 {track}");
    }

    // -----------------------------------------------------------------
    // Issue #803: a conflicting legacy plaintext is kept in a sidecar the
    // app never rewrites.
    // -----------------------------------------------------------------

    /// Issue #803: the conflict arm leaves the plaintext in `config.json`
    /// because the keychain already holds a different secret — but
    /// `save_config` serialises `AppConfig`, which has no such field, so the
    /// next unrelated save deleted the only remaining copy while the app kept
    /// authenticating with the stale keychain value. The sidecar is what makes
    /// the documented "the plaintext is not deleted" promise outlive that save,
    /// and the value itself never reaches the log.
    #[test]
    fn test_legacy_secret_sidecar_survives_a_later_save() {
        let _guard = LOG_CAPTURE_TEST_LOCK.lock();
        LOGGER.call_once(|| {
            let _ = log::set_boxed_logger(Box::new(CapturingLogger));
            log::set_max_level(log::LevelFilter::Warn);
        });
        LOG_LINES.lock().clear();
        let caches = crate::state::AppCaches::new();

        let (dir, path) = temp_config_file(
            "legacy-sidecar",
            r#"{"autostart": true,
                "spotify": {"client_id": "abc", "client_secret": "LEGACY-SENTINEL"}}"#,
        );
        // The conflict arm itself needs a real keychain read and the real config
        // path, so the sidecar writer is exercised directly here.
        let name = write_legacy_secret_sidecar(&path, "LEGACY-SENTINEL").expect("sidecar");
        assert_eq!(name, LEGACY_SECRET_SIDECAR_NAME);
        let sidecar = path.with_file_name(LEGACY_SECRET_SIDECAR_NAME);
        let sidecar_json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
        assert_eq!(sidecar_json["client_secret"], "LEGACY-SENTINEL");

        #[cfg(unix)]
        assert_eq!(
            std::fs::metadata(&sidecar).unwrap().permissions().mode() & 0o777,
            0o600,
            "the sidecar must be user-only from the moment it exists"
        );

        // Any later save replaces `config.json` wholesale — the #803 premise —
        // and the sidecar is what keeps the user's copy.
        let mut cfg = load_config_from(&caches, &path).expect("must load");
        cfg.autostart = false;
        save_config_to(&path, &cfg).expect("save must succeed");
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("LEGACY-SENTINEL"),
            "a later save drops the plaintext from config.json"
        );
        assert!(
            std::fs::read_to_string(&sidecar)
                .unwrap()
                .contains("LEGACY-SENTINEL"),
            "…and the sidecar still holds the user's copy"
        );

        let logged = LOG_LINES.lock().clone();
        assert!(
            logged
                .iter()
                .any(|line| line.contains(LEGACY_SECRET_SIDECAR_NAME)),
            "the user must be told which file holds the copy: {logged:?}"
        );
        assert!(
            !logged.iter().any(|line| line.contains("LEGACY-SENTINEL")),
            "a client secret must never reach the log: {logged:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------
    // Issue #943: a monotonic revision, and a stale write is refused.
    // -----------------------------------------------------------------

    /// Issue #943: Settings renders in both the main window and the detached
    /// pane, each holding its own copy loaded once — so the second window's save
    /// used to revert the first window's change silently. A payload behind the
    /// document on disk is refused instead, and the file is left untouched.
    #[test]
    fn test_save_rejects_a_stale_revision_and_leaves_the_file_alone() {
        let contents = r#"{"autostart": true, "revision": 5}"#;
        let (dir, path) = temp_config_file("stale-revision", contents);
        let caches = crate::state::AppCaches::new();
        let mut stale = load_config_from(&caches, &path).expect("must load");
        assert_eq!(stale.revision, 5);

        stale.revision = 4; // the other window saved in between
        stale.autostart = false;
        let err = save_config_to(&path, &stale).expect_err("a stale revision must be refused");
        assert!(
            err.starts_with(STALE_REVISION_MARKER),
            "the store has to be able to tell this failure apart: {err}"
        );
        assert!(err.contains("another window"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            contents,
            "the newer document must be left byte-identical"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #943: the revision rises on every accepted save, the document a
    /// save returns is the one on disk, and that returned document is accepted
    /// by its own next save — the write-back the commands layer needs so a
    /// window is never stale against itself.
    #[test]
    fn test_save_advances_the_revision_monotonically() {
        let (dir, path) = temp_config_file("revision", r#"{"autostart": true}"#);
        let caches = crate::state::AppCaches::new();
        let cfg = load_config_from(&caches, &path).expect("must load");
        assert_eq!(cfg.revision, 0, "a pre-#943 file reads as revision 0");

        let first = save_config_to(&path, &cfg).expect("first save");
        assert_eq!(first.revision, 1, "a save advances the revision");
        let on_disk: AppConfig =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            on_disk.revision, first.revision,
            "the returned document is the one that was written"
        );

        let second = save_config_to(&path, &first).expect("the returned document saves again");
        assert_eq!(second.revision, 2);

        let err = save_config_to(&path, &first)
            .expect_err("a copy from before the second save is now behind the file");
        assert!(err.starts_with(STALE_REVISION_MARKER), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #943, the compatibility half: `revision: 0` means "this payload
    /// carries no revision" — a frontend that does not send the field yet, or a
    /// fresh install. It is stamped above the stored revision rather than
    /// refused, because refusing it would make every save from such a client
    /// fail as soon as the first one succeeded.
    #[test]
    fn test_a_payload_without_a_revision_is_stamped_not_refused() {
        let (dir, path) = temp_config_file("no-revision", r#"{"autostart": true, "revision": 7}"#);
        let caches = crate::state::AppCaches::new();
        let mut payload = load_config_from(&caches, &path).expect("must load");
        payload.revision = 0;
        payload.autostart = false;

        let persisted =
            save_config_to(&path, &payload).expect("a revision-less payload must still save");
        assert_eq!(persisted.revision, 8, "stamped above the stored revision");
        assert!(!persisted.autostart, "the write itself still happened");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The frontend switches on both literals, so their spelling is part of the
    /// wire contract — the same guard `SPOTIFY_SECRET_CONFLICT_EVENT` has.
    #[test]
    fn test_config_changed_event_name_contract() {
        assert_eq!(CONFIG_CHANGED_EVENT, "config-changed");
        assert_eq!(STALE_REVISION_MARKER, "stale-config-revision");
    }
}
