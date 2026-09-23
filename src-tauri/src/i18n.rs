//! Rust-side UI string table for the native surfaces (4.7.0, issue #674).
//!
//! The webview owns its own dictionaries (`src/lib/i18n/{en,de,fr}.ts`); this
//! module is the Rust counterpart for the literals the webview never renders:
//! the tray menu (`tray.rs`) and the native application menu (`menu.rs`).
//!
//! [`Strings`] carries one field per user-visible literal and the three tables
//! below must stay field-for-field identical. That is enforced, not assumed:
//! `tables_carry_an_identical_field_set` parses the struct declaration out of
//! this file and fails when a table misses a field or falls out of order, and
//! `no_user_visible_literal_stays_hard_coded` scans production literals in
//! `tray.rs`/`menu.rs` and fails when user-visible copy is not in these tables.
//!
//! Deliberate exceptions, mirroring the frontend's documented limitation:
//! error strings surfaced through `invoke()` rejections or event payloads
//! (e.g. `playback-error`) stay English, and the app name ("PresenceJam",
//! the tray's no-track tooltip fallback) is a product name, not copy.

use std::sync::atomic::{AtomicU8, Ordering};

/// Every user-visible literal of the tray menu and the native application
/// menu. Plain `&'static str` fields so the tables below stay `const` data
/// the parity test can read field by field.
pub struct Strings {
    // ── Tray menu ───────────────────────────────────────────────────────────
    /// Show/Hide item while the main window is hidden.
    pub show_window: &'static str,
    /// Show/Hide item while the main window is visible.
    pub hide_window: &'static str,
    /// Pause/Resume item while sync is running.
    pub pause_sync: &'static str,
    /// Pause/Resume item while sync is stopped.
    pub resume_sync: &'static str,
    pub open_settings: &'static str,
    pub open_logs_folder: &'static str,
    pub quit: &'static str,
    /// Play/Pause check item — the playing state is the native check mark.
    pub play_pause: &'static str,
    pub previous: &'static str,
    pub next: &'static str,
    /// Shuffle check item — the state is the native check mark.
    pub shuffle: &'static str,
    /// Repeat item labels: repeat has three states, so the mode is spelled out.
    pub repeat_off: &'static str,
    pub repeat_context: &'static str,
    pub repeat_track: &'static str,
    /// Devices submenu title.
    pub devices: &'static str,
    /// Disabled placeholder shown when no Spotify device is known.
    pub no_devices: &'static str,
    /// Up Next (queue) submenu title.
    pub up_next: &'static str,
    /// Disabled placeholder shown when the queue is empty.
    pub queue_empty: &'static str,
    // ── Tray status line and tooltip (issue #591) ───────────────────────────
    /// Leading status word: "Syncing — artist — track".
    pub status_syncing: &'static str,
    /// Leading status word: "Paused — artist — track".
    pub status_paused: &'static str,
    /// Whole status line while sync is stopped.
    pub status_not_syncing: &'static str,
    /// Whole status line while syncing with nothing playing.
    pub status_syncing_no_track: &'static str,
    // ── Native application menu ─────────────────────────────────────────────
    pub menu_file: &'static str,
    pub menu_edit: &'static str,
    pub menu_view: &'static str,
    pub menu_help: &'static str,
    pub menu_settings: &'static str,
    pub menu_quit: &'static str,
    pub menu_show_dashboard: &'static str,
    pub menu_show_logs: &'static str,
    pub menu_about: &'static str,
    // ── Snooze submenu (4.7.0, S9 / issue #677) ────────────────────────────
    /// Title of the tray's "pause sync for a while" submenu.
    pub snooze_pause_menu: &'static str,
    pub snooze_30_minutes: &'static str,
    pub snooze_1_hour: &'static str,
    pub snooze_until_tomorrow: &'static str,
    /// Issue #867: only present while a busy Outlook meeting is in
    /// progress. The entry reads the calendar cache at click time; the
    /// label is the literal copy the tray shows.
    pub snooze_until_next_meeting_ends: &'static str,
    /// Only present while a snooze is active.
    pub snooze_resume_now: &'static str,
    /// Leading status word while snoozed: "Snoozed — 29 min left (→ 14:32)".
    pub snooze_paused: &'static str,
    /// Countdown unit, composed as `{word} — {n} {unit} (→ HH:MM)`.
    pub snooze_minutes_left: &'static str,
    // ── Manual-status submenu (issue #870) ───────────────────────────────────
    /// Title of the tray's "Recent statuses" submenu.
    pub manual_status_recent_menu: &'static str,
    /// Disabled placeholder shown when the recent ring is empty.
    pub manual_status_recent_empty: &'static str,
    /// "Clear manual status" entry, only present while a manual status is armed.
    pub manual_status_clear: &'static str,
    // ── Volume / Seek submenus (issue #871) ─────────────────────────────────
    /// Title of the tray's "Volume" submenu (issue #871). The entries
    /// themselves use the documented Spotify percentage (`{percent}` is
    /// substituted at build time).
    pub volume_menu: &'static str,
    /// Per-entry label of the Volume submenu (issue #871). The
    /// `{percent}` placeholder is replaced with the literal Spotify
    /// percentage at build time (0 / 25 / 50 / 75 / 100).
    pub volume_percent_label: &'static str,
    /// Title of the tray's "Seek" submenu (issue #871).
    pub seek_menu: &'static str,
    /// Label of the "seek back N seconds" entry (issue #871). The
    /// `{seconds}` placeholder is replaced with the literal second
    /// count (30 by default).
    pub seek_back_30s_label: &'static str,
    /// Label of the "seek forward N seconds" entry (issue #871).
    pub seek_forward_30s_label: &'static str,
    // ── Profile submenu (issue #869) ──────────────────────────────────────────
    /// Title of the tray's "Active profile" submenu (issue #869). The
    /// first entry is "Base configuration" (no profile); the rest are the
    /// configured profile names in their stored order. `clamp_presence_profiles`
    /// guarantees names are unique and ≤ 32 characters, so the submenu cannot
    /// have colliding labels.
    pub profile_menu: &'static str,
    /// Sentinel entry that clears the active profile. The contract is the
    /// same as the CLI's `--profile base` — switching back to "Base
    /// configuration" never rewrites the on-disk base values, the
    /// `effective_config` overlay just resolves to a no-op.
    pub profile_base: &'static str,
    /// Disabled placeholder shown when the profile list is empty.
    pub profile_empty: &'static str,
}

