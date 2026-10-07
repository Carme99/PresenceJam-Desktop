//! Teams status-text shaping (issue #754).
//!
//! Split from `polling::poll_once`: the music-emoji prefix, the paused and
//! no-track placeholders, the change-key fingerprint, the config-flip rewrite
//! probe and the playing-track status expiry.

use chrono::Utc;

use crate::config::AppConfig;
use crate::profanity;

/// The music-note prefix on both placeholder clears. Kept out of the config
/// fields so their defaults stay plain text (`"Paused"`), which is what the
/// fixed schema in the release contract specifies.
/// The standard music-emoji prefix (`🎵`) the polling path prefixes every
/// status with. Exported `pub(crate)` so the manual-status clear path
/// (issue #870) can mirror it without copying the literal — a future
/// "make the prefix configurable" change should land in one place.
pub(crate) const MUSIC_EMOJI: &str = "\u{1F3B5}";
pub(crate) const DEFAULT_STOPPED_STATUS_FORMAT: &str = crate::i18n::EN.status_stopped_default;

pub(crate) fn config_locale(config: &Option<std::sync::Arc<AppConfig>>) -> Option<&str> {
    config.as_ref().and_then(|cfg| cfg.locale.as_deref())
}

/// Resolve only an empty field or a byte-equal shipped English default. Any
/// other value is user-authored and is returned without normalization.
pub(crate) fn localized_status_fallback<'a>(
    configured: Option<&'a str>,
    shipped_english: &'static str,
    localized: &'static str,
) -> &'a str {
    match configured {
        Some(value) if !value.is_empty() && value != shipped_english => value,
        _ => localized,
    }
}

/// The configured paused text, localized at post time when the stored value
/// is empty or still the shipped English default.
pub(crate) fn paused_status_text(config: &Option<std::sync::Arc<AppConfig>>) -> &str {
    crate::commands::sync::paused_status_text(config)
}

/// [`paused_status_text`]'s no-track sibling, with the same untouched-default
/// and user-authored-value contract.
pub(crate) fn stopped_status_text(config: &Option<std::sync::Arc<AppConfig>>) -> &str {
    let strings = crate::i18n::strings_for(crate::i18n::resolve_tag(config_locale(config)));
    localized_status_fallback(
        config
            .as_ref()
            .map(|cfg| cfg.teams.stopped_status_format.as_str()),
        DEFAULT_STOPPED_STATUS_FORMAT,
        strings.status_stopped_default,
    )
}

/// S4 (issue #672): the paused-clear placeholder. The emoji is ours; the text is
/// `teams.paused_status_format` (default "Paused"), so the default renders
/// byte-identically to the pre-4.7 literal `"🎵 Paused"`.
pub(crate) fn paused_status_placeholder(config: &Option<std::sync::Arc<AppConfig>>) -> String {
    crate::commands::sync::paused_status_placeholder(config)
}

/// S4 (issue #672): the no-track clear's placeholder — the same emoji contract
/// as [`paused_status_placeholder`], with `teams.stopped_status_format`
/// (default `"Nothing playing on Spotify"`). A matching rule's replacement text
/// still takes precedence over it.
pub(crate) fn stopped_status_placeholder(config: &Option<std::sync::Arc<AppConfig>>) -> String {
    format!("{MUSIC_EMOJI} {}", stopped_status_text(config))
}

