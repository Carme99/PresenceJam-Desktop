//! Typed Tauri event payloads (issue #762).
//!
//! The ts-rs pipeline typed every `invoke` payload but none of the event
//! payloads: `serde_json::json!` literals at the `.emit(` sites carried
//! shapes declared nowhere on the Rust side, so renaming a field compiled,
//! passed the suite, and reached the UI as `undefined` at runtime. Every
//! struct here derives `Serialize` + `ts_rs::TS` with
//! `#[ts(export, export_to = "../../src/lib/types-generated/")]`, so a
//! Rust-side rename fails `npm run check` instead. The generated files are
//! gitignored and materialised by `cargo test --lib` — run the Rust tests
//! before `npm run check` whenever an exported struct changes.
//!
//! Conventions:
//! - Field names stay `snake_case` on both sides (the frontend already
//!   reads `is_playing`, `track_key`, `self_terminated`, `user_initiated`,
//!   `used_at`, `expires_at` — no `rename_all` here).
//! - Timestamp fields are pre-formatted RFC 3339 `String`s (the emitters
//!   format with `Utc::now().to_rfc3339()`), not `DateTime<Utc>` — the
//!   event payload is a snapshot, never parsed back into a clock.
//! - `spotify-track-changed` reuses the already-exported [`TrackInfo`](crate::spotify::TrackInfo)
//!   (built by `track_event_payload`), so it has no struct here.
//! - `config-changed` embeds the persisted document as
//!   `serde_json::Value` — the whole `AppConfig` document, already
//!   ts-rs-typed as `AppConfig`, so re-typing it here would duplicate
//!   the schema without pinning anything new. The envelope pins the
//!   `revision`/`config` keys instead.
//! - Unit payloads (`()`) and bare-string payloads (`playback-error`,
//!   `*-auth-failed`, `navigate`, `spotify-reconnect-required` from the
//!   commands layer) have no shape to pin — a rename there changes the
//!   event NAME, which the listener already keys on, not a silent field.
//!   They are listed in [`UNTYPED_STRING_EVENTS`] / [`UNTYPED_UNIT_EVENTS`]
//!   so a future `json!` payload on one of those names is caught in review.
//! - `log://log` belongs to `tauri-plugin-log`, not this crate — the
//!   `LogPayload` interface stays hand-written in `src/lib/types.ts`.
//! - No numeric fields exist on any event payload today (all counts live
//!   in `invoke` payloads / `SyncStatus`); when one is added it MUST use
//!   `#[ts(type = "number")]` from the start — ts-rs's `bigint` default
//!   type-lies about the serde_json wire shape (see `TrackInfo` and the
//!   #765 sequencing note on issue #762).

use serde::{Deserialize, Serialize};

/// Marker events with a `json!(null)` payload — the reconnect prompts.
/// Split from the structured payloads because a single type cannot express
/// both without erasing the distinction the tests assert on (the same
/// split `ErrorEventEmitter`/`emit_marker` already makes in `polling/mod.rs`).
pub const NULL_PAYLOAD_EVENTS: &[&str] = &[
    "spotify-reconnect-required",
    "reconnect-required",
    "teams-reconnect-required",
    "polling-thread-panicked",
];

/// Events carrying a bare `String` payload — a rename changes the event
/// name the listener keys on, not a silent field, so there is no shape to pin.
pub const UNTYPED_STRING_EVENTS: &[&str] = &[
    "playback-error",
    "spotify-auth-failed",
    "teams-auth-failed",
    "navigate",
];

/// Events carrying a unit (`()`) payload.
pub const UNTYPED_UNIT_EVENTS: &[&str] = &[
    "app-shutdown",
    "show-about",
    "toggle-pause",
    "tray-click",
    "open-logs-folder",
    "spotify-auth-complete",
    "teams-auth-complete",
    "sync-started",
];

/// Canonical payload of the frontend `error` event (issue #79).
///
/// Built by `polling::emit_error` / `polling::emit_error_with_recovery`;
/// the Dashboard gates the red fatal banner on `severity == "error"` and
/// renders a Teams `warning` with `recovery == "retry_scheduled"` as a
/// non-fatal `role="status"` warning. `recovery` is absent on ordinary
/// retry warnings so the TypeScript side is `recovery?` rather than a
/// required `string | null`.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct ErrorEvent {
    pub source: String,
    pub message: String,
    pub severity: ErrorSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub recovery: Option<ErrorRecovery>,
}

impl ErrorEvent {
    pub fn new(
        source: &str,
        message: String,
        severity: ErrorSeverity,
        recovery: Option<ErrorRecovery>,
    ) -> Self {
        Self {
            source: source.to_string(),
            message,
            severity,
            recovery,
        }
    }
}

/// Severity tier for `error` events. A transient error the polling loop
/// will retry (a 401 that triggers token refresh, a 429 that triggers
/// back-off) is `warning`; an error that ended the current attempt with
/// no automatic recovery is `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub enum ErrorSeverity {
    Warning,
    Error,
}