/// English table — the source of truth the other two mirror.
pub const EN: Strings = Strings {
    show_window: "Show Window",
    hide_window: "Hide Window",
    pause_sync: "Pause Sync",
    resume_sync: "Resume Sync",
    open_settings: "Open Settings",
    open_logs_folder: "Open Logs Folder",
    quit: "Quit",
    play_pause: "Play/Pause",
    previous: "Previous",
    next: "Next",
    shuffle: "Shuffle",
    repeat_off: "Repeat: Off",
    repeat_context: "Repeat: Context",
    repeat_track: "Repeat: Track",
    devices: "Devices",
    no_devices: "(no devices)",
    up_next: "Up Next",
    queue_empty: "(queue empty)",
    status_syncing: "Syncing",
    status_paused: "Paused",
    status_not_syncing: "Not syncing",
    status_syncing_no_track: "Syncing — no track",
    menu_file: "File",
    menu_edit: "Edit",
    menu_view: "View",
    menu_help: "Help",
    menu_settings: "Settings...",
    menu_quit: "Quit PresenceJam",
    menu_show_dashboard: "Show Dashboard",
    menu_show_logs: "Show Logs",
    menu_about: "About PresenceJam",
    snooze_pause_menu: "Pause sync for…",
    snooze_30_minutes: "30 minutes",
    snooze_1_hour: "1 hour",
    snooze_until_tomorrow: "Until tomorrow",
    snooze_until_next_meeting_ends: "Until this meeting ends",
    snooze_resume_now: "Resume sync now",
    snooze_paused: "Snoozed",
    snooze_minutes_left: "min left",
    manual_status_recent_menu: "Recent statuses",
    manual_status_recent_empty: "(no recent statuses)",
    manual_status_clear: "Clear manual status",
    volume_menu: "Volume",
    volume_percent_label: "{percent}%",
    seek_menu: "Seek",
    seek_back_30s_label: "Back {seconds} s",
    seek_forward_30s_label: "Forward {seconds} s",
    // ── Profile submenu (issue #869) ──────────────────────────────────────────
    profile_menu: "Active profile",
    profile_base: "Base configuration",
    profile_empty: "(no profiles configured)",
};

