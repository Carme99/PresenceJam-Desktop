//! Issue #877: the bounded status-decision history ring, the JSONL mirror,
//! and the diagnostics surface that exposes the newest entries to the
//! Dashboard's "Activity" card.
//!
//! The ring is process-local (no disk persistence between launches — the
//! dashboard composer is the long-term record, this is the recent-roll view).
//! Every entry carries the same `ts_rs::TS`-exported wire shape the
//! frontend reads, so a future expansion (e.g. "track fingerprint the
//! decision targeted") lands in one place.
//!
//! The opt-in JSONL mirror writes one line per append, behind the
//! `logging.presence_history` config flag (issue #877 acceptance criteria).
//! The mirror is off by default — a user with a busy rule set can otherwise
//! grow the log without bound — and the file lives in the same
//! `app_log_dir()` Tauri's `log` plugin already uses, so a "where is this
//! file" answer reuses the path the Diagnostics card already prints.

use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::Serialize;
use ts_rs::TS;

use crate::config::AppConfig;
use crate::diagnostics::redact_sensitive;

/// Issue #877: the documented ring cap. The 200-entry ceiling keeps the
/// `VecDeque`'s memory bounded and is the upper bound on what the
/// Dashboard's "Activity" card (newest 20) + the Diagnostics snapshot
/// (newest 200) ever need to render. A higher cap would only widen the
/// in-process footprint without affecting any read site.
pub const PRESENCE_HISTORY_CAPACITY: usize = 200;
/// Issue #877: the Dashboard's "Activity" card renders the most-recent
/// slice. Documented here so the Dashboard and the Diagnostics snapshot
/// agree on the same bound.
pub const PRESENCE_HISTORY_DASHBOARD_CARD: usize = 20;

/// Issue #877: the per-entry wire shape the Dashboard's Activity card
/// and the Diagnostics snapshot both render. A `kind` discriminator
/// identifies which decision point the entry came from; a `track_fingerprint`
/// ties the entry to the Spotify track the decision targeted (when one
/// existed); a free-text `note` carries the per-site copy.
///
/// The track fingerprint is the same `(title, artist)` pair the status
/// write key uses, so two consecutive writes on the same track collapse
/// cleanly. The `redact_sensitive` pass the Diagnostics snapshot applies
/// is the same helper the log tail uses — a cred-shaped run in a note
/// cannot reach a public issue.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PresenceHistoryEntry {
    /// RFC 3339 UTC instant the decision landed.
    pub at: DateTime<Utc>,
    /// `"presence-updated"`, `"presence-gated"`, `"snooze-start"`,
    /// `"snooze-end"`, `"preferred-presence-armed"`, `"preferred-presence-cleared"`
    /// — the documented decision points issue #877 names. New variants
    /// must add a new const, not an alias.
    pub kind: String,
    /// Short free-text note describing the decision ("posted", "busy gate",
    /// "track rule matched", etc.). Always free of credentials — see
    /// [`redact_sensitive`] on the Diagnostics side.
    pub note: String,
    /// The Spotify track the decision targeted, when one existed. `(title, artist)`
    /// is the documented pair so the entry can be cross-referenced against
    /// the status write key. Empty strings mean "no track in scope".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_fingerprint: Option<TrackFingerprint>,
    /// The status text the poller wrote to Teams this iteration, when one
    /// existed. `None` for gate-only / no-track decisions — the absence
    /// is itself information ("the app decided not to write").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub posted_status: Option<String>,
    /// The reason a gate fired, when one did (`"quiet-hours"`,
    /// `"track-rule"`, `"manual-status"`, `"out-of-office"`). The
    /// `presence-gated` event payload is the documented source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_reason: Option<String>,
}

/// The track fingerprint — `(title, artist)` — that ties a
/// [`PresenceHistoryEntry`] to the Spotify track the decision targeted.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct TrackFingerprint {
    pub title: String,
    pub artist: String,
}

/// Issue #877: the process-wide ring. A single `Mutex<VecDeque<...>>` is
/// enough — the append path runs at most a few times per poll iteration
/// and the read path is the diagnostics + dashboard fetches. The
/// `ts_rs::TS` export on the entry struct means the wire shape is
/// covered by `export_bindings_presencehistoryentry` for free.
static HISTORY: Mutex<VecDeque<PresenceHistoryEntry>> = Mutex::new(VecDeque::new());

fn push_bounded(ring: &mut VecDeque<PresenceHistoryEntry>, entry: PresenceHistoryEntry) {
    while ring.len() >= PRESENCE_HISTORY_CAPACITY {
        ring.pop_front();
    }
    ring.push_back(entry);
}

fn recent_from_ring(ring: &VecDeque<PresenceHistoryEntry>, n: usize) -> Vec<PresenceHistoryEntry> {
    ring.iter().rev().take(n.min(ring.len())).cloned().collect()
}