/// Recovery action advertised by an error event, when the producer has one.
/// Absent on ordinary retry warnings (see [`ErrorEvent`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub enum ErrorRecovery {
    RetryScheduled,
    ReconnectRequired,
    UserActionRequired,
}

/// Payload of `presence-paused` (finding D7, issue #690): the track is
/// paused so Teams shows the paused placeholder instead of the playing
/// status. The Dashboard reads `status` to keep the track card up.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PresencePaused {
    pub status: String,
}

/// Payload of `playback-state-changed` (finding D6, issue #689): the
/// poller observed the playing-state flip for the track it already tracks.
/// The Dashboard reads `is_playing`; `track_key` is part of the wire shape
/// (pinned by the shape test) but intentionally unconsumed — the Dashboard
/// card renders its own hydrated track and the tray's dedup key already
/// carries the track identity.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PlaybackStateChanged {
    pub is_playing: bool,
    pub track_key: String,
}

/// Payload of `presence-gated`: the write is being suppressed, with the
/// poller's reason. The single emitter is `emit_presence_gated` in
/// `polling/poll_once.rs` (~7 call sites funnel through it).
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PresenceGated {
    pub reason: String,
    pub availability: String,
    pub activity: String,
    pub timestamp: String,
}

/// Payload of `presence-updated`: a status reached Teams. Emitted after a
/// playing write in `process_track`.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PresenceUpdated {
    pub status: String,
    pub timestamp: String,
}

/// Payload of `presence-cleared`: the track stopped and the Teams status
/// was removed. The only transition that drops the Dashboard track card.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PresenceCleared {
    pub timestamp: String,
}

/// Payload of `presence-availability-updated`: the app's presence session
/// was armed (`available: true`) or cleared (`available: false`).
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PresenceAvailabilityUpdated {
    pub available: bool,
    pub label: String,
    pub timestamp: String,
}

/// Payload of `preferred-presence-updated` (issue #866): the app's
/// `setUserPreferredPresence` session was armed or cleared. The armed case
/// carries the pair, the label, and both expiries; the cleared case
/// carries `available: false` plus the label and timestamp, so the arm
/// fields are `Option`al and skipped when absent.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PreferredPresenceUpdated {
    pub available: bool,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub availability: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub activity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expires_at: Option<String>,
    pub timestamp: String,
}

/// Payload of `sync-stopped` (finding D5, issue #688): `self_terminated:
/// true` is the poller's own exit (`polling/state.rs`), `false` the
/// explicit user stop (`commands/sync.rs`) — the notification consumer
/// toasts only the surprise.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct SyncStopped {
    pub self_terminated: bool,
}

/// Payload of `manual-status-updated` (issue #870): the set case carries
/// the new [`ManualStatus`](crate::commands::status::ManualStatus) plus
/// the `filtered` flag and the user's raw `user_text`; the clear and
/// expiry cases carry `cleared: true` / `expired: true` (+ `expires_at`
/// on expiry) instead — consumed in `Dashboard.svelte` as the generated
/// `ManualStatusUpdated` (imported from `$lib/types`, issue #762).
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct ManualStatusUpdated {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub manual_status: Option<crate::commands::status::ManualStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub filtered: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub user_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cleared: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expired: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expires_at: Option<String>,
}

/// Payload of `spotify-secret-conflict`: the legacy-plaintext migration
/// found a *different* secret in the OS keychain. The message is a fixed
/// literal (not user input), so a `String` pins the key without
/// over-constraining the copy.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct SpotifySecretConflict {
    pub action: String,
    pub message: String,
}

/// Envelope of `config-changed`: the `revision` plus the whole persisted
/// document (already typed as `AppConfig` on the TS side). The document
/// itself is a `serde_json::Value` so this struct pins the envelope keys
/// without duplicating the `AppConfig` schema.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct ConfigChanged {
    // Tauri IPC crosses the boundary via serde_json, which decodes u64 as
    // JS `number` (f64) — ts-rs's `bigint` default would type-lie about the
    // wire shape (same override `TrackInfo.duration_ms` carries; #765
    // sequencing: `number` from the start, no rework when #765 lands).
    #[ts(type = "number")]
    pub revision: u64,
    #[ts(type = "import('./AppConfig').AppConfig")]
    pub config: serde_json::Value,
}

/// Payload of `teams-reconnect-required` when user-initiated
/// (`commands/onboarding.rs::reconnect_teams` — the ONLY user-initiated
/// emitter, issue #675): the notification consumer stays quiet for the
/// reconnect the user just asked for. The poller's dead-session emitters
/// use the null marker instead.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct TeamsReconnectRequired {
    pub user_initiated: bool,
}