/// German table.
pub const DE: Strings = Strings {
    show_window: "Fenster anzeigen",
    hide_window: "Fenster ausblenden",
    pause_sync: "Synchronisierung pausieren",
    resume_sync: "Synchronisierung fortsetzen",
    open_settings: "Einstellungen öffnen",
    open_logs_folder: "Protokollordner öffnen",
    quit: "Beenden",
    play_pause: "Wiedergabe/Pause",
    previous: "Zurück",
    next: "Weiter",
    shuffle: "Zufallswiedergabe",
    repeat_off: "Wiederholen: Aus",
    repeat_context: "Wiederholen: Kontext",
    repeat_track: "Wiederholen: Titel",
    devices: "Geräte",
    no_devices: "(keine Geräte)",
    up_next: "Als Nächstes",
    queue_empty: "(Warteschlange leer)",
    status_syncing: "Synchronisiert",
    status_paused: "Pausiert",
    status_not_syncing: "Nicht synchronisiert",
    status_syncing_no_track: "Synchronisiert — kein Titel",
    menu_file: "Datei",
    menu_edit: "Bearbeiten",
    menu_view: "Ansicht",
    menu_help: "Hilfe",
    menu_settings: "Einstellungen...",
    menu_quit: "PresenceJam beenden",
    menu_show_dashboard: "Dashboard anzeigen",
    menu_show_logs: "Protokolle anzeigen",
    menu_about: "Über PresenceJam",
    snooze_pause_menu: "Sync pausieren für…",
    snooze_30_minutes: "30 Minuten",
    snooze_1_hour: "1 Stunde",
    snooze_until_tomorrow: "Bis morgen",
    snooze_until_next_meeting_ends: "Bis zum Ende dieses Termins",
    snooze_resume_now: "Sync jetzt fortsetzen",
    snooze_paused: "Sync pausiert",
    snooze_minutes_left: "Min. verbleibend",
    manual_status_recent_menu: "Letzte Status",
    manual_status_recent_empty: "(keine letzten Status)",
    manual_status_clear: "Status löschen",
    volume_menu: "Lautstärke",
    volume_percent_label: "{percent} %",
    seek_menu: "Spulen",
    seek_back_30s_label: "{seconds} s zurück",
    seek_forward_30s_label: "{seconds} s vor",
    // ── Profile submenu (issue #869) ──────────────────────────────────────────
    profile_menu: "Aktives Profil",
    profile_base: "Basiskonfiguration",
    profile_empty: "(keine Profile definiert)",
};