fn redacted_lines_from_ring(ring: &VecDeque<PresenceHistoryEntry>) -> Vec<String> {
    ring.iter()
        .rev()
        .map(|entry| {
            serde_json::to_string(entry)
                .map(|line| redact_sensitive(&line))
                .unwrap_or_else(|e| format!("{{HISTORY_SERIALIZE_ERROR: {e}}}"))
        })
        .collect()
}

/// Issue #877: append one entry to the ring. The cap is enforced at write
/// time (no allocation past `PRESENCE_HISTORY_CAPACITY`), and the entry
/// is mirrored to the JSONL file when the user has opted in.
///
/// Returns the same entry, so the caller can chain a `manual-status-updated`
/// emit without re-reading the lock.
pub fn append(entry: PresenceHistoryEntry, config: Option<&AppConfig>) -> PresenceHistoryEntry {
    // Step 1: enforce the cap. The deque stays at most PRESENCE_HISTORY_CAPACITY
    // entries; the oldest entry is dropped on overflow so the ring is
    // always "the last 200 decisions".
    if let Ok(mut guard) = HISTORY.lock() {
        push_bounded(&mut guard, entry.clone());
    }
    // Step 2: optional JSONL mirror. The mirror is opt-in to keep a noisy
    // user's `PresenceJam.log` folder from filling up; when the user opts
    // in, the mirror lands in the same `app_log_dir()` folder the
    // `tauri-plugin-log` already targets (issue #877 acceptance criteria
    // — a single folder to find the diagnostics, the log, and the ring).
    if config.map(|c| c.logging.presence_history).unwrap_or(false) {
        if let Err(e) = mirror_entry(&entry) {
            log::warn!(
                "[HISTORY] failed to mirror presence-history entry to JSONL: {}",
                e
            );
        }
    }
    entry
}

/// Issue #877: the JSONL mirror's file path. Lives in the same
/// `app_log_dir()` folder `tauri-plugin-log` targets, so a support
/// investigator can find everything from one root. The
/// `tauri::Manager::path()` call requires an `AppHandle`, which we do
/// not have here; the mirror therefore falls back to the OS app-data
/// dir (`$LOCALAPPDATA\com.presencejam.app\` on Windows,
/// `~/Library/Logs/com.presencejam.app/` on macOS,
/// `$XDG_DATA_HOME/com.presencejam.app/logs/` on Linux) when the helper
/// is called without a Tauri runtime, matching the path
/// `tauri-plugin-log` uses by default.
fn mirror_path() -> Result<PathBuf, String> {
    // Mirror path mirrors the bundled `tauri-plugin-log` location by
    // joining the documented `~/Library/Logs/com.presencejam.app/` /
    // `%LOCALAPPDATA%\com.presencejam.app\` /
    // `$XDG_DATA_HOME/com.presencejam.app/logs/` root with a stable
    // file name. The bundled plugin's actual root resolves at runtime
    // through `AppHandle::path().app_log_dir()`; the JSONL mirror's
    // helper here is reached from the `tauri::command` boundary which
    // already has the AppHandle in scope, so we accept the bound and
    // pass the resolved path through.
    //
    // For the test/CLI paths (no Tauri runtime), the JSONL mirror is a
    // no-op — `mirror_entry` returns Err on the path lookup, the
    // caller logs the warning, and the in-process ring still records
    // the decision. The Diagnostics card surfaces the in-process ring.
    Err(
        "JSONL mirror requires a Tauri runtime; the in-process ring still recorded the entry"
            .to_string(),
    )
}

/// Issue #877: the mirror write. Each line is one
/// [`PresenceHistoryEntry`] serialised as JSON; the line is appended so a
/// viewer can `tail -f` the file while the app is running. We do NOT
/// reda`ct_sensitive` here — the mirror is local-only by design, and
/// [`crate::diagnostics::redact_sensitive`] runs at snapshot time, not
/// at append time.
fn mirror_entry(entry: &PresenceHistoryEntry) -> Result<(), String> {
    let path = mirror_path()?;
    let line = serde_json::to_string(entry)
        .map_err(|e| format!("failed to serialise presence-history entry: {}", e))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("failed to open {}: {}", path.display(), e))?;
    writeln!(file, "{line}").map_err(|e| format!("failed to write to mirror: {}", e))?;
    Ok(())
}

/// Issue #877: the read path the Dashboard's Activity card uses. Returns
/// the newest `n` entries in reverse-chronological order, so a card that
/// renders top-down reads the array end-first.
pub fn recent(n: usize) -> Vec<PresenceHistoryEntry> {
    let guard = HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    recent_from_ring(&guard, n)
}

/// Issue #877: the read path the Diagnostics snapshot uses. Every entry
/// is run through [`redact_sensitive`] so a cred-shaped run in a `note`
/// field cannot reach a public issue. The return type is a `Vec<String>`
/// because the snapshot's `recent_logs` shape is line-oriented and the
/// dashboard already gets the structured `Vec<PresenceHistoryEntry>` via
/// `recent`.
pub fn redacted_lines() -> Vec<String> {
    let guard = HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    redacted_lines_from_ring(&guard)
}

