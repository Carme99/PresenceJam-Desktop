use super::clamp::deserialize_optional_tag;
use super::schema::{
    AppConfig, PreferredPresenceConfig, QuietHoursEntry, TrackRuleEntry, UpdateChannel,
};
use serde::{Deserialize, Serialize};
/// Field-level patch for the `spotify` section (CfgDiag#0, issue #535).
///
/// `client_secret_set` and `client_secret_state` are deliberately absent:
/// both are derived display values filled in by [`with_keychain_flags`] from
/// the OS keychain, so a client must not be able to assert them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct SpotifyPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
}

/// Field-level patch for the `teams` section (CfgDiag#0, issue #535).
///
/// Every user-facing field of [`TeamsConfig`] is carried here, so no
/// `update_config` caller is pushed onto the whole-document `save_config`
/// path for want of a field (issue #767). Every value is re-clamped by
/// [`clamp_teams`] through [`clamped_config`] after the merge.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct TeamsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_on_pause: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profanity_filter: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profanity_placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_minimized: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_sync: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_gate: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profanity_extra_words: Option<Vec<String>>,
    /// Finding #635 (issue #635): never overwrite a Teams status message the
    /// user set by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub respect_manual_status: Option<bool>,
    /// Finding #637 (issue #637): also gate the status write while the user
    /// is marked out of office.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_when_out_of_office: Option<bool>,
    /// Issue #872: also gate while the OS reports a full-screen app,
    /// presentation mode or Quiet Time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_when_presenting: Option<bool>,
    /// Issue #873: idle threshold in seconds; `0` disables the gate.
    /// Clamped into `60..=3600` (or left at `0`) by `clamp_teams`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub idle_away_after_seconds: Option<u64>,
    /// Issue #867: minutes before a meeting starts that the write is
    /// suppressed; `0` means during the meeting only. Capped at 60 by
    /// `clamp_teams`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_meeting_suppress_minutes: Option<u16>,
    /// S4 (issue #672): the user-templatable paused/stopped status texts, part
    /// of the same field-level patch as the rest of the section — a Settings
    /// save that omitted them would leave the stored text untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_status_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_status_format: Option<String>,
    /// Issue #866: the preferred-presence config. Replaced wholesale when
    /// present, exactly like the rule lists above — there is no per-field
    /// addressing, and the Settings pane edits the section as one form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_presence: Option<PreferredPresenceConfig>,
}

/// Field-level patch for the `updates` section (issue #767).
///
/// The channel is the section's only user-facing field. Unlike
/// [`UpdatesConfig::channel`] on the config itself, an unrecognised
/// spelling here REJECTS the patch rather than being read leniently: a
/// partial write is a deliberate IPC action by a caller that already holds
/// the rendered channel list, so a value outside the enum means the two
/// sides disagree — and the stored document is then left untouched, which
/// is the safe answer for a partial write.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct UpdatesPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<UpdateChannel>,
}

/// Field-level patch for the `shortcuts` section (issue #767).
///
/// `None` in the stored [`ShortcutsConfig`] is the documented "unbound"
/// state, so a patch names a slot with a string and leaves it out to keep
/// the stored binding. A blank string is normalised to unbound by
/// `clamp_shortcuts`, exactly as
/// [`crate::commands::shortcuts::configured_binding`] already reads it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct ShortcutsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle_playback: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle_sync: Option<String>,
}

/// Field-level patch for the `polling` section (CfgDiag#0, issue #535). Every
/// value is re-clamped by [`clamped_config`] after the merge.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PollingPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub default_interval_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub minimum_interval_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub max_interval_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub expiry_buffer_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub pause_backoff_max_seconds: Option<u64>,
}

/// Field-level patch for the `logging` section (CfgDiag#0, issue #535).
///
/// Issue #767: the rotation settings (`max_file_size_mb`, `keep_files`) and
/// the issue #877 `presence_history` mirror are user-facing too, so they
/// ride the same partial-write path instead of forcing a whole-document
/// save. Re-clamped by [`clamp_logging`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct LoggingPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,
    /// Rotation ceiling in mebibytes; clamped to `1..=500` by `clamp_logging`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub max_file_size_mb: Option<u64>,
    /// Archived log files to retain; clamped to `1..=20` by `clamp_logging`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_files: Option<u32>,
    /// Issue #877: mirror the bounded status-decision history to disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_history: Option<bool>,
}

/// Field-level patch for the `status_rules` section (CfgDiag#0, issue #535).
///
/// A named list is replaced wholesale — there is no per-entry addressing, so
/// naming `quiet_hours` means "this is the new list". Omitting it leaves the
/// stored list untouched.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct StatusRulesPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_hours: Option<Vec<QuietHoursEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_rules: Option<Vec<TrackRuleEntry>>,
}