/// French table.
pub const FR: Strings = Strings {
    show_window: "Afficher la fenêtre",
    hide_window: "Masquer la fenêtre",
    pause_sync: "Suspendre la synchro",
    resume_sync: "Reprendre la synchro",
    open_settings: "Ouvrir les paramètres",
    open_logs_folder: "Ouvrir le dossier des journaux",
    quit: "Quitter",
    play_pause: "Lecture/Pause",
    previous: "Précédent",
    next: "Suivant",
    shuffle: "Lecture aléatoire",
    repeat_off: "Répéter : désactivé",
    repeat_context: "Répéter : contexte",
    repeat_track: "Répéter : titre",
    devices: "Appareils",
    no_devices: "(aucun appareil)",
    up_next: "À suivre",
    queue_empty: "(file vide)",
    status_syncing: "Synchronisation",
    status_paused: "En pause",
    status_not_syncing: "Non synchronisé",
    status_syncing_no_track: "Synchronisation — aucun titre",
    menu_file: "Fichier",
    menu_edit: "Édition",
    menu_view: "Affichage",
    menu_help: "Aide",
    menu_settings: "Paramètres...",
    menu_quit: "Quitter PresenceJam",
    menu_show_dashboard: "Afficher le tableau de bord",
    menu_show_logs: "Afficher les journaux",
    menu_about: "À propos de PresenceJam",
    snooze_pause_menu: "Suspendre la synchro pour…",
    snooze_30_minutes: "Pendant 30 minutes",
    snooze_1_hour: "Pendant 1 heure",
    snooze_until_tomorrow: "Jusqu’à demain",
    snooze_until_next_meeting_ends: "Jusqu’à la fin de cette réunion",
    snooze_resume_now: "Reprendre la synchro maintenant",
    snooze_paused: "Synchro en pause",
    snooze_minutes_left: "min restant",
    manual_status_recent_menu: "Statuts récents",
    manual_status_recent_empty: "(aucun statut récent)",
    manual_status_clear: "Effacer le statut manuel",
    volume_menu: "Volume sonore",
    volume_percent_label: "{percent} %",
    seek_menu: "Position",
    seek_back_30s_label: "Reculer de {seconds} s",
    seek_forward_30s_label: "Avancer de {seconds} s",
    // ── Profile submenu (issue #869) ──────────────────────────────────────────
    profile_menu: "Profil actif",
    profile_base: "Configuration de base",
    profile_empty: "(aucun profil configuré)",
};

/// Canonical locale tags, in table order. The value persisted in
/// `AppConfig::locale` is always one of these.
pub const LOCALES: [&str; 3] = ["en", "de", "fr"];

/// Lowercased base language subtag: `"DE_at"`/`"de-AT"` → `"de"`.
fn base_tag(locale: &str) -> String {
    locale
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

/// Canonical tag for a stored `AppConfig::locale`.
///
/// `None` is the documented pre-4.7 state and an empty string is what a
/// hand-edited config may carry — both mean English without a warning. A
/// *unknown* tag (`"zz"`, `"pt-BR"`) also falls back to English but logs the
/// fallback, so a typo'd locale is visible in the log instead of silent.
pub fn resolve_tag(locale: Option<&str>) -> &'static str {
    let Some(raw) = locale.map(str::trim).filter(|value| !value.is_empty()) else {
        return "en";
    };
    match base_tag(raw).as_str() {
        "en" => "en",
        "de" => "de",
        "fr" => "fr",
        unknown => {
            log::warn!("[I18N] unknown locale '{}' — falling back to 'en'", unknown);
            "en"
        }
    }
}

/// The table for a canonical tag. Anything that is not `"de"`/`"fr"` reads as
/// English, so a caller can never render a half-translated menu.
pub fn strings_for(tag: &str) -> &'static Strings {
    match tag {
        "de" => &DE,
        "fr" => &FR,
        _ => &EN,
    }
}

/// Index into [`LOCALES`] of the locale the native surfaces render in.
/// `AppConfig::locale` remains the source of truth; this mirrors the resolved
/// tag so the menu builders on paths without config access stay cheap.
static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Installs `locale` process-wide and returns the canonical tag it resolved
/// to. Callers persist that tag, so the stored value and the rendered tables
/// cannot disagree.
pub fn set_current(locale: Option<&str>) -> &'static str {
    let tag = resolve_tag(locale);
    let index = LOCALES.iter().position(|known| *known == tag).unwrap_or(0) as u8;
    CURRENT.store(index, Ordering::Relaxed);
    tag
}

/// The installed table — what every tray/menu build renders from.
pub fn current() -> &'static Strings {
    let index = CURRENT.load(Ordering::Relaxed) as usize % LOCALES.len();
    strings_for(LOCALES[index])
}

/// Installs the locale stored in the mounted [`crate::AppState`] config.
/// A missing config (before the startup load) or a pre-4.7 file keeps English.
/// Idempotent, so the tray and app-menu setup can both call it.
pub fn install_from_app_state(state: &crate::AppState) -> &'static str {
    let locale = state
        .config
        .get()
        .as_ref()
        .and_then(|cfg| cfg.locale.clone());
    set_current(locale.as_deref())
}