/// Issue #343: fingerprint of the status-shaping config. Embedded in the
/// track change key so a filter/placeholder/format flip mid-track reads as
/// a change and forces one rewrite on the next poll, instead of leaving
/// the stale status posted until the next track change.
///
/// The `None`-config fallbacks mirror `process_track`'s exactly — a
pub(crate) fn status_config_fingerprint(
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> String {
    // Issue #869: the live fingerprint reads the EFFECTIVE config
    // (active profile overlay applied). A profile change must force a
    // status rewrite on the next poll, exactly like any other config
    // flip — the contract is "any config-shaped change mid-track gets
    // one fresh write".
    let effective = config.as_ref().map(crate::config::effective_snapshot);
    let filter = effective
        .as_ref()
        .map(|c| c.teams.profanity_filter)
        .unwrap_or(true);
    let placeholder = effective.as_ref().map_or_else(
        || profanity::safe_placeholder_default_for_locale(config_locale(config)),
        |c| {
            profanity::effective_placeholder(
                &c.teams.profanity_placeholder,
                &c.teams.profanity_extra_words,
                c.locale.as_deref(),
            )
        },
    );
    let format = effective
        .as_ref()
        .map(|c| c.teams.status_format.as_str())
        .unwrap_or("🎵 {artist} - {track} 🎧");
    let extra_words = effective
        .as_ref()
        .map(|c| format!("{:?}", c.teams.profanity_extra_words))
        .unwrap_or_else(|| "[]".to_string());
    // S4 (issue #672): the manual-status texts and the rule schedules are part
    // of the same key, so editing a window, a weekday set, `pause_polling` or
    // one of the two placeholder texts mid-track forces the same one-off
    // rewrite.
    // The API accessors, not the raw fields, so an empty field fingerprints the
    // same as an absent one (it renders the same) instead of forcing a rewrite.
    let paused_format = paused_status_text(config);
    let stopped_format = stopped_status_text(config);
    // Issue #432: rule edits flip the key too, so enabling/disabling a
    // rule or quiet-hours entry mid-track forces one rewrite pass instead
    // of leaving the stale gate decision until the next track change.
    // Full CONTENT (not lengths): a same-length text edit must flip the
    // key, otherwise the stale gate decision stands until the next track.
    // (User content in a change key is safe: it stays in-process, is only
    // compared, and never leaves via log/snapshot — ConfigSummary carries
    // counts only.)
    // Issue #869: the rule list reads from the EFFECTIVE config so a
    // profile's rules overlay flips the key (a profile switch is a
    // config-shaped change).
    let rules = effective.as_ref().map(|c| {
        let q: Vec<String> = c
            .status_rules
            .quiet_hours
            .iter()
            .map(|e| {
                // Finding #634: the quiet-hours replacement text and presence
                // pair are part of the decision, so editing either mid-track
                // must flip the key and force one rewrite (issue #432's
                // contract, widened to the new fields).
                format!(
                    "{}:{}-{}:{:?}:{}:{}:{}:{}",
                    e.enabled,
                    e.start_minutes,
                    e.end_minutes,
                    e.days,
                    e.replacement_status,
                    e.presence_availability,
                    e.presence_activity,
                    e.pause_polling
                )
            })
            .collect();
        let t: Vec<String> = c
            .status_rules
            .track_rules
            .iter()
            .map(|r| {
                format!(
                    "{}:{}:{}:{:?}:{}-{}:{}:{}:{}",
                    r.enabled,
                    r.artist_substring,
                    r.track_substring,
                    r.days,
                    r.start_minutes,
                    r.end_minutes,
                    r.replacement_status,
                    r.presence_availability,
                    r.presence_activity
                )
            })
            .collect();
        format!("quiet=[{}] rules=[{}]", q.join(","), t.join(","))
    });
    format!(
        "filter={filter} placeholder={placeholder} extra_words={extra_words} format={format} paused={paused_format} stopped={stopped_format} rules={}",
        rules.as_deref().unwrap_or("quiet=[] rules=[]")
    )
}

/// The last observed item in full — media plus episode metadata and playback
/// context (issues #580/#581).
///
/// `AppState::polling` stores only the frozen `TrackInfo`, and the issue
/// #343 config-flip rewrite runs on a 304 body-less response, so without this
/// that forced rewrite would render an episode through the music template and
/// drop every playback-context token. Kept in lockstep with
/// `AppState::polling.current_track` — written on the same genuine track
/// change, cleared by the same no-track clear — so the two can never disagree
/// about what is playing.
/// Issue #758: the last-observed-item slot lives on the session, owned by
/// `AppState`; the writer is `SessionState::{store,load}_now_playing`.
///
/// Issue #343: the change key compared against `last_track_key`. Item
/// identity — including the episode marker, so a track and an episode that
/// share a title/artist still re-key (issue #581) — plus the status-shaping
/// config fingerprint.
///
/// Deliberately NOT keyed on `progress_ms` (it advances on every poll, which
/// would rewrite the status continuously) nor on the playback context
/// (device/playlist/shuffle/repeat). A context change the template does not
/// mention still renders byte-identical text and is skipped by the issue #384
/// identical-write check, while one the template DOES mention changes the
/// text and is written by that same check — so keying on it would only add
/// redundant forced writes.
pub(crate) fn status_track_key(
    now: &crate::spotify::NowPlaying,
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> String {
    let kind = match &now.episode {
        Some(episode) => format!("episode:{}:{}", episode.show_name, episode.publisher),
        None => "track".to_string(),
    };
    format!(
        "{} - {} | {} | {}",
        now.media.title,
        now.media.artist,
        kind,
        status_config_fingerprint(config)
    )
}

/// Issue #343: 304 steady-state force-rewrite. A 304 carries no body, so
/// `process_track` never runs and the change key above is never compared —
/// a config flip mid-track would stay stale until the next track change.
/// Returns the last observed item when the stored key no longer matches the
/// current item + config, so `run()` can push one fresh write through
/// `process_track`; `None` otherwise (nothing tracked, or nothing changed).
///
/// Reads `LAST_NOW_PLAYING` rather than the stored `TrackInfo` because the
/// rewrite must render the same template and the same context tokens the
/// live path would have (issue #581): the whole item, not just its media.
pub(crate) fn config_flip_rewrite_track(
    session: &super::state::SessionState,
    last_track_key: &Option<String>,
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> Option<crate::spotify::NowPlaying> {
    let now = session.load_now_playing()?;
    let expected = status_track_key(&now, config);
    if last_track_key.as_ref() != Some(&expected) {
        Some(now)
    } else {
        None
    }
}

/// Expiry for a playing-track status message: now + remaining + buffer when
/// the position is known; `None` (no `expiryDateTime` on the wire) for
/// live/unknown-position streams (issue #165).
pub(crate) fn status_expiry_str(
    remaining_ms: Option<u64>,
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> Option<String> {
    remaining_ms.map(|remaining| {
        let buffer_ms = config
            .as_ref()
            .map(|c| c.polling.expiry_buffer_seconds)
            .unwrap_or(10)
            * 1000;
        let expiry =
            Utc::now() + chrono::Duration::milliseconds(remaining as i64 + buffer_ms as i64);
        super::timing::format_expiry(expiry)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #165: known position → an expiry exists; live stream (None) →
    /// no expiry so no `expiryDateTime` goes on the wire.
    ///
    /// Issue #264: the VALUE must be `now + remaining + buffer` — asserting
    /// the offset-less shape alone passes if the arithmetic sign flips or
    /// the buffer is dropped. The buffer default is read from the config
    /// type rather than hardcoded so a default change cannot silently
    /// invalidate the expectation.
    #[test]
    fn test_status_expiry_known_and_unknown_position() {
        let config = Some(std::sync::Arc::new(crate::config::AppConfig::default()));
        let buffer_secs = config
            .as_ref()
            .expect("config is Some")
            .polling
            .expiry_buffer_seconds;
        let remaining_ms = 120_000u64;

        let before = chrono::Utc::now();
        let s = status_expiry_str(Some(remaining_ms), &config)
            .expect("known position must yield an expiry");
        assert!(
            !s.contains('+') && !s.contains('Z'),
            "offset leaked into status expiry: {}",
            s
        );

        // The wire shape is offset-less; re-attach UTC to parse it back.
        let parsed = chrono::DateTime::parse_from_rfc3339(&format!("{}+00:00", s))
            .expect("status expiry must round-trip as RFC3339 once UTC is re-attached")
            .with_timezone(&Utc);
        let delta_secs = (parsed - before).num_seconds();
        let expected = (remaining_ms / 1000) as i64 + buffer_secs as i64;
        assert!(
            (delta_secs - expected).abs() <= 2,
            "status expiry must be now + remaining + buffer = {}s; got {}s (delta {}s). \
             A flipped `+ buffer_ms` or a zeroed default buffer lands here. See issue #264.",
            expected,
            delta_secs,
            delta_secs - expected
        );

        assert_eq!(
            status_expiry_str(None, &config),
            None,
            "live streams must not get an expiryDateTime"
        );
    }

    /// Issue #343: the change-key fingerprint must move with each of the
    /// status-shaping config values (filter flag, placeholder, format) —
    /// otherwise a mid-track flip reads as "unchanged" and the stale
    /// status stays posted.
    #[test]
    fn test_status_config_fingerprint_tracks_filter_placeholder_format() {
        let base = Some(std::sync::Arc::new(crate::config::AppConfig::default()));
        let fp = status_config_fingerprint(&base);

        let mut off = crate::config::AppConfig::default();
        off.teams.profanity_filter = false;
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(std::sync::Arc::new(off))),
            "toggling the filter must change the fingerprint"
        );

        let mut ph = crate::config::AppConfig::default();
        ph.teams.profanity_placeholder = "something else".to_string();
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(std::sync::Arc::new(ph))),
            "editing the placeholder must change the fingerprint"
        );

        let mut fmt = crate::config::AppConfig::default();
        fmt.teams.status_format = "{track}".to_string();
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(std::sync::Arc::new(fmt))),
            "editing the format must change the fingerprint"
        );

        assert_eq!(
            fp,
            status_config_fingerprint(&base),
            "identical config must fingerprint identically"
        );

        // Issue #432: enabling a rule or quiet-hours entry must flip the
        // fingerprint so the change takes effect mid-track.
        let mut ruled = crate::config::AppConfig::default();
        ruled
            .status_rules
            .quiet_hours
            .push(crate::config::QuietHoursEntry {
                replacement_status: String::new(),
                enabled: true,
                start_minutes: 0,
                end_minutes: 1439,
                days: Vec::new(),
                ..Default::default()
            });
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(std::sync::Arc::new(ruled))),
            "adding a quiet-hours entry must change the fingerprint"
        );

        // S4 (issue #672): the rule schedule, `pause_polling` and the two
        // manual-status texts are part of the decision, so editing any of them
        // mid-track must flip the key (and force one rewrite).
        let scheduled = crate::config::AppConfig {
            status_rules: crate::config::StatusRulesConfig {
                quiet_hours: Vec::new(),
                track_rules: vec![crate::config::TrackRuleEntry {
                    enabled: true,
                    days: vec![1],
                    start_minutes: 480,
                    end_minutes: 1020,
                    ..Default::default()
                }],
                ..crate::config::StatusRulesConfig::default()
            },
            ..Default::default()
        };
        let scheduled_fp = status_config_fingerprint(&Some(std::sync::Arc::new(scheduled.clone())));
        assert_ne!(
            fp, scheduled_fp,
            "adding a scheduled rule must change the fingerprint"
        );

        // The SAME rule with a different window is a different fingerprint: that
        // is what re-evaluates the rule mid-track.
        let mut moved = scheduled.clone();
        moved.status_rules.track_rules[0].end_minutes = 1021;
        assert_ne!(
            scheduled_fp,
            status_config_fingerprint(&Some(std::sync::Arc::new(moved))),
            "editing a rule's window must change the fingerprint"
        );
        let mut other_days = scheduled.clone();
        other_days.status_rules.track_rules[0].days = vec![2];
        assert_ne!(
            scheduled_fp,
            status_config_fingerprint(&Some(std::sync::Arc::new(other_days))),
            "editing a rule's weekday set must change the fingerprint"
        );

        let mut pauses = crate::config::AppConfig::default();
        pauses
            .status_rules
            .quiet_hours
            .push(crate::config::QuietHoursEntry {
                enabled: true,
                start_minutes: 0,
                end_minutes: 1439,
                pause_polling: true,
                ..Default::default()
            });
        let pauses_fp = status_config_fingerprint(&Some(std::sync::Arc::new(pauses)));
        assert_ne!(fp, pauses_fp, "`pause_polling` must change the fingerprint");

        let mut paused_text = crate::config::AppConfig::default();
        paused_text.teams.paused_status_format = "BRB".to_string();
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(std::sync::Arc::new(paused_text))),
            "editing the paused status text must change the fingerprint"
        );

        let mut stopped_text = crate::config::AppConfig::default();
        stopped_text.teams.stopped_status_format = "Idle".to_string();
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(std::sync::Arc::new(stopped_text))),
            "editing the stopped status text must change the fingerprint"
        );
        // An EMPTY text renders the default, so it must fingerprint like the
        // default — otherwise clearing a field would force a rewrite that
        // changes nothing on Teams.
        let mut cleared_texts = crate::config::AppConfig::default();
        cleared_texts.teams.paused_status_format = String::new();
        cleared_texts.teams.stopped_status_format = String::new();
        assert_eq!(
            fp,
            status_config_fingerprint(&Some(std::sync::Arc::new(cleared_texts))),
            "an empty status text must fingerprint like the default it renders"
        );

        // Locale changes move the fingerprint only when they change effective
        // posted text. Untouched defaults are localized; user-authored copy
        // remains byte-identical.
        let german = crate::config::AppConfig {
            locale: Some("de".to_string()),
            ..Default::default()
        };
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(std::sync::Arc::new(german)))
        );

        let custom = crate::config::AppConfig {
            locale: Some("de".to_string()),
            teams: crate::config::TeamsConfig {
                profanity_placeholder: "Eigener Status".to_string(),
                paused_status_format: "Kurze Pause".to_string(),
                stopped_status_format: "Gerade nicht".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            status_config_fingerprint(&Some(std::sync::Arc::new(custom))),
            {
                let mut english = crate::config::AppConfig::default();
                english.teams.profanity_placeholder = "Eigener Status".to_string();
                english.teams.paused_status_format = "Kurze Pause".to_string();
                english.teams.stopped_status_format = "Gerade nicht".to_string();
                status_config_fingerprint(&Some(std::sync::Arc::new(english)))
            },
            "a locale cannot change user-authored status text, so it cannot change its fingerprint"
        );

        let empty_german = crate::config::AppConfig {
            locale: Some("de".to_string()),
            teams: crate::config::TeamsConfig {
                profanity_placeholder: String::new(),
                paused_status_format: String::new(),
                stopped_status_format: String::new(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            status_config_fingerprint(&Some(std::sync::Arc::new(empty_german))),
            status_config_fingerprint(&Some(std::sync::Arc::new(crate::config::AppConfig {
                locale: Some("de".to_string()),
                ..Default::default()
            }))),
            "empty values fingerprint like the localized text they post"
        );

        let mut profane_english = crate::config::AppConfig::default();
        profane_english.teams.profanity_placeholder = "my shit mix".to_string();
        profane_english.teams.paused_status_format = "Custom pause".to_string();
        profane_english.teams.stopped_status_format = "Custom stop".to_string();
        let mut profane_german = profane_english.clone();
        profane_german.locale = Some("de".to_string());
        assert_ne!(
            status_config_fingerprint(&Some(std::sync::Arc::new(profane_english))),
            status_config_fingerprint(&Some(std::sync::Arc::new(profane_german))),
            "a rejected custom placeholder must fingerprint the locale fallback that is actually posted"
        );

        let mut custom_lexicon = crate::config::AppConfig::default();
        custom_lexicon.teams.profanity_placeholder = "Eigener Status".to_string();
        custom_lexicon.teams.profanity_extra_words = vec!["Status".to_string()];
        let mut without_lexicon = custom_lexicon.clone();
        without_lexicon.teams.profanity_extra_words.clear();
        assert_ne!(
            status_config_fingerprint(&Some(std::sync::Arc::new(without_lexicon))),
            status_config_fingerprint(&Some(std::sync::Arc::new(custom_lexicon))),
            "a lexicon that rejects the placeholder must change the fingerprint"
        );
    }

    /// Issue #343: the 304 force-rewrite fires exactly when the stored key
    /// no longer matches the current item + config — nothing tracked, no
    /// rewrite; matching key, no rewrite; flipped config, one rewrite
    /// carrying the last observed item.
    ///
    /// Issue #581: the rewrite carries the WHOLE item, so an episode is
    /// re-rendered through the episode template with its context tokens
    /// instead of being flattened to its media.
    #[test]
    fn test_config_flip_rewrite_track_fires_only_on_mismatch() {
        let config = Some(std::sync::Arc::new(crate::config::AppConfig::default()));
        let now = crate::spotify::NowPlaying {
            media: crate::spotify::TrackInfo {
                title: "T".to_string(),
                artist: "A".to_string(),
                album: String::new(),
                album_art_url: String::new(),
                is_playing: true,
                progress_ms: Some(0),
                duration_ms: 0,
                volume_percent: None,
                supports_volume: None,
                actions: None,
            },
            episode: None,
            context: crate::spotify::PlaybackContext {
                device: "Kitchen speaker".to_string(),
                playlist: "Workout Mix".to_string(),
                shuffle: true,
                repeat: crate::spotify::RepeatState::Context,
            },
        };

        // Nothing tracked → no rewrite. The cache lives on the session, so
        // the test owns its session and needs no teardown or global lock.
        let session = crate::polling::SessionState::new();
        session.store_now_playing(None);
        assert!(
            config_flip_rewrite_track(&session, &None, &config).is_none(),
            "nothing tracked means nothing to rewrite"
        );
        session.store_now_playing(Some(now.clone()));
        let key = status_track_key(&now, &config);
        // Matching key → steady-state 304 stays a no-op.
        assert!(
            config_flip_rewrite_track(&session, &Some(key.clone()), &config).is_none(),
            "a matching key must not force a rewrite"
        );
        // Same item, flipped filter → one rewrite carrying the full item.
        let mut flipped = crate::config::AppConfig::default();
        flipped.teams.profanity_filter = false;
        let rewrite =
            config_flip_rewrite_track(&session, &Some(key), &Some(std::sync::Arc::new(flipped)));
        let rewrite = rewrite.expect("a config flip must force one rewrite");
        assert_eq!(rewrite.media.title, "T");
        assert_eq!(rewrite.media.artist, "A");
        assert_eq!(
            rewrite.context.playlist, "Workout Mix",
            "the rewrite must keep the playback context the live path would render"
        );
        assert_eq!(rewrite.context.repeat, crate::spotify::RepeatState::Context);
    }

    /// Issue #581: an episode and a track that happen to share the same
    /// displayed title/show still re-key, so switching track → episode →
    /// track writes a status at each step instead of deduping them into one.
    #[test]
    fn status_track_key_separates_episodes_from_tracks() {
        let config = Some(std::sync::Arc::new(crate::config::AppConfig::default()));
        let media = crate::spotify::TrackInfo {
            title: "Episode 12".to_string(),
            artist: "The Deep Work Show".to_string(),
            album: String::new(),
            album_art_url: String::new(),
            is_playing: true,
            progress_ms: Some(0),
            duration_ms: 0,
            volume_percent: None,
            supports_volume: None,
            actions: None,
        };
        let as_track = status_track_key(
            &crate::spotify::NowPlaying {
                media: media.clone(),
                episode: None,
                context: crate::spotify::PlaybackContext::default(),
            },
            &config,
        );
        let as_episode = status_track_key(
            &crate::spotify::NowPlaying {
                media,
                episode: Some(crate::spotify::EpisodeInfo {
                    show_name: "The Deep Work Show".to_string(),
                    publisher: "Acme Audio".to_string(),
                }),
                context: crate::spotify::PlaybackContext::default(),
            },
            &config,
        );
        assert_ne!(as_track, as_episode);
        // A publisher change on the same episode is also a new key — the
        // `{publisher}` token may be part of the posted text.
        let other_publisher = status_track_key(
            &crate::spotify::NowPlaying {
                media: crate::spotify::TrackInfo {
                    title: "Episode 12".to_string(),
                    artist: "The Deep Work Show".to_string(),
                    album: String::new(),
                    album_art_url: String::new(),
                    is_playing: true,
                    progress_ms: Some(0),
                    duration_ms: 0,
                    volume_percent: None,
                    supports_volume: None,
                    actions: None,
                },
                episode: Some(crate::spotify::EpisodeInfo {
                    show_name: "The Deep Work Show".to_string(),
                    publisher: "Other Audio".to_string(),
                }),
                context: crate::spotify::PlaybackContext::default(),
            },
            &config,
        );
        assert_ne!(as_episode, other_publisher);
    }

    /// Issue #980: the exact strings handed to Teams localize shipped English
    /// defaults and empty fields, while every user-authored value remains
    /// byte-identical. English keeps the original posted output.
    #[test]
    fn manual_status_placeholders_localize_only_untouched_defaults() {
        use crate::config::AppConfig;

        for (locale, paused, stopped) in [
            ("de", "Pausiert", "Nichts läuft auf Spotify"),
            ("fr", "En pause", "Rien ne joue sur Spotify"),
        ] {
            let defaults = AppConfig {
                locale: Some(locale.to_string()),
                ..Default::default()
            };
            assert_eq!(
                paused_status_placeholder(&Some(std::sync::Arc::new(defaults.clone()))),
                format!("🎵 {paused}")
            );
            assert_eq!(
                stopped_status_placeholder(&Some(std::sync::Arc::new(defaults.clone()))),
                format!("🎵 {stopped}")
            );

            let mut empty = defaults.clone();
            empty.teams.paused_status_format.clear();
            empty.teams.stopped_status_format.clear();
            assert_eq!(
                paused_status_placeholder(&Some(std::sync::Arc::new(empty.clone()))),
                format!("🎵 {paused}")
            );
            assert_eq!(
                stopped_status_placeholder(&Some(std::sync::Arc::new(empty))),
                format!("🎵 {stopped}")
            );

            let mut custom = defaults;
            custom.teams.paused_status_format = "Kurze Pause".to_string();
            custom.teams.stopped_status_format = "Gerade nicht".to_string();
            assert_eq!(
                paused_status_placeholder(&Some(std::sync::Arc::new(custom.clone()))),
                "🎵 Kurze Pause"
            );
            assert_eq!(
                stopped_status_placeholder(&Some(std::sync::Arc::new(custom))),
                "🎵 Gerade nicht"
            );
        }

        let english = Some(std::sync::Arc::new(AppConfig::default()));
        assert_eq!(paused_status_placeholder(&english), "🎵 Paused");
        assert_eq!(
            stopped_status_placeholder(&english),
            "🎵 Nothing playing on Spotify"
        );
        assert_eq!(paused_status_placeholder(&None), "🎵 Paused");
        assert_eq!(
            stopped_status_placeholder(&None),
            "🎵 Nothing playing on Spotify"
        );
    }
}