/// Field-level patch for the `notifications` section (issue #789). Each
/// class is an `Option<bool>` so a toggle names only its own class; an
/// absent class leaves the stored flag untouched, mirroring the per-field
/// shape of every other section patch above.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct NotificationsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_change: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_stopped: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_staged: Option<bool>,
}

/// Field-level update to [`AppConfig`] for callers that only know part of
/// the document (CfgDiag#0, issue #535).
///
/// `save_config` is a whole-document replace, so every caller had to already
/// hold a complete, current `AppConfig`. A caller that did not — the setup
/// wizard being the first — silently reset everything it omitted, which is
/// the backend half of the #531 config-clobber family.
///
/// **Every field of every nested patch is `Option` and skipped when absent.**
/// That shape is load-bearing, not stylistic: typing a section as the whole
/// `TeamsConfig` would deserialize a patch of `{"teams":
/// {"status_format": "x"}}` into a fully populated `TeamsConfig` whose
/// omitted fields took their *defaults*, and assigning that section would
/// reset the user's `start_minimized`, profanity and presence settings — the
/// very clobber this command exists to prevent, reproduced one level down.
/// An absent key MUST leave the stored value untouched.
///
/// **Every section the app can write is represented here** (issue #767).
/// A section missing from this struct is a section whose only write path is
/// the whole-document `save_config` — the clobber class this type exists to
/// end. `presence_profiles` / `active_profile` are deliberately absent: the
/// tray's profile picker owns them and reaches `save_config` under the write
/// guard itself (issue #869), so a second addressing scheme would only add a
/// way for the two to disagree.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct ConfigPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spotify: Option<SpotifyPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teams: Option<TeamsPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polling: Option<PollingPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingPatch>,
    /// Release channel for the updater (issue #767).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updates: Option<UpdatesPatch>,
    /// Playback source selection (issue #767).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playback: Option<PlaybackPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autostart: Option<bool>,
    // Issue #789: one-class toggle + pause-sync deadline for the same
    // merge-instead-of-replace path. `snooze_until` is `Option<Option<_>>`
    // so the three states stay distinct: absent leaves the stored deadline
    // untouched, `Some(None)` clears it, `Some(Some(..))` sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notifications: Option<NotificationsPatch>,
    // Issue #789: the pause-sync deadline for the same partial-write path.
    // `Option<Option<_>>` so the three states stay distinct: absent leaves the
    // stored deadline untouched, `Some(None)` clears it, `Some(Some(..))` sets
    // a new one. `deserialize_with` is what makes the middle state reachable —
    // see `deserialize_optional_tag`.
    #[serde(
        default,
        deserialize_with = "deserialize_optional_tag",
        skip_serializing_if = "Option::is_none"
    )]
    pub snooze_until: Option<Option<String>>,
    /// UI locale (issue #767). Same three-state shape as `snooze_until`:
    /// absent leaves the stored tag untouched, `Some(None)` clears it back to
    /// the documented "follow the OS" default, `Some(Some(..))` sets it. The
    /// value is canonicalised by `clamp_locale` on the way out, so a patch
    /// cannot persist a tag the app cannot render.
    #[serde(
        default,
        deserialize_with = "deserialize_optional_tag",
        skip_serializing_if = "Option::is_none"
    )]
    pub locale: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_rules: Option<StatusRulesPatch>,
    /// Global-shortcut bindings (issue #767).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcuts: Option<ShortcutsPatch>,
}

/// Field-level patch for the `playback` section (issue #767).
///
/// `source` is the section's only field; `Auto` is the documented default and
/// is what an untouched config resolves to.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PlaybackPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<crate::sources::PlaybackSourceKind>,
}