/// Payload of `spotify-auth-persist-warning` / `teams-auth-persist-warning`:
/// the token commit succeeded in memory but persistence failed, so the
/// session is live only until the next restart. Both providers share the
/// shape so the frontend drives a single banner for both.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct AuthPersistWarning {
    pub provider: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `.emit(` payload with a `json!` shape must have a struct here —
    /// a new inline payload bypasses the ts-rs pin this module exists for.
    /// (Unit / bare-string / null payloads are listed in the `UNTYPED_*` /
    /// `NULL_PAYLOAD_EVENTS` consts above, not here.)
    #[test]
    fn every_structured_emit_payload_has_a_typed_struct() {
        let cases: &[(&str, &str, serde_json::Value)] = &[
            (
                "error",
                "source/message/severity/recovery?",
                serde_json::to_value(ErrorEvent {
                    source: "spotify".to_string(),
                    message: "m".to_string(),
                    severity: ErrorSeverity::Warning,
                    recovery: None,
                })
                .expect("ErrorEvent must serialize"),
            ),
            (
                "presence-paused",
                "status",
                serde_json::to_value(PresencePaused {
                    status: "s".to_string(),
                })
                .expect("PresencePaused must serialize"),
            ),
            (
                "playback-state-changed",
                "is_playing/track_key",
                serde_json::to_value(PlaybackStateChanged {
                    is_playing: true,
                    track_key: "k".to_string(),
                })
                .expect("PlaybackStateChanged must serialize"),
            ),
            (
                "presence-gated",
                "reason/availability/activity/timestamp",
                serde_json::to_value(PresenceGated {
                    reason: "r".to_string(),
                    availability: "a".to_string(),
                    activity: "a".to_string(),
                    timestamp: "t".to_string(),
                })
                .expect("PresenceGated must serialize"),
            ),
            (
                "presence-updated",
                "status/timestamp",
                serde_json::to_value(PresenceUpdated {
                    status: "s".to_string(),
                    timestamp: "t".to_string(),
                })
                .expect("PresenceUpdated must serialize"),
            ),
            (
                "presence-cleared",
                "timestamp",
                serde_json::to_value(PresenceCleared {
                    timestamp: "t".to_string(),
                })
                .expect("PresenceCleared must serialize"),
            ),
            (
                "presence-availability-updated",
                "available/label/timestamp",
                serde_json::to_value(PresenceAvailabilityUpdated {
                    available: true,
                    label: "l".to_string(),
                    timestamp: "t".to_string(),
                })
                .expect("PresenceAvailabilityUpdated must serialize"),
            ),
            (
                "preferred-presence-updated",
                "available/label/availability?/activity?/expires_at?/timestamp",
                serde_json::to_value(PreferredPresenceUpdated {
                    available: false,
                    label: "l".to_string(),
                    availability: None,
                    activity: None,
                    expires_at: None,
                    timestamp: "t".to_string(),
                })
                .expect("PreferredPresenceUpdated must serialize"),
            ),
            (
                "sync-stopped",
                "self_terminated",
                serde_json::to_value(SyncStopped {
                    self_terminated: true,
                })
                .expect("SyncStopped must serialize"),
            ),
            (
                "manual-status-updated",
                "manual_status?/filtered?/user_text?/cleared?/expired?/expires_at?",
                serde_json::to_value(ManualStatusUpdated {
                    manual_status: None,
                    filtered: None,
                    user_text: None,
                    cleared: Some(true),
                    expired: None,
                    expires_at: None,
                })
                .expect("ManualStatusUpdated must serialize"),
            ),
            (
                "spotify-secret-conflict",
                "action/message",
                serde_json::to_value(SpotifySecretConflict {
                    action: "reconnect-spotify".to_string(),
                    message: "m".to_string(),
                })
                .expect("SpotifySecretConflict must serialize"),
            ),
            (
                "config-changed",
                "revision/config",
                serde_json::to_value(ConfigChanged {
                    revision: 1,
                    config: serde_json::json!({}),
                })
                .expect("ConfigChanged must serialize"),
            ),
            (
                "teams-reconnect-required",
                "user_initiated",
                serde_json::to_value(TeamsReconnectRequired {
                    user_initiated: true,
                })
                .expect("TeamsReconnectRequired must serialize"),
            ),
            (
                "spotify-auth-persist-warning",
                "provider/message",
                serde_json::to_value(AuthPersistWarning {
                    provider: "spotify".to_string(),
                    message: "m".to_string(),
                })
                .expect("AuthPersistWarning must serialize"),
            ),
        ];
        for (event, shape, value) in cases {
            let obj = value
                .as_object()
                .unwrap_or_else(|| panic!("{event} must serialize to an object"));
            for key in shape.split('/') {
                let optional = key.ends_with('?');
                let key = key.trim_end_matches('?');
                if optional {
                    // `skip_serializing_if` omits `None` — the key must
                    // round-trip when `Some`, checked by the struct's own
                    // construction above; absence here is the documented
                    // `ts(optional)` shape, not a missing field.
                    continue;
                }
                assert!(
                    obj.contains_key(key),
                    "{event} must carry `{key}` (shape: {shape})"
                );
            }
        }
        // `spotify-track-changed` reuses the already-exported `TrackInfo`
        // (built by `track_event_payload`); `update-stage-progress` and
        // `update-stage-complete` are already ts-rs-typed in `updater_bg.rs`
        // (`StageProgressEvent` / `StageComplete`).
    }
}