/// Installs the locale of a just-persisted config and reports whether that
/// changed the installed table.
///
/// `true` means the native surfaces render different labels from now on and
/// must be repainted; `false` means the config write did not touch the language
/// (the common case for every other field), so no rebuild is warranted.
///
/// Every config write path converges through this — the `set_locale` command
/// and `after_persist`, which the generic save/update/import paths share — so a
/// locale arriving through an imported config cannot leave the tray and the app
/// menu in the previous language (4.7.0, issue #674).
pub fn install_from_config(cfg: &crate::config::AppConfig) -> bool {
    let previous = CURRENT.load(Ordering::Relaxed);
    set_current(cfg.locale.as_deref());
    CURRENT.load(Ordering::Relaxed) != previous
}

/// Serialises the tests that install a process-wide table, so a test asserting
/// `current()` cannot observe another test's locale.
#[cfg(test)]
pub(crate) static LOCALE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
impl Strings {
    /// `(field name, value)` pairs in declaration order. Used by the parity
    /// test so the tables are compared as data, not only by their type.
    fn values(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("show_window", self.show_window),
            ("hide_window", self.hide_window),
            ("pause_sync", self.pause_sync),
            ("resume_sync", self.resume_sync),
            ("open_settings", self.open_settings),
            ("open_logs_folder", self.open_logs_folder),
            ("quit", self.quit),
            ("play_pause", self.play_pause),
            ("previous", self.previous),
            ("next", self.next),
            ("shuffle", self.shuffle),
            ("repeat_off", self.repeat_off),
            ("repeat_context", self.repeat_context),
            ("repeat_track", self.repeat_track),
            ("devices", self.devices),
            ("no_devices", self.no_devices),
            ("up_next", self.up_next),
            ("queue_empty", self.queue_empty),
            ("status_syncing", self.status_syncing),
            ("status_paused", self.status_paused),
            ("status_not_syncing", self.status_not_syncing),
            ("status_syncing_no_track", self.status_syncing_no_track),
            ("menu_file", self.menu_file),
            ("menu_edit", self.menu_edit),
            ("menu_view", self.menu_view),
            ("menu_help", self.menu_help),
            ("menu_settings", self.menu_settings),
            ("menu_quit", self.menu_quit),
            ("menu_show_dashboard", self.menu_show_dashboard),
            ("menu_show_logs", self.menu_show_logs),
            ("menu_about", self.menu_about),
            ("snooze_pause_menu", self.snooze_pause_menu),
            ("snooze_30_minutes", self.snooze_30_minutes),
            ("snooze_1_hour", self.snooze_1_hour),
            ("snooze_until_tomorrow", self.snooze_until_tomorrow),
            (
                "snooze_until_next_meeting_ends",
                self.snooze_until_next_meeting_ends,
            ),
            ("snooze_resume_now", self.snooze_resume_now),
            ("snooze_paused", self.snooze_paused),
            ("snooze_minutes_left", self.snooze_minutes_left),
            ("manual_status_recent_menu", self.manual_status_recent_menu),
            (
                "manual_status_recent_empty",
                self.manual_status_recent_empty,
            ),
            ("manual_status_clear", self.manual_status_clear),
            ("volume_menu", self.volume_menu),
            ("volume_percent_label", self.volume_percent_label),
            ("seek_menu", self.seek_menu),
            ("seek_back_30s_label", self.seek_back_30s_label),
            ("seek_forward_30s_label", self.seek_forward_30s_label),
            ("profile_menu", self.profile_menu),
            ("profile_base", self.profile_base),
            ("profile_empty", self.profile_empty),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Field names declared by `pub struct Strings` in this file, in order.
    /// Parsed from source so a field added without a value in the tables
    /// cannot slip through.
    fn declared_field_names(src: &str) -> Vec<String> {
        let header = "pub struct Strings {";
        let start = src
            .find(header)
            .unwrap_or_else(|| panic!("i18n.rs must declare `{}`", header));
        let body = &src[start + header.len()..];
        let end = body
            .find("\n}")
            .unwrap_or_else(|| panic!("`{}` is never closed", header));
        body[..end]
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("pub "))
            .filter_map(|line| line.trim_start_matches("pub ").split(':').next())
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect()
    }