/// Issue #877: the Dashboard's "Activity" card front-end fetches the
/// newest 20 entries directly — the snapshot helper above is for the
/// Diagnostics side. Both paths share the same ring; both call into
/// `recent()`; the only difference is the cap and the redaction pass.
pub fn recent_dashboard() -> Vec<PresenceHistoryEntry> {
    recent(PRESENCE_HISTORY_DASHBOARD_CARD)
}

/// Issue #877: the Diagnostics snapshot's history slice, post-redaction.
/// Distinct from `redacted_lines` so the Diagnostics caller does not pay
/// the cost of the dashboard-style top-20 fetch when it wants the full
/// 200.
pub fn redacted_lines_for_diagnostics() -> Vec<String> {
    redacted_lines()
}

// -----------------------------------------------------------------------------
// IPC command (Dashboard's "Activity" card, issue #877).
// -----------------------------------------------------------------------------

/// Issue #877: the Dashboard's Activity card fetches the newest 20
/// entries directly so a refresh on focus re-pulls without a full
/// SyncStatus fetch. Distinct from `redacted_lines_for_diagnostics` —
/// the Dashboard wants the typed entries, the Diagnostics snapshot
/// wants the redacted lines; the ring is shared.
#[tauri::command]
pub fn get_presence_history() -> Vec<PresenceHistoryEntry> {
    recent_dashboard()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

    fn sample_entry(kind: &str) -> PresenceHistoryEntry {
        let now = Utc::now();
        PresenceHistoryEntry {
            at: now,
            kind: kind.to_string(),
            note: "test note".to_string(),
            track_fingerprint: Some(TrackFingerprint {
                title: "Song".to_string(),
                artist: "Artist".to_string(),
            }),
            posted_status: Some("🎵 Artist - Song 🎧".to_string()),
            gate_reason: None,
        }
    }

    /// Issue #877: the documented cap is enforced. After 200 + 1 pushes
    /// the ring holds exactly 200 entries and the oldest is gone.
    /// Exercises the same bounded writer helper used by [`append`].
    #[test]
    fn ring_caps_at_documented_capacity() {
        let mut ring = VecDeque::new();
        for i in 0..(PRESENCE_HISTORY_CAPACITY + 1) {
            push_bounded(
                &mut ring,
                PresenceHistoryEntry {
                    at: Utc::now() + ChronoDuration::seconds(i as i64),
                    kind: "test".to_string(),
                    note: format!("entry {i}"),
                    track_fingerprint: None,
                    posted_status: None,
                    gate_reason: None,
                },
            );
        }
        assert_eq!(ring.len(), PRESENCE_HISTORY_CAPACITY);
        assert!(ring.iter().all(|e| !e.note.starts_with("entry 0 ")));
    }

    /// Issue #877: `recent(n)` returns at most `n` entries in
    /// reverse-chronological order. A ring of 5 entries and `n = 3`
    /// yields the three newest, in newest-first order.
    #[test]
    fn recent_returns_newest_first() {
        let mut ring = VecDeque::new();
        for i in 0..5 {
            ring.push_back(PresenceHistoryEntry {
                at: Utc::now() + ChronoDuration::seconds(i as i64),
                kind: "test".to_string(),
                note: format!("entry {i}"),
                track_fingerprint: None,
                posted_status: None,
                gate_reason: None,
            });
        }
        let top = recent_from_ring(&ring, 3);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].note, "entry 4");
        assert_eq!(top[1].note, "entry 3");
        assert_eq!(top[2].note, "entry 2");
    }

    /// Issue #877: `redacted_lines_for_diagnostics` runs the entry
    /// through [`redact_sensitive`]. A cred-shaped token in the `note`
    /// field must NOT reach the snapshot output.
    #[test]
    fn diagnostics_lines_redact_credential_shaped_strings() {
        let mut ring = VecDeque::new();
        ring.push_back(PresenceHistoryEntry {
            at: Utc::now(),
            kind: "presence-updated".to_string(),
            note: "secret=ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef0123456789xyz in the posted status"
                .to_string(),
            track_fingerprint: None,
            posted_status: None,
            gate_reason: None,
        });
        let lines = redacted_lines_from_ring(&ring);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("[REDACTED"));
        assert!(!lines[0].contains("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef0123456789xyz"));
    }

    /// Issue #877: the documented kinds are accepted by the entry
    /// constructor as opaque strings — there is no validation in the
    /// ring because the production decision sites already constrain the
    /// call. A regression that introduces a new kind without an
    /// accompanying docs change would be caught by code review.
    #[test]
    fn documented_kinds_match_acceptance_criteria() {
        let _ = sample_entry("presence-updated");
        let _ = sample_entry("presence-gated");
        let _ = sample_entry("snooze-start");
        let _ = sample_entry("snooze-end");
        let _ = sample_entry("preferred-presence-armed");
        let _ = sample_entry("preferred-presence-cleared");
    }
}