/// Merge a patch into `base`, field by field. Only the fields the patch
/// explicitly names are overwritten; everything else — including `extra` and
/// the binary-owned `schema_version` — is left exactly as it was.
///
/// Pure, so the merge guarantee is unit-testable without touching disk.
pub fn apply_patch(base: &mut AppConfig, patch: &ConfigPatch) {
    if let Some(p) = &patch.spotify {
        if let Some(v) = &p.client_id {
            base.spotify.client_id = v.clone();
        }
        if let Some(v) = &p.redirect_uri {
            base.spotify.redirect_uri = v.clone();
        }
    }
    if let Some(p) = &patch.teams {
        if let Some(v) = &p.status_format {
            base.teams.status_format = v.clone();
        }
        if let Some(v) = p.clear_on_pause {
            base.teams.clear_on_pause = v;
        }
        if let Some(v) = p.profanity_filter {
            base.teams.profanity_filter = v;
        }
        if let Some(v) = &p.profanity_placeholder {
            base.teams.profanity_placeholder = v.clone();
        }
        if let Some(v) = p.start_minimized {
            base.teams.start_minimized = v;
        }
        if let Some(v) = p.availability_sync {
            base.teams.availability_sync = v;
        }
        if let Some(v) = p.presence_gate {
            base.teams.presence_gate = v;
        }
        if let Some(v) = &p.profanity_extra_words {
            base.teams.profanity_extra_words = v.clone();
        }
        // Issue #767: the presence gates and the two bounded windows join the
        // patch, so a caller that owns one toggle no longer has to hold the
        // whole document to change it. `clamp_teams` re-bounds them after the
        // merge, exactly as it does for a full save.
        if let Some(v) = p.respect_manual_status {
            base.teams.respect_manual_status = v;
        }
        if let Some(v) = p.gate_when_out_of_office {
            base.teams.gate_when_out_of_office = v;
        }
        if let Some(v) = p.gate_when_presenting {
            base.teams.gate_when_presenting = v;
        }
        if let Some(v) = p.idle_away_after_seconds {
            base.teams.idle_away_after_seconds = v;
        }
        if let Some(v) = p.pre_meeting_suppress_minutes {
            base.teams.pre_meeting_suppress_minutes = v;
        }
        if let Some(v) = &p.paused_status_format {
            base.teams.paused_status_format = v.clone();
        }
        if let Some(v) = &p.stopped_status_format {
            base.teams.stopped_status_format = v.clone();
        }
        if let Some(v) = &p.preferred_presence {
            base.teams.preferred_presence = v.clone();
        }
    }
    if let Some(p) = &patch.polling {
        if let Some(v) = p.default_interval_seconds {
            base.polling.default_interval_seconds = v;
        }
        if let Some(v) = p.minimum_interval_seconds {
            base.polling.minimum_interval_seconds = v;
        }
        if let Some(v) = p.max_interval_seconds {
            base.polling.max_interval_seconds = v;
        }
        if let Some(v) = p.expiry_buffer_seconds {
            base.polling.expiry_buffer_seconds = v;
        }
        if let Some(v) = p.pause_backoff_max_seconds {
            base.polling.pause_backoff_max_seconds = v;
        }
    }
    if let Some(p) = &patch.logging {
        if let Some(v) = p.enabled {
            base.logging.enabled = v;
        }
        if let Some(v) = &p.log_level {
            base.logging.log_level = v.clone();
        }
        // Issue #767: the rotation ceiling, the retained-archive count and the
        // #877 history mirror. `clamp_logging` re-bounds the two numbers.
        if let Some(v) = p.max_file_size_mb {
            base.logging.max_file_size_mb = v;
        }
        if let Some(v) = p.keep_files {
            base.logging.keep_files = v;
        }
        if let Some(v) = p.presence_history {
            base.logging.presence_history = v;
        }
    }
    // Issue #767: the release channel. Replaced wholesale (it is a single
    // field), and an unrecognised spelling never reaches this function —
    // serde rejects the patch instead, leaving the stored document alone.
    if let Some(p) = &patch.updates {
        if let Some(v) = p.channel {
            base.updates.channel = v;
        }
    }
    // Issue #767: the playback source selection.
    if let Some(p) = &patch.playback {
        if let Some(v) = p.source {
            base.playback.source = v;
        }
    }
    if let Some(v) = patch.autostart {
        base.autostart = v;
    }
    if let Some(p) = &patch.notifications {
        if let Some(v) = p.track_change {
            base.notifications.track_change = v;
        }
        if let Some(v) = p.sync_stopped {
            base.notifications.sync_stopped = v;
        }
        if let Some(v) = p.auth_required {
            base.notifications.auth_required = v;
        }
        if let Some(v) = p.update_staged {
            base.notifications.update_staged = v;
        }
    }
    // Absent leaves the stored deadline untouched; `Some(None)` clears an
    // active pause; `Some(Some(..))` sets a new one. The write path's
    // `clamp_snooze` still drops an expired value afterwards.
    if let Some(v) = &patch.snooze_until {
        base.snooze_until = v.clone();
    }
    if let Some(p) = &patch.status_rules {
        if let Some(v) = &p.quiet_hours {
            base.status_rules.quiet_hours = v.clone();
        }
        if let Some(v) = &p.track_rules {
            base.status_rules.track_rules = v.clone();
        }
    }
    // Issue #767: `locale` mirrors `snooze_until`'s three states, so a patch
    // that names no locale leaves the user's tag exactly as it was.
    if let Some(v) = &patch.locale {
        base.locale = v.clone();
    }
    // Issue #767: shortcut bindings, one slot at a time. `clamp_shortcuts`
    // normalises a blank binding to "unbound" on the write path.
    if let Some(p) = &patch.shortcuts {
        if let Some(v) = &p.toggle_playback {
            base.shortcuts.toggle_playback = Some(v.clone());
        }
        if let Some(v) = &p.toggle_sync {
            base.shortcuts.toggle_sync = Some(v.clone());
        }
    }
}