    /// Production half of a sibling module — everything before its inline test
    /// module, so a scan can never match the assertions themselves.
    fn prod_source(src: &str) -> &str {
        src.split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_else(|| panic!("module has no #[cfg(test)] mod tests block"))
    }

    /// Drops `//` line comments (doc comments included) so prose that quotes a
    /// literal cannot trip the hard-coded-literal scan. Brace counting is not
    /// involved here, so truncating a line inside a `//` is harmless.
    fn strip_line_comments(src: &str) -> String {
        src.lines()
            .map(|line| match line.find("//") {
                Some(i) => &line[..i],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Returns every double-quoted literal in `src`, including escaped quotes.
    /// Byte-wise scanning is safe here because only ASCII quote/backslash bytes
    /// control the state and slice boundaries always follow those bytes.
    fn double_quoted_literals(src: &str) -> Vec<&str> {
        let bytes = src.as_bytes();
        let mut literals = Vec::new();
        let mut cursor = 0;
        while cursor < bytes.len() {
            if bytes[cursor] != b'"' {
                cursor += 1;
                continue;
            }
            let start = cursor + 1;
            cursor = start;
            while cursor < bytes.len() {
                match bytes[cursor] {
                    b'\\' => cursor = (cursor + 2).min(bytes.len()),
                    b'"' => break,
                    _ => cursor += 1,
                }
            }
            if cursor < bytes.len() {
                literals.push(&src[start..cursor]);
                cursor += 1;
            }
        }
        literals
    }

    /// Deliberate non-copy literals in the native modules. These are stable
    /// menu/window/event ids, action and log context, documented English
    /// backend errors, the product name, accelerators, or format-only pieces;
    /// none is rendered as tray/menu copy. A new entry here needs that same
    /// user-visible-exception justification.
    const NATIVE_LITERAL_ALLOWLIST: &[&str] = &[
        "show_hide_window",
        "pause_sync",
        "resume_sync",
        "current_track",
        "sync_status",
        "play_pause",
        "previous",
        "next",
        "shuffle",
        "repeat",
        "devices",
        "queue",
        "devices|",
        "snooze|30m",
        "snooze|1h",
        "snooze|tomorrow",
        "snooze|next_meeting",
        "snooze|resume",
        "snooze|",
        "tomorrow",
        "next_meeting",
        "resume",
        "manualstatus|",
        "manualstatus|clear",
        "volume|",
        "seek|",
        "profile|base",
        "profile|",
        "settings",
        "open_logs",
        "quit",
        "show_dashboard",
        "show_logs",
        "about",
        "main",
        "empty",
        "none",
        "transfer",
        "pause",
        "play",
        "snooze",
        "profile",
        "volume",
        "seek",
        "dashboard",
        "logs",
        "macos",
        "linux",
        "toggle-pause",
        "navigate",
        "open-logs-folder",
        "play/pause state",
        "playback-error",
        "tray-click",
        "playback-state-changed",
        "app-shutdown",
        "show-about",
        "pause/resume",
        "profile (empty placeholder)",
        "manual status clear",
        "manual status pick (stale)",
        "manual status pick",
        "volume (stale)",
        "seek (stale)",
        "seek (no track)",
        "transfer device list",
        "tray icon",
        "refresh_tray_from_state",
        "<id len={}>",
        "<legacy index={}>",
        "<invalid>",
        "{}|none",
        "{}|none|{}",
        "{PROFILE_ITEM_PREFIX}empty",
        "{MANUAL_STATUS_ITEM_PREFIX}{idx}",
        "{MANUAL_STATUS_ITEM_PREFIX}none",
        "{VOLUME_ITEM_PREFIX}{percent}",
        "CmdOrCtrl+,",
        "CmdOrCtrl+Shift+L",
        "CmdOrCtrl+Q",
        "CmdOrCtrl+1",
        "CmdOrCtrl+2",
        "PresenceJam",
        "Tray already initialized",
        "No default icon",
        "unknown panic",
        "Tray not initialized",
        "No active playback device - pick one from the tray Devices menu",
        "Failed to set tray menu: {}",
        "{} unavailable: {}",
        "main window not found",
        "Failed to set window menu: {}",
    ];

    /// True for copy-like literals: either whitespace makes them a phrase, or
    /// three ASCII letters catch single-word native labels such as "Quit".
    fn looks_like_native_copy(value: &str) -> bool {
        value.contains(' ') || value.bytes().filter(u8::is_ascii_alphabetic).count() >= 3
    }

    /// True for format-only decorations. Once `{...}` placeholders are
    /// removed, no ASCII letters remain (for example `🎵 {} - {}` or `✓ {}`).
    fn is_format_only_literal(value: &str) -> bool {
        let mut rest = value;
        while let Some(start) = rest.find('{') {
            let after = &rest[start..];
            let Some(end) = after.find('}') else {
                break;
            };
            rest = &after[end + 1..];
            rest = rest.trim_start_matches('{');
        }
        !rest.bytes().any(|byte| byte.is_ascii_alphabetic())
    }

    /// Scans already-isolated production source and returns copy-like literals
    /// absent from all locale tables and the explicit native exception list.
    fn native_literal_offenders(src: &str, translated: &HashSet<&'static str>) -> Vec<String> {
        let mut offenders: Vec<String> = double_quoted_literals(&strip_line_comments(src))
            .into_iter()
            .filter(|value| looks_like_native_copy(value))
            .filter(|value| !translated.contains(*value))
            .filter(|value| !NATIVE_LITERAL_ALLOWLIST.contains(value))
            .filter(|value| !value.starts_with("[TRAY]") && !value.starts_with("[MENU]"))
            .filter(|value| !is_format_only_literal(value))
            .map(|value| format!("{:?}", value))
            .collect();
        offenders.sort_unstable();
        offenders.dedup();
        offenders
    }

    /// Mirrors the frontend's `Dict` parity test: the three tables describe
    /// exactly the fields `Strings` declares, in the same order.
    #[test]
    fn tables_carry_an_identical_field_set() {
        let declared = declared_field_names(include_str!("i18n.rs"));
        assert!(
            declared.len() > 20,
            "the parser must find the whole struct, found {:?}",
            declared
        );
        for (tag, table) in [("en", &EN), ("de", &DE), ("fr", &FR)] {
            let names: Vec<String> = table
                .values()
                .iter()
                .map(|(name, _)| (*name).to_string())
                .collect();
            assert_eq!(
                names, declared,
                "the `{}` table must cover every `Strings` field in declaration order",
                tag
            );
        }
    }

    /// A table that is a copy of English would render a German UI as English.
    #[test]
    fn de_and_fr_translate_every_field() {
        let en = EN.values();
        for (tag, table) in [("de", &DE), ("fr", &FR)] {
            for ((name, english), (_, translated)) in en.iter().zip(table.values()) {
                let english: &'static str = english;
                let name: &'static str = name;
                assert_ne!(
                    translated, english,
                    "the `{}` table still carries the English copy for `{}`",
                    tag, name
                );
                assert!(
                    !translated.trim().is_empty(),
                    "the `{}` table is empty for `{}`",
                    tag,
                    name
                );
            }
        }
    }

    /// Issue #674: a locale the binary does not know must render English and
    /// say so, and the tag a caller persists must be one of `LOCALES`.
    #[test]
    fn unknown_locale_falls_back_to_english() {
        assert_eq!(resolve_tag(None), "en", "the pre-4.7 default is English");
        assert_eq!(resolve_tag(Some("")), "en", "a blank tag is not a locale");
        assert_eq!(resolve_tag(Some("   ")), "en");
        assert_eq!(resolve_tag(Some("zz")), "en", "an unknown tag falls back");
        assert_eq!(resolve_tag(Some("pt-BR")), "en");
        assert_eq!(resolve_tag(Some("en-GB")), "en");
        assert_eq!(
            resolve_tag(Some("DE-at")),
            "de",
            "case and region are folded"
        );
        assert_eq!(resolve_tag(Some("fr_CA")), "fr");
        assert!(LOCALES.contains(&resolve_tag(Some("zz"))));
        assert_eq!(
            strings_for("zz").show_window,
            EN.show_window,
            "an uncanonical tag must never render a half-translated menu"
        );
    }

    /// The tray/menu builders read [`current`], which is installed from the
    /// mounted config — this is the startup path in one test.
    #[test]
    fn install_from_app_state_selects_the_table() {
        let _serialised = LOCALE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let state = crate::AppState::new();
        assert_eq!(
            install_from_app_state(&state),
            "en",
            "before the config load there is no locale, so English"
        );
        assert_eq!(current().show_window, EN.show_window);

        {
            let mut guard = state.config.get_mut();
            *guard = Some(crate::config::AppConfig {
                locale: Some("de".to_string()),
                ..Default::default()
            });
        }
        assert_eq!(install_from_app_state(&state), "de");
        assert_eq!(
            current().show_window,
            DE.show_window,
            "the installed locale must reach the builders through `current()`"
        );

        {
            let mut guard = state.config.get_mut();
            *guard = Some(crate::config::AppConfig {
                locale: Some("zz".to_string()),
                ..Default::default()
            });
        }
        assert_eq!(install_from_app_state(&state), "en");
        assert_eq!(current().show_window, EN.show_window);

        // Leave the process-wide table on English for the other tests.
        set_current(None);
    }

    /// Every config write path converges on this helper (4.7.0, issue #674):
    /// it installs the persisted locale and tells the caller whether the
    /// visible surfaces actually need relabelling.
    #[test]
    fn install_from_config_reports_only_real_locale_changes() {
        let _serialised = LOCALE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let cfg = |locale: Option<&str>| crate::config::AppConfig {
            locale: locale.map(str::to_string),
            ..Default::default()
        };

        assert!(
            install_from_config(&cfg(Some("de"))),
            "switching to a new locale must ask for a repaint"
        );
        assert_eq!(current().show_window, DE.show_window);
        assert!(
            !install_from_config(&cfg(Some("de"))),
            "the same locale again must not rebuild the menus"
        );
        assert!(install_from_config(&cfg(Some("fr"))));
        assert!(
            install_from_config(&cfg(None)),
            "clearing the field is a change back to the English default"
        );
        assert!(
            !install_from_config(&cfg(None)),
            "the English default is stable"
        );
        assert_eq!(current().show_window, EN.show_window);
    }

    /// Issue #843: scan forward, not only for table values reappearing in the
    /// two modules that build native surfaces. A new English-only label must
    /// enter all three tables before it can live there.
    #[test]
    fn no_user_visible_literal_stays_hard_coded() {
        let translated: HashSet<&'static str> = [&EN, &DE, &FR]
            .into_iter()
            .flat_map(Strings::values)
            .map(|(_, value)| value)
            .collect();
        let mut offenders = Vec::new();
        for (module, src) in [
            ("tray.rs", include_str!("tray.rs")),
            ("menu.rs", include_str!("menu.rs")),
        ] {
            offenders.extend(
                native_literal_offenders(prod_source(src), &translated)
                    .into_iter()
                    .map(|literal| format!("{} hard-codes {}", module, literal)),
            );
        }
        assert!(
            offenders.is_empty(),
            "user-visible literals must come from the i18n tables: {:?}",
            offenders
        );
    }

    /// Proves the scanner rejects unknown copy before a table can contain it,
    /// while the same native call shape with a table-backed label is accepted.
    #[test]
    fn native_literal_scanner_rejects_unknown_copy_and_accepts_table_copy() {
        let source = r#"
            MenuItemBuilder::with_id(ID_NEW_ACTION, "Brand new action");
            MenuItemBuilder::with_id(ID_KNOWN_ACTION, "Next");
        "#;
        let translated: HashSet<&'static str> = [EN.next].into_iter().collect();

        assert_eq!(
            native_literal_offenders(source, &translated),
            vec![r#""Brand new action""#]
        );
    }
}
