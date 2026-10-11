## VERDICTS

USAGE-C001 | CONFIRMED | USAGE.md:3 | Day-to-day guide claim matches the file's own subtitle. | P3
USAGE-C002 | CONFIRMED | USAGE.md:7; src-tauri/src/app.rs:1124-1144 (tray built in setup) | Tray icon is built unconditionally on desktop, so "lives in your system tray" is accurate. | P3
USAGE-C003 | DRIFT | src-tauri/tauri.conf.json:24 ("visible": true) + app.rs:558-582 | "The app window is hidden by default to keep your taskbar clean" is not true out of the box: the main window is declared `"visible": true` in tauri.conf.json and is hidden at launch only when `teams.start_minimized` or the autostart plugin's `--minimized` is set. | P1
USAGE-C004 | CONFIRMED | tray/mod.rs:784-793 (left click emits `tray-click`); +page.svelte:170-179 | Left-click opens/focuses the window through the `show_window` command. | P3
USAGE-C005 | CONFIRMED | tray/mod.rs:778 (on_menu_event) | Right-click opens the tray menu. | P3
USAGE-C006 | CONFIRMED | docs/screenshots/tray-menu.png (file exists) | Referenced screenshot exists on disk. | P3
USAGE-C007 | CONFIRMED | tray/actions.rs / i18n.rs:139 "Show Window" | Menu item exists and brings the window to the foreground. | P3
USAGE-C008 | CONFIRMED | tray/mod.rs:298-319; polling/loop.rs:180-260 | Pause/Resume Sync stops polling while the Teams status is left untouched. | P3
USAGE-C009 | CONFIRMED | tray/mod.rs:298-319 | Resume restarts the poller after a pause. | P3
USAGE-C010 | DRIFT | config/snooze.rs:58-93; i18n.rs:84-92 | The doc lists three snooze presets (30 minutes, 1 hour, until tomorrow), but `SnoozePreset` has a fourth — `UntilNextMeetingEnds` ("Until this meeting ends", issue #867) — which the tray submenu also offers, so the enumeration is incomplete. | P2
USAGE-C011 | CONFIRMED | polling/loop.rs:180-260; gate.rs SnoozeGate::Skipped | While snoozed the loop performs no Spotify or Graph request and leaves the status as-is. | P2
USAGE-C012 | CONFIRMED | tray/snooze.rs:102-110 (resume entry only when snooze active) | "Resume sync now" appears only while a snooze is active and ends it. | P3
USAGE-C013 | CONFIRMED | Dashboard.svelte:1139-1158 (snooze chip + resume button) | The Dashboard snooze chip carries the same resume button. | P3
USAGE-C014 | CONFIRMED | tray/mod.rs:350-396 | Play/Pause toggles playback on the active device. | P3
USAGE-C015 | CONFIRMED | tray/mod.rs:398-406 | Previous skips to the previous track. | P3
USAGE-C016 | CONFIRMED | tray/mod.rs:408-416 | Next skips to the next track. | P3
USAGE-C017 | CONFIRMED | spotify.rs:1517-1530 PUT /me/player/shuffle | Shuffle toggles Spotify shuffle. | P3
USAGE-C018 | CONFIRMED | tray/mod.rs:1216-1219 (checked from shuffle_flag) | The check mark mirrors the state Spotify reports. | P3
USAGE-C019 | CONFIRMED | spotify.rs RepeatState::next() off→context→track→off; i18n.rs:150-152 | Repeat cycles Off → Context → Track → back to off. | P3
USAGE-C020 | CONFIRMED | tray/mod.rs:1227-1234 (label spells the mode out) | The label always names the current mode rather than relying on a check mark. | P3
USAGE-C021 | CONFIRMED | tray/mod.rs:714-740 (transfer with play=true) | Picking a device transfers playback there and starts it. | P3
USAGE-C022 | CONFIRMED | tray/mod.rs:330-344 (emits `navigate` "settings") | Open Settings jumps straight to the Settings view. | P3
USAGE-C023 | CONFIRMED | tray/mod.rs:345-347 → +page.svelte:201-208 | Open Logs Folder opens the log directory in the OS file manager. | P3
USAGE-C024 | CONFIRMED | tray/devices.rs:158-179 (`.take(3)`) | Up Next peeks at the queue, capped at 3 rows. | P3
USAGE-C025 | CONFIRMED | tray/devices.rs:177 ("{artist} - {title}") | Queue rows read artist - title, so an episode shows as Show - Episode. | P3
USAGE-C026 | CONFIRMED | tray/mod.rs:320-327 (graceful shutdown) | Quit fully exits the app. | P3
USAGE-C027 | CONFIRMED | spotify.rs:605-608 NotPremium; commands/playback.rs:215-217 | Playback controls require a Premium account and the app surfaces a specific error otherwise. | P2
USAGE-C028 | CONFIRMED | commands/spotify_auth.rs:150 (scope list) | `user-modify-playback-state` is one of the three Spotify scopes requested. | P2
USAGE-C029 | CONFIRMED | SETUP.md:156 "## Upgrading from 2.x" exists; link resolves | The SETUP.md cross-reference target exists. | P3
USAGE-C030 | DRIFT | SpotifyCard.svelte:178-180 + i18n/en.ts:133 | The banner exists, but its text is "Spotify added playback controls. Click Reconnect next to this message to enable them.", not the quoted "Playback control needs a one-time reconnect". | P3
USAGE-C031 | CONFIRMED | spotify.rs:601-604 NoActiveDevice; commands/playback.rs:215 | Shuffle and Repeat need an active device and fail without one. | P2
USAGE-C032 | CONFIRMED | commands/playback.rs:215; tray/actions.rs:112-121 | The exact in-app toast string matches. | P2
USAGE-C033 | CONFIRMED | spotify.rs:605-608; commands/playback.rs:217 | Non-Premium accounts fail with the quoted message. | P2
USAGE-C034 | CONFIRMED | tray/mod.rs:418-455 (record-on-success discipline) | A failed toggle leaves the check mark untouched. | P2
USAGE-C035 | CONFIRMED | app.rs:1236-1251 (CloseRequested hides, prevent_close) | Closing the window hides to the tray rather than quitting. | P2
USAGE-C036 | CONFIRMED | polling/loop.rs (thread independent of window) | Sync keeps running in the background by design. | P3
USAGE-C037 | CONFIRMED | tray/mod.rs:320-327 | Tray Quit fully exits. | P3
USAGE-C038 | CONFIRMED | docs/screenshots/dashboard.png exists | Referenced screenshot exists. | P3
USAGE-C039 | CONFIRMED | Dashboard.svelte | The Dashboard is the main screen showing sync status. | P3
USAGE-C040 | CONFIRMED | Dashboard.svelte:988-993; commands/sync.rs:736 | Green badge = signed in with stored credentials. | P3
USAGE-C041 | CONFIRMED | commands/sync.rs:703,736-737 | Red badge = no stored session / not connected. | P3
USAGE-C042 | CONFIRMED | Dashboard.svelte:1075-1078 | Icon-only button with `dashboard.pauseSync` / `dashboard.resumeSync` tooltip + aria-label. | P2
USAGE-C043 | CONFIRMED | Dashboard.svelte:705-730 (start_syncing) | The play button starts polling and Teams status updates. | P3
USAGE-C044 | CONFIRMED | Dashboard.svelte:705-730 (stop_syncing) | Pause stops polling and leaves the Teams status unchanged. | P2
USAGE-C045 | CONFIRMED | Dashboard.svelte:1195-1205 | Card shows artist, title and album art. | P3
USAGE-C046 | CONFIRMED | spotify.rs:1041-1055 (episode → show name in artist slot, publisher in album slot) | Episode renders show where the artist goes and publisher where the album goes. | P2
USAGE-C047 | CONFIRMED | Dashboard.svelte dashboardHydration + polling events | The card updates in real time as tracks change. | P3
USAGE-C048 | CONFIRMED | spotify.rs:1181-1185 (ad → Ok(None)); Dashboard.svelte:1213 paused indicator | Adverts and nothing-playing show the paused indicator and are never treated as listening. | P2
USAGE-C049 | CONFIRMED | Dashboard.svelte:43-66 (gatedReasonLabel) + render | A suppressed write shows a reason chip. | P2
USAGE-C050 | CONFIRMED | Dashboard.svelte:45-53; i18n/en.ts:470-474 | Reason-specific copy exists for quiet hours, track rule, manual status and out of office. | P2
USAGE-C051 | DRIFT | i18n/en.ts:66 presenceGated default "you're busy, in a call, or presenting" | The generic fallback line exists, but `presenting`, `quiet-time` and `idle` each map to their own reason-specific copy in `gatedReasonLabel`, so the doc overstates how much falls back to the generic line. | P2
USAGE-C052 | CONFIRMED | Dashboard.svelte:30-34 (#670 comment) | The reason lives in the shared presence store, so it survives a view switch. | P2
USAGE-C053 | CONFIRMED | Dashboard.svelte:1139-1158; i18n/en.ts:539 snoozeChip | Countdown chip shows "Snoozed — N left (until HH:MM)" with a Resume now button. | P2
USAGE-C054 | CONFIRMED | Dashboard.svelte:1139-1158 | A tray snooze is visible and cancellable in the window. | P2
USAGE-C055 | CONFIRMED | tray/mod.rs:1437 (tooltip = status_line · track); snooze.rs:431-443 | The tray tooltip leads with the status line, which while snoozed states the remaining time. | P2
USAGE-C056 | DRIFT | Settings.svelte:745; i18n/en.ts:115 "Unsaved changes" | The banner appears as described, but its label is the short "Unsaved changes", not the quoted "You have unsaved changes". | P3
USAGE-C057 | DRIFT | SettingsCard.svelte resetLabel; i18n/en.ts:37 "Reset to default" | The per-section reset control exists and restores shipped defaults, but it is labelled "Reset to default", not "Reset". | P3
USAGE-C058 | CONFIRMED | config/clamp.rs clamp_polling; PollingCard.svelte:19-38 + clamp-hint | Out-of-range values are clamped with inline feedback before saving. | P2
USAGE-C059 | CONFIRMED | config/schema.rs:196-198 default_status_format | Default template is `🎵 {artist} - {track} 🎧`. | P2
USAGE-C060 | CONFIRMED | spotify.rs:1771 ("artist") + 1041-1055 episode mapping | `{artist}` is the artist name, show name on an episode. | P2
USAGE-C061 | CONFIRMED | spotify.rs:1772 ("track") + episode mapping | `{track}` is the track name, episode name on an episode. | P2
USAGE-C062 | CONFIRMED | spotify.rs:1773 ("album") + episode mapping | `{album}` is the album name, publisher on an episode. | P2
USAGE-C063 | CONFIRMED | spotify.rs:1821-1825 emoji selection | Emoji is 🎵 track / 🎙️ episode / ⏸️ paused. | P2
USAGE-C064 | CONFIRMED | spotify.rs:1774 ("device") | `{device}` renders the device name. | P2
USAGE-C065 | CONFIRMED | spotify.rs:1777 ("playlist") from context.playlist | `{playlist}` renders the playlist/album/artist/show the item was started from. | P2
USAGE-C066 | CONFIRMED | spotify.rs:1778 ("context" same value as playlist) | `{context}` is an exact alias for `{playlist}`. | P2
USAGE-C067 | CONFIRMED | spotify.rs:1749-1757 format_progress | `{progress}` renders minutes:seconds. | P2
USAGE-C068 | CONFIRMED | spotify.rs:1755 (None → empty) | Empty when Spotify reports no position (live stream). | P2
USAGE-C069 | CONFIRMED | spotify.rs:1783 | `{shuffle}` is 🔀 while on, empty while off. | P2
USAGE-C070 | CONFIRMED | spotify.rs:1784 (repeat.is_on()) | `{repeat}` is 🔁 while on (context or track), empty while off. | P2
USAGE-C071 | CONFIRMED | spotify.rs:1785 ("show") | `{show}` is episode-only, empty on a music track. | P2
USAGE-C072 | CONFIRMED | spotify.rs:1790-1797 ("episode") | `{episode}` is episode-only, empty on a music track. | P2
USAGE-C073 | CONFIRMED | spotify.rs:1798-1801 ("publisher") | `{publisher}` is episode-only, empty on a music track. | P2
USAGE-C074 | CONFIRMED | spotify.rs:1903-1907 (identical render) | The documented example renders exactly as written. | P2
USAGE-C075 | CONFIRMED | spotify.rs:1724-1745 substitute_placeholders | Substitution is a single left-to-right pass. | P2
USAGE-C076 | CONFIRMED | spotify.rs:1717-1723 + test at 2097 | Text a token produces is never re-scanned. | P2
USAGE-C077 | CONFIRMED | spotify.rs:1730-1739 (unterminated brace + unknown token kept verbatim) | Unknown tokens and an unclosed `{` are left as typed. | P2
USAGE-C078 | CONFIRMED | spotify.rs:500-507 PlaybackContext::sample() | The fixed preview sample is device Kitchen speaker, playlist Workout Mix, progress 0:00, shuffle and repeat on. | P2
USAGE-C079 | CONFIRMED | spotify.rs:532 DEFAULT_EPISODE_STATUS_FORMAT | Episodes use the built-in `🎙️ {show} - {episode}` template. | P2
USAGE-C080 | CONFIRMED | spotify.rs:527-532 ("until that key exists, the built-in default IS what episodes use") | The episode template has no config key yet. | P2
USAGE-C081 | CONFIRMED | config/schema.rs:204-206 default_clear_on_pause = true | `teams.clear_on_pause` defaults to On. | P2
USAGE-C082 | CONFIRMED | polling/write.rs clear path; teams.rs placeholder_expiry_rfc3339 | Clear-on-pause clears the Teams status when Spotify pauses or stops. | P2
USAGE-C083 | CONFIRMED | grep for clear_on_pause in src/lib → only Diagnostics + ProfilesCard (per-profile) | There is no top-level Settings toggle for the global `teams.clear_on_pause`; it must be edited in config.json. | P2
USAGE-C084 | DRIFT | polling/ dir contains no poll_once.rs (clocks.rs, daemon.rs, exit.rs, gate.rs, iteration.rs, loop.rs, mod.rs, presence.rs, refresh.rs, rules.rs, state.rs, status_text.rs, timing.rs, write.rs) | The claim that clear_on_pause is "consumed at src-tauri/src/polling/poll_once.rs" cites a module that no longer exists as a file; the module was mechanically split (gate.rs:1-6) into gate/iteration/write/etc. | P2
USAGE-C085 | CONFIRMED | config/schema.rs:208-210 default_profanity_filter = true | The profanity filter defaults to On. | P2
USAGE-C086 | CONFIRMED | profanity.rs filter_status_for_locale | Profane track/artist names are replaced with a safe placeholder. | P2
USAGE-C087 | CONFIRMED | i18n.rs:161 placeholder_default = "Currently Listening to Spotify" | The default placeholder matches exactly. | P2
USAGE-C088 | CONFIRMED | profanity.rs:715-730 apply_placeholder substitutes {emoji} | The placeholder supports the `{emoji}` token. | P2
USAGE-C089 | CONFIRMED | config/schema.rs:115-116 profanity_extra_words `#[serde(default)]` = empty | Custom words default to empty. | P2
USAGE-C090 | CONFIRMED | config/clamp.rs:34-39 (truncate(64), 32 chars); StatusFormatCard.svelte:36-37 | Extra words are bounded to the first 64 entries of 32 characters. | P2
USAGE-C091 | CONFIRMED | i18n/en.ts:465-466 extraWordsClampHint | The same inline clamp feedback is shown. | P2
USAGE-C092 | CONFIRMED | profanity.rs:552-589 boundary gates; tests at 972, 1029 | Added words honour word boundaries and catch the usual evasions. | P2
USAGE-C093 | CONFIRMED | StatusFormatCard.svelte:93-118 (live preview behind the filter toggle) | The effect is visible immediately in the status preview. | P2
USAGE-C094 | CONFIRMED | config/schema.rs:216-218 default_availability_sync = false | Show Available while listening defaults to Off. | P2
USAGE-C095 | CONFIRMED | polling/presence.rs:26 AVAILABILITY_REARM_SECONDS = 240; gate.rs:436 | Presence is re-armed every 4 minutes while a track plays. | P2
USAGE-C096 | CONFIRMED | polling/presence.rs:32-33, 70-82 presence_expiration_duration | Session length = remaining + one re-arm period, clamped to PT5M–PT4H. | P2
USAGE-C097 | CONFIRMED | polling/presence.rs:66-68 (issue #636 finding) | Bounding the session by listening time prevents a crash leaving the user green. | P2
USAGE-C098 | CONFIRMED | polling/presence.rs:72 (None → "PT4H") | A live/unknown-position stream still asks for PT4H. | P2
USAGE-C099 | CONFIRMED | config/schema.rs:220-222 default_presence_gate = true | Pause status during meetings/calls/DND defaults to On. | P2
USAGE-C100 | CONFIRMED | teams.rs:1099-1128 presence_gate_reason; is_presence_gated | Presence is read before a write and busy/meeting/call/presenting states skip the write. | P2
USAGE-C101 | CONFIRMED | polling/loop.rs (keepalive continues) | Polling continues while only the Teams write is skipped. | P2
USAGE-C102 | CONFIRMED | config/schema.rs:224-226 default_respect_manual_status = true | Never overwrite a hand-set status defaults to On. | P2
USAGE-C103 | CONFIRMED | polling/presence.rs:108-130 manual_status_blocks_write | A foreign, unexpired status message blocks the write. | P2
USAGE-C104 | CONFIRMED | polling/presence.rs:88-92 (reuses the presence sample) | The manual-status check reuses the gate's presence read. | P2
USAGE-C105 | CONFIRMED | polling/presence.rs:125-127 (expired message stops blocking) | An expired hand-set message stops blocking and the next track posts. | P2
USAGE-C106 | CONFIRMED | polling/presence.rs:91-92 (no sample → fails open); write.rs:2112-2117 | A failed presence read lets the write proceed. | P2
USAGE-C107 | CONFIRMED | config/schema.rs:228-230 default_gate_when_out_of_office = false | Pause while out of office defaults to Off. | P2
USAGE-C108 | CONFIRMED | teams.rs:1122-1126 (out_of_office flag or outOfOffice activity) | Both documented out-of-office signals are honoured. | P2
USAGE-C109 | CONFIRMED | gate.rs:24-33 ooo_gate_enabled (rule presence action overrides) | A rule carrying its own presence action overrides the OOO gate. | P2
USAGE-C110 | CONFIRMED | teams.rs:26 MICROSOFT_GRAPH_SCOPES includes Presence.Read, profile, offline_access | The Presence.Read and profile scopes are part of the device-code request. | P2
USAGE-C111 | CONFIRMED | SETUP.md:156 target exists | The SETUP.md cross-reference target exists. | P3
USAGE-C112 | CONFIRMED | TeamsCard.svelte scopesMissing banner (i18n/en.ts presenceScopeBanner) | The Teams card shows a reconnect prompt when the presence scopes are missing. | P2
USAGE-C113 | CONFIRMED | polling/rules.rs + write.rs:1493-1515 | Quiet hours and track rules suppress or replace the write and can move presence. | P2
USAGE-C114 | CONFIRMED | gate.rs:36-70 (shared schedule_window_contains) and the gate path | Rules reuse the presence-gate path, so a rule stopping mid-track posts without waiting. | P2
USAGE-C115 | CONFIRMED | config/schema.rs:495-527 QuietHoursEntry | Each entry has enabled, start and end minutes and a weekday list. | P2
USAGE-C116 | CONFIRMED | gate.rs:96-106 (wrap-around semantics) | Times wrap around midnight. | P2
USAGE-C117 | CONFIRMED | gate.rs:100-104 (empty days = every day) | No weekday ticked means every day. | P2
USAGE-C118 | CONFIRMED | config/schema.rs:525-526, 551-553 default_pause_polling = false | The Stop-polling checkbox exists and is off by default. | P2
USAGE-C119 | CONFIRMED | gate.rs:206-222 quiet_pause_at; loop.rs | A pausing window makes no Spotify or Teams request, moves no write clock and leaves the status untouched. | P2
USAGE-C120 | CONFIRMED | gate.rs:195-201 (ANY pausing entry asserts; latest end reported) | The poller re-checks every iteration and resumes by itself; the thread is never parked. | P2
USAGE-C121 | CONFIRMED | gate.rs:226-239 quiet_pause_log_line | The log shows `[POLLING] quiet hours: polling paused until …` and the resume line. | P2
USAGE-C122 | CONFIRMED | config/schema.rs:514-520 QuietHoursEntry presence pair; RulesCard.svelte:490-502 | Each quiet-hours row has a Presence-while-this-rule-applies picker. | P2
USAGE-C123 | CONFIRMED | config/clamp.rs:120-136 PRESENCE_COMBINATIONS | The picker offers exactly the five documented (availability, activity) pairs. | P2
USAGE-C124 | CONFIRMED | config/clamp.rs:146-169 normalize_presence_pair; clamp.rs:200-215 | An unrecognised pair is cleared at the config boundary rather than guessed. | P2
USAGE-C125 | CONFIRMED | polling/presence.rs:634 availability_sync_enabled gate | The action is inert while availability sync is off. | P2
USAGE-C126 | CONFIRMED | polling/presence.rs:583-646; gate.rs presence decision | Rule presence never overrides a call, a meeting, or a hand-set status. | P2
USAGE-C127 | DRIFT | polling/rules.rs:321-346 (case-insensitive substring match; empty = match-anything) | The substring matching and optional replacement status are correct, but the card also exposes a match-style picker (substring/exact/glob), album / show / device / playlist-URI substring fields, a negate flag, a minimum-duration gate and a five-kind action picker (suppress / replace / snooze minutes / switch profile / set presence) — none of which the doc mentions. | P1
USAGE-C128 | CONFIRMED | RulesCard.svelte:630 (artist input bound to artist_substring) | Artist contains matches against the track's artist. | P2
USAGE-C129 | CONFIRMED | polling/rules.rs:331-346 (empty pattern matches anything) | An empty pattern matches any artist. | P2
USAGE-C130 | CONFIRMED | RulesCard.svelte:636 | Track title contains matches against the track title. | P2
USAGE-C131 | CONFIRMED | polling/rules.rs:331-346 | An empty title pattern matches any title. | P2
USAGE-C132 | CONFIRMED | polling/rules.rs:389 schedule_window_contains; RulesCard.svelte:694-709 | Active days gate the rule; none ticked means every day. | P2
USAGE-C133 | CONFIRMED | gate.rs:96-106 (start == end matches nothing) | An end of 00:00 means end of day and a start equal to the end never matches. | P2
USAGE-C134 | CONFIRMED | RulesCard.svelte:900-901 (default start 0 / end 1440) | The default 00:00 → 24:00 window contains every time. | P2
USAGE-C135 | CONFIRMED | polling/write.rs:1498-1500 (replacement becomes the posted text) | Non-empty replacement text is posted instead of the formatted status. | P2
USAGE-C136 | CONFIRMED | polling/rules.rs suppress semantics; write.rs | An empty replacement suppresses the write entirely for the matching track. | P2
USAGE-C137 | CONFIRMED | clamp.rs:130-136 + RulesCard.svelte presence picker | The rule presence picker defaults to Don't change my presence and offers the five pairs. | P2
USAGE-C138 | CONFIRMED | polling/presence.rs:583-646 (armed on the nothing-playing path; scoped rule's stale pair clears at track end) | The rule presence pair is armed while the rule matches, including on the no-track path. | P2
USAGE-C139 | CONFIRMED | RulesCard.svelte:885 (new rule pushed with enabled: false) | New track rules are added disabled. | P2
USAGE-C140 | CONFIRMED | polling/rules.rs first-match walk; test at 592 | Rules are evaluated top to bottom and the first match wins. | P2
USAGE-C141 | CONFIRMED | RulesCard.svelte moveRuleUp/moveRuleDown (i18n/en.ts:509-510) | Move-up / move-down buttons change the evaluation order. | P2
USAGE-C142 | CONFIRMED | polling/rules.rs schedule + day gates | A rule outside its window or day set simply does not match. | P2
USAGE-C143 | CONFIRMED | gate.rs:424-438 gate_recheck_due uses AVAILABILITY_REARM_SECONDS = 240 | A suppressed track is re-checked every 240 s on the same clock as the presence gate. | P2
USAGE-C144 | CONFIRMED | polling/rules.rs decision re-evaluation each iteration | Clearing the rule or leaving the window posts the status mid-track. | P2
USAGE-C145 | CONFIRMED | polling/presence.rs:634-646 (rule presence fires even when replacement is empty) | A rule with an empty replacement still moves presence. | P2
USAGE-C146 | CONFIRMED | polling/write.rs:1493-1497 (replacement flows through the identical-write skip) | An unchanged replacement is not re-posted every cycle. | P2
USAGE-C147 | CONFIRMED | polling/gate.rs + status_text.rs (quiet hours decided before the track-rule walk) | Quiet hours win over track rules. | P2
USAGE-C148 | CONFIRMED | polling/gate.rs (quiet hours decide the iteration before rules are consulted) | A matching quiet-hours row prevents the track rule from being consulted at all. | P2
USAGE-C149 | CONFIRMED | config/clamp.rs:176 MAX_RULE_STATUS_CHARS = 128; i18n/en.ts:436-437 | Replacement text is capped at 128 characters. | P2
USAGE-C150 | CONFIRMED | polling/write.rs:2047-2050 (quiet-hours replacement on the no-track clear); status_text.rs | The quiet-hours replacement is posted from both the playing path and the stop/pause clear. | P2
USAGE-C151 | CONFIRMED | rules.manualStatusLabel / pausedStatusPlaceholder / stoppedStatusPlaceholder (i18n/en.ts:511-515) | The two texts are editable at the bottom of the rules card with the stated defaults. | P2
USAGE-C152 | CONFIRMED | polling/status_text.rs:19 MUSIC_EMOJI; schema.rs:43-44 (empty text left alone → default) | The 🎵 prefix is added for you and clearing a field restores the shipped default. | P2
USAGE-C153 | CONFIRMED | polling/write.rs:1498-1500 | A matching rule's replacement takes precedence over both placeholder texts. | P2
USAGE-C154 | CONFIRMED | config/schema.rs:748-752 StatusRulesConfig | Both lists live in config.json under `status_rules` with `quiet_hours[]` and `track_rules[]`. | P2
USAGE-C155 | CONFIRMED | config/schema.rs:495-552 (per-field serde defaults); migrate.rs | The 4.7 schedule fields, pause_polling and the status texts are additive with serde defaults. | P2
USAGE-C156 | CONFIRMED | config/migrate.rs SCHEMA_VERSION handling | A pre-4.7 or pre-4.5 config file loads unchanged. | P2
USAGE-C157 | CONFIRMED | config/schema.rs:342-344 default_interval_seconds = 30 | The default interval is 30 s. | P2
USAGE-C158 | CONFIRMED | polling/timing.rs:248-260 playing_track_sleep (None → default interval) | The baseline gap is used when no playback position is reported. | P2
USAGE-C159 | CONFIRMED | PollingCard.svelte:47-48 (range min=10 max=60) | The default-interval slider spans 10–60 s. | P2
USAGE-C160 | CONFIRMED | polling/timing.rs:292-302 pause_backoff (0→base, 1→×2, 2→×4, else ceiling) | The pause backoff doubles the base and is capped at the ceiling. | P2
USAGE-C161 | CONFIRMED | config/schema.rs:346-348 default_min_interval_seconds = 10 | The min interval is 10 s. | P2
USAGE-C162 | CONFIRMED | PollingCard.svelte:59-60 (min=5 max=30); clamp.rs:9 | The min-interval field is bounded 5–30 s in Settings. | P2
USAGE-C163 | CONFIRMED | config/schema.rs:350-352 default_max_interval_seconds = 60 | The max interval is 60 s. | P2
USAGE-C164 | CONFIRMED | PollingCard.svelte:74 (max=300) | The max-interval field accepts up to 300 s in Settings. | P2
USAGE-C165 | CONFIRMED | config/schema.rs:358-360 default_pause_backoff_max = 300 | The paused-backoff ceiling is 300 s. | P2
USAGE-C166 | CONFIRMED | clamp.rs:24 (clamp(60, 3600)); PollingCard.svelte:29-30 | The pause ceiling is accepted in the 60–3600 s range with inline feedback. | P2
USAGE-C167 | CONFIRMED | polling/timing.rs:292-302 | Raising the ceiling lets the ladder climb further; lowering it settles sooner. | P3
USAGE-C168 | CONFIRMED | polling/timing.rs:295 (ceiling = ceiling.max(default)) | A value below the base interval is floored at the base. | P2
USAGE-C169 | DRIFT | config/clamp.rs:7-25 clamp_polling | All four values are clamped by the backend poll clamp. The module is `config/clamp.rs`, not `config.rs`. | P2
USAGE-C170 | CONFIRMED | clamp.rs:8-24 (default 5-300, min 5-30, max min..300, ceiling 60-3600) | The documented clamp ranges match. | P2
USAGE-C171 | CONFIRMED | i18n/en.ts:159-160 clampHint; PollingCard.svelte:103-107 | The Settings form previews the clamped values with the quoted message. | P2
USAGE-C172 | CONFIRMED | config/schema.rs:930-932 default_notification_class = true | Track-change notifications default to On. | P2
USAGE-C173 | CONFIRMED | src/lib/stores/notifications.ts TRACK_NOTIFICATION_THROTTLE_MS = 5000 | Track notifications are throttled to one per 5 s. | P2
USAGE-C174 | CONFIRMED | notifications.ts notifyTrackChange (id dedupe) | The same track is never notified twice. | P2
USAGE-C175 | CONFIRMED | notifications.ts CLASS_TARGETS id + group | The newest notification replaces the previous one in place where the platform supports it. | P2
USAGE-C176 | CONFIRMED | config/schema.rs:930-932 | Sync-stopped notifications default to On. | P2
USAGE-C177 | CONFIRMED | +layout.svelte:446-454 (self_terminated flag) | The sync-stopped notification fires when the poller gave up by itself. | P2
USAGE-C178 | CONFIRMED | +layout.svelte:446-454 self_terminated check; sync.ts | A user-initiated Pause Sync is deliberately not reported. | P2
USAGE-C179 | CONFIRMED | config/schema.rs:930-932 | Auth-required notifications default to On. | P2
USAGE-C180 | CONFIRMED | +layout.svelte:284-286 teams-reconnect-required | The auth-required notification fires on Teams session expiry. | P2
USAGE-C181 | CONFIRMED | +layout.svelte:284-286 (user_initiated filter); onboarding.rs:642-646 | A reconnect the user just pressed is skipped. | P2
USAGE-C182 | CONFIRMED | config/schema.rs:930-932 | Update-staged notifications default to On. | P2
USAGE-C183 | CONFIRMED | updater_bg.rs install_pending_on_exit; +layout.svelte:172-197 | The notification fires once when the deferred update finished downloading and verified. | P2
USAGE-C184 | CONFIRMED | config/schema.rs:775-800 NotificationsConfig | All four classes live in config.json under `notifications`. | P2
USAGE-C185 | CONFIRMED | notifications.ts migrateLegacyNotificationPreference | The pre-4.7 `notificationsEnabled` flag migrates into `track_change` once, on first save. | P2
USAGE-C186 | CONFIRMED | notifications.ts notifyTrackChange + sendNow (no timer for the other three) | Track changes keep the throttle and replace-in-place id; the others notify once per occurrence. | P2
USAGE-C187 | CONFIRMED | Dashboard.svelte:646 (track change) vs +layout.svelte:172-197, 284-286, 446-454 | The track-change toast is dispatched by the Dashboard; the other three by the always-mounted layout. | P2
USAGE-C188 | CONFIRMED | notifications.ts setNotificationPreference → ensurePermission | The first enabled class may ask the OS for notification permission. | P2
USAGE-C189 | CONFIRMED | src/lib/stores/theme.ts readInitial/STORAGE_KEY; config default | Theme defaults to Dark. | P2
USAGE-C190 | CONFIRMED | AppearanceCard.svelte theme options; theme.ts Theme type | Dark, Light and System are the three options. | P2
USAGE-C191 | CONFIRMED | theme.ts:84-94 (lightQuery change listener repaints when preference is system) | System follows the OS appearance live with no restart. | P2
USAGE-C192 | CONFIRMED | theme.ts:86 (returns early unless preference is system) | An explicit Dark or Light is pinned and never overridden by the OS. | P2
USAGE-C193 | CONFIRMED | src/app.html:32-51 (pre-paint bootstrap) | The pre-paint bootstrap resolves the stored preference. | P2
USAGE-C194 | CONFIRMED | theme.ts density store default comfortable; app.html:45-50 | Compact spacing defaults to Off. | P2
USAGE-C195 | CONFIRMED | theme.ts:68-80 ([data-density] token-scale override painted pre-paint) | Compact is a token-scale override, not component variants. | P2
USAGE-C196 | CONFIRMED | theme.ts:68-76 (density painted independent of data-theme) | Compact is independent of the theme including System. | P2
USAGE-C197 | CONFIRMED | config/schema.rs:1064 autostart default false; apply_os_autostart | Launch at login defaults to Off. | P2
USAGE-C198 | CONFIRMED | commands/window.rs apply_os_autostart | The app starts automatically at OS boot when enabled. | P2
USAGE-C199 | CONFIRMED | config/schema.rs locale Option default None → resolves from browser/OS | Language defaults to System. | P2
USAGE-C200 | DRIFT | AppearanceCard.svelte:157-164 (8 options: en, de, fr, es, it, pl, pt, nl); src/lib/i18n/store.svelte.ts:29,44 | The doc says "English, Deutsch (German), or Français (French)" but the card ships all eight locales. | P1
USAGE-C201 | CONFIRMED | src/lib/i18n/store.svelte.ts detectSystemLocale / followsSystemLanguage | The language defaults to the OS/browser language. | P2
USAGE-C202 | CONFIRMED | i18n/store.svelte.ts:6-10 (config.locale is the single source of truth); set_locale relabels tray + native menu | The choice is stored in config.json (`locale`) and drives window, tray and native menu. | P2
USAGE-C203 | CONFIRMED | i18n/store.svelte.ts:60-64 DEFAULT_LOCALE = 'en' | An unknown value falls back to English. | P2
USAGE-C204 | CONFIRMED | i18n/store.svelte.ts:171-177 applyDocumentLang; src/lib/i18n.ts:56-62 Intl.PluralRules/NumberFormat | Switching retags `<html lang>` and applies the locale's number and plural rules. | P2
USAGE-C205 | CONFIRMED | i18n/store.svelte.ts:359-378 (storage listener converges every webview) | Detached Logs and Settings windows follow the switch. | P2
USAGE-C206 | CONFIRMED | config/schema.rs:200-202 default_start_minimized = false | Start minimized defaults to Off. | P2
USAGE-C207 | CONFIRMED | app.rs:562-573 (hides the window when the flag is set) | The field hides the window on launch. | P2
USAGE-C208 | CONFIRMED | grep across src/lib components: start_minimized appears only in Diagnostics.svelte (display) and ProfilesCard.svelte (per-profile) | There is no Settings toggle for the global field; it must be set in config.json. | P2
USAGE-C209 | DRIFT | app.rs:562-573 consumes the field (the file is `app.rs`, not `lib.rs`) | The behaviour claim is right, but the cited consumer file is wrong: `teams.start_minimized` is consumed in `src-tauri/src/app.rs:562-573` (and re-applied on save in `commands/config.rs`), not in `lib.rs`. | P2
USAGE-C210 | CONFIRMED | app.rs:574-581 (set_activation_policy Accessory on macOS) | macOS switches the activation policy to Accessory when set. | P2
USAGE-C211 | CONFIRMED | commands/config.rs:428-440 (policy re-synced on every save) | The dock icon reappears when set back to false, with no restart. | P2
USAGE-C212 | CONFIRMED | config/schema.rs:420-422 default_logging_enabled = true | Write a log file defaults to On. | P2
USAGE-C213 | DRIFT | config/schema.rs:452-466 (enabled=false → LevelFilter::Off) + app.rs:1111-1120 (Webview target is the same plugin) | Turning logging off silences the whole logger, including the LogViewer's live stream, so the claim that the in-app viewer still works is false. | P1
USAGE-C214 | CONFIRMED | config/schema.rs:452-466 | The logging switch takes effect immediately. | P2
USAGE-C215 | CONFIRMED | config/schema.rs:424-426 default_log_level = "Info" | Log level defaults to Info. | P2
USAGE-C216 | CONFIRMED | config/schema.rs:457-465 (off/error/warn/info/debug/trace); LoggingCard.svelte:22 | The six level values match. | P2
USAGE-C217 | CONFIRMED | config/schema.rs:452-466 | The log level takes effect immediately. | P2
USAGE-C218 | CONFIRMED | config/schema.rs:428-430 default_max_file_size_mb = 10 | Maximum log file size defaults to 10. | P2
USAGE-C219 | CONFIRMED | app.rs:1095-1096 + tauri_plugin_log max_file_size | The live file rotates at the configured size. | P2
USAGE-C220 | CONFIRMED | clamp.rs:480 (clamp(1, 500)); LoggingCard.svelte:16 | The accepted range is 1–500 MB. | P2
USAGE-C221 | CONFIRMED | config/schema.rs:432-434 default_keep_files = 3 | Archived log files default to 3. | P2
USAGE-C222 | CONFIRMED | app.rs:1096 log_rotation_strategy(keep_files) | The field controls how many rotated files are kept. | P2
USAGE-C223 | CONFIRMED | clamp.rs:481 (clamp(1, 20)); LoggingCard.svelte:17 | The accepted range is 1–20. | P2
USAGE-C224 | CONFIRMED | config/schema.rs:396-398 ("The active PresenceJam.log is not counted") | The live log is kept in addition to the archives. | P2
USAGE-C225 | CONFIRMED | app.rs:1092-1094 (size/retention read once at plugin build) | Size and retention apply from the next launch. | P2
USAGE-C226 | CONFIRMED | config/schema.rs:447-450 | The level and the on/off switch apply at once. | P2
USAGE-C227 | CONFIRMED | LoggingCard.svelte:31-41 (invoke open_logs_folder); +page.svelte:201-208 | The Settings button invokes the same command as the tray item. | P2
USAGE-C228 | CONFIRMED | config/transfer.rs:10-22 export_file_name (version + YYYYMMDD-HHMMSS) | Exports write a dated, version-stamped file. | P2
USAGE-C229 | CONFIRMED | config/transfer.rs:33-65 walk_client_secret_keys; commands/config.rs:587-594 | Exports strip every client_secret key and carry no token material. | P2
USAGE-C230 | CONFIRMED | BackupCard.svelte:33 (backupExported message with path) | The resolved path is shown in the card. | P2
USAGE-C231 | CONFIRMED | BackupCard.svelte:54-59 + i18n/en.ts:533 (confirm names config.json.bak) | Import confirms and names the .bak backup. | P2
USAGE-C232 | CONFIRMED | transfer.rs + commands/config.rs:750 (refuses a plaintext client_secret); clamped_config | Import refuses a plaintext secret, clamps, and reloads the values. | P2
USAGE-C233 | CONFIRMED | BackupCard.svelte:62 | The resolved path is shown in the card. | P2
USAGE-C234 | CONFIRMED | ShortcutsCard.svelte:27-30; config/schema.rs:881-917 | Two app-wide bindings are editable in the card. | P2
USAGE-C235 | CONFIRMED | config/schema.rs:881 DEFAULT_TOGGLE_PLAYBACK_SHORTCUT | Toggle playback defaults to CmdOrCtrl+Alt+P. | P2
USAGE-C236 | CONFIRMED | commands/shortcuts.rs start_syncing_with/player path shared with tray and Dashboard | Play/pause uses the same refresh-aware path. | P2
USAGE-C237 | CONFIRMED | config/schema.rs:884 DEFAULT_TOGGLE_SYNC_SHORTCUT | Pause or resume sync defaults to CmdOrCtrl+Alt+S. | P2
USAGE-C238 | CONFIRMED | commands/shortcuts.rs (slot handler mirrors the tray's toggle-pause) | The shortcut starts or stops the poller like the tray item. | P2
USAGE-C239 | CONFIRMED | commands/shortcuts.rs (global hotkey, no window required) | The bindings work while the window is hidden. | P2
USAGE-C240 | CONFIRMED | ShortcutsCard.svelte:42-48, 126-147 (capture releases the current grab) | The field records the pressed combination with the grab released. | P2
USAGE-C241 | CONFIRMED | ShortcutsCard.svelte:269 (settings.shortcutClear); store bindings = null | Clear empties a slot. | P2
USAGE-C242 | CONFIRMED | commands/shortcuts.rs validate_accelerator; shortcut.ts normalizeShortcutReason | Unparseable, modifier-less, or colliding combinations are refused inline. | P2
USAGE-C243 | CONFIRMED | i18n/en.ts:585 shortcutRegistrationFailed | A refused grab shows "Registration failed on this desktop" with the reason. | P2
USAGE-C244 | CONFIRMED | commands/shortcuts.rs (per-slot registration; plans.rs test at 1305) | A failed registration leaves the other binding working. | P2
USAGE-C245 | CONFIRMED | commands/shortcuts.rs:315-322 (preflight opens an X11 connection on Linux only) | The Linux shortcut backend is X11-only. | P2
USAGE-C246 | CONFIRMED | shortcuts.rs:288-300 X11Unavailable; test at 1343-1365 | Without a reachable X11 display both slots report unavailable. | P2
USAGE-C247 | CONFIRMED | shortcuts.rs:315-322 | A reachable X11 display follows the normal plugin path. | P2
USAGE-C248 | CONFIRMED | shortcuts.rs (register returns Err per slot) | A refused individual grab reports the typed failure inline and leaves the other binding working. | P2
USAGE-C249 | CONFIRMED | shortcuts.rs:126-130; key_binds_bare at 147-179 | A binding needs at least one modifier unless the key binds bare. | P2
USAGE-C250 | CONFIRMED | shortcuts.rs:147-179 (F1–F24, MediaPlayPause, MediaPlay, MediaPause, MediaStop, MediaTrackNext, MediaTrackPrevious) | Function keys and dedicated media keys bind bare. | P2
USAGE-C251 | CONFIRMED | ShortcutsCard.svelte:129-142 (bare Escape/Enter cancels capture) | Bare Escape or Enter cancels the capture instead of binding. | P2
USAGE-C252 | CONFIRMED | UpdatePrompt.svelte onMount check + tauri.conf.json updater endpoints | On startup the app checks GitHub Releases for a newer version. | P2
USAGE-C253 | CONFIRMED | UpdatePrompt.svelte:53 (CHECK_INTERVAL_MS = 24h) | The banner re-checks silently every ~24 hours. | P2
USAGE-C254 | CONFIRMED | i18n/en.ts:288 'Update v{version} available' | The banner text matches the documented form. | P2
USAGE-C255 | CONFIRMED | UpdatePrompt.svelte (download with progress readout; relaunch) | Download & Install downloads and relaunches immediately. | P2
USAGE-C256 | CONFIRMED | i18n/en.ts:296-299 (confirm with staged vs current) | Install on quit opens a confirmation comparing versions. | P2
USAGE-C257 | CONFIRMED | i18n/en.ts:294 'Preparing…', 413 'Preparing update — {percent}%' | The live percentage and no-size fallback match. | P2
USAGE-C258 | CONFIRMED | UpdatePrompt.svelte:630-633 (Cancel next to progress) | A Cancel button sits next to the progress readout. | P2
USAGE-C259 | CONFIRMED | updater_bg.rs:1709-1718 (late download discarded after cancellation) | A cancel during download cannot stop the in-flight transfer; the payload is discarded on arrival. | P2
USAGE-C260 | CONFIRMED | UpdatePrompt.svelte:486-496 (state cleared, check re-run) | After either outcome the banner returns to its plain offer. | P2
USAGE-C261 | CONFIRMED | updater_bg.rs install_pending_on_exit (RunEvent::Exit) | The verified update is applied on the next quit. | P2
USAGE-C262 | CONFIRMED | updater_bg.rs:1799-1802 + i18n/en.ts:301-305 (staleSkipped + installAnyway) | A stale staged update is skipped with an Install anyway override. | P2
USAGE-C263 | CONFIRMED | updater_bg.rs:1847-1850 ("Windows installer relaunches automatically") | Windows relaunches; macOS/Linux pick up the new version on next launch. | P2
USAGE-C264 | CONFIRMED | UpdatePrompt.svelte:45,512 (dismissed flag) | The banner is dismissible. | P2
USAGE-C265 | CONFIRMED | UpdatePrompt.svelte error handling (silent); updater_bg.rs (30 s bound) | A failed check is silent and never blocks the UI. | P2
USAGE-C266 | CONFIRMED | updater_bg.rs (tauri_plugin_updater verifies minisign signature before install); app.rs:1085 | Update payloads are signature-verified against a key baked into the app. | P2
USAGE-C267 | CONFIRMED | README.md:130 (unsigned DMG / unidentified developer note) | The README's macOS Gatekeeper note exists and applies equally to updated builds. | P2
USAGE-C268 | CONFIRMED | updater_bg.rs (staged payload held in PendingUpdate memory; cancel discards) | Deferred updates are verified before staging and held in memory only. | P2
USAGE-C269 | CONFIRMED | UpdatesCard.svelte + config/schema.rs:939-950 UpdateChannel | Settings → Updates chooses between Stable and Beta. | P2
USAGE-C270 | CONFIRMED | updater_bg.rs:915-918 update_endpoints(channel); check_for_update at 1372 | The backend resolves the candidate from the channel choice. | P2
USAGE-C271 | CONFIRMED | updater_bg.rs:901-918 (beta manifest, stable fallback) | Beta falls back to the stable release when the beta feed is missing or not newer. | P2
USAGE-C272 | CONFIRMED | UpdatePrompt.svelte:104-115 (beta offers only the deferred path) | On Beta only Install on quit is offered. | P2
USAGE-C273 | CONFIRMED | lib/stores/detach.ts:24-29 (logs-detached / settings-detached) | Logs and Settings can each be popped out. | P2
USAGE-C274 | CONFIRMED | LogViewer.svelte:498; detach.ts popOut | The Pop out control sits on the pane header. | P2
USAGE-C275 | CONFIRMED | detach.ts:26-29 DETACHED_LABEL; lib.rs detached_pane_spec | The pane opens in its own OS window with the stated labels. | P2
USAGE-C276 | CONFIRMED | detach.ts:120-157 popIn; PageHeader.svelte:14-16 | Pop back in returns the pane to the main window and closes the detached window. | P2
USAGE-C277 | CONFIRMED | Dashboard.svelte:789-810 (focusDetached instead of navigating); detach.ts:141-156 | While a pane is detached the nav focuses it instead of navigating. | P2
USAGE-C278 | CONFIRMED | detach.ts:5-22 (shared app-global state, main window is the source of truth) | Detached windows share live state with the main app. | P2
USAGE-C279 | CONFIRMED | Dashboard.svelte:1020 (🩺 button → currentView 'diagnostics') | The 🩺 button in the header opens the Diagnostics page. | P2
USAGE-C280 | CONFIRMED | diagnostics.rs:76-120 (DiagnosticsSnapshot fields; tokens metadata only; keychain flags; recent_logs) | The snapshot contains the listed sections with token metadata only. | P2
USAGE-C281 | CONFIRMED | Diagnostics.svelte:72-81; i18n/en.ts:177 'Copy diagnostics' | Copy puts the displayed snapshot on the clipboard. | P2
USAGE-C282 | CONFIRMED | Diagnostics.svelte:90-103; i18n/en.ts:178 'Save to file' | Save to file invokes the Rust command. | P2
USAGE-C283 | CONFIRMED | diagnostics.rs:62 SNAPSHOT_MAX_BYTES = 256 * 1024 | The save keeps within the independent 256 KiB limit. | P2
USAGE-C284 | CONFIRMED | diagnostics.rs:57,1651-1665 (download_dir + timestamped name) | The file is written atomically to the platform Downloads directory. | P2
USAGE-C285 | CONFIRMED | Diagnostics.svelte:83-89 (comment: webview supplies neither JSON nor destination) | The page supplies neither the contents nor the destination. | P2
USAGE-C286 | CONFIRMED | diagnostics.rs:66 SNAPSHOT_WRITE_ATTEMPTS = 8; test at 2213 | Rust tries up to eight generated filenames without replacing an existing file. | P2
USAGE-C287 | CONFIRMED | i18n/en.ts:182 saveFailed | The failure message matches the quoted string exactly. | P2
USAGE-C288 | CONFIRMED | Diagnostics.svelte:20 ("No network calls anywhere — matches SECURITY.md 'No Telemetry'") | The page makes no network calls. | P2
USAGE-C289 | CONFIRMED | Diagnostics.svelte:38-57, 45-51; i18n/en.ts:426-428 quarantineTitle/BodyNow | The amber Settings were reset banner appears on top of the snapshot. | P2
USAGE-C290 | CONFIRMED | i18n/en.ts:429-433 (names {name} backup and the folder) | The banner says the file could not be read, names the .bak, and points at the folder. | P2
USAGE-C291 | CONFIRMED | Diagnostics.svelte:38-44 (config_quarantine_backup from a previous launch) | The banner also appears on a later launch. | P2
USAGE-C292 | CONFIRMED | Diagnostics.svelte:28-34 (#537 comment: dismiss only hides) | Dismissing hides it for the session only and nothing on disk is deleted. | P2
USAGE-C293 | CONFIRMED | TROUBLESHOOTING.md:188 "### The app came up with default settings" | The cross-reference target exists. | P3
USAGE-C294 | CONFIRMED | app.rs:1111-1120 (tauri_plugin_log with LogDir target, file_name "PresenceJam") | The log is managed by the logging plugin. | P2
USAGE-C295 | CONFIRMED | app.rs:461-463 app_log_dir(); schema.rs:390 | The Windows log path is under the bundle-id folder. | P2
USAGE-C296 | CONFIRMED | app.rs:442-443 (~/Library/Logs/com.presencejam.app/) | The macOS log path matches. | P2
USAGE-C297 | CONFIRMED | app.rs:461-463 app_log_dir() (XDG data dir on Linux) | The Linux log path matches. | P2
USAGE-C298 | CONFIRMED | schema.rs:393 max_file_size_mb; app.rs:1095-1096 | Rotation size comes from Settings with default 10 MB. | P2
USAGE-C299 | CONFIRMED | schema.rs:399-400 keep_files | Archive count comes from Settings with default 3. | P2
USAGE-C300 | CONFIRMED | schema.rs:396-398 | The folder holds at most keep_files + 1 files. | P2
USAGE-C301 | CONFIRMED | app.rs:461-463; tauri.conf.json:5 identifier | The log directory is the bundle-identifier folder. | P2
USAGE-C302 | CONFIRMED | app.rs:442-443 ("app_log_dir() … carries the bundle-id segment since #300") | Tauri's app_log_dir() appends the bundle id to the local data directory. | P2
USAGE-C303 | CONFIRMED | token_io.rs:198-211 (config-dir base) vs app_log_dir() | The log does not sit next to config.json. | P2
USAGE-C304 | CONFIRMED | commands/misc.rs open-logs-folder / +page.svelte:201-208 | This is the directory the tray item opens. | P2
USAGE-C305 | CONFIRMED | LogViewer.svelte:498-502 + detach.ts | The Log Viewer browses the logs and can be popped out. | P2
USAGE-C306 | CONFIRMED | commands/logs.rs:45,52 (MAX_LOG_LINES 500, LOG_TAIL_MAX_BYTES 256 KiB) | The viewer backfills the last 500 lines capped at 256 KiB. | P2
USAGE-C307 | CONFIRMED | LogViewer.svelte:311-312 (invoke get_recent_logs on mount) | The history is present before the first new line is logged. | P2
USAGE-C308 | CONFIRMED | LogViewer.svelte:64,100-133,174-197 (atBottom guard) | The pane holds the scroll position when scrolled away from the bottom. | P2
USAGE-C309 | CONFIRMED | LogViewer.svelte:397-399,543-544 | Jump to latest pins the pane back to the bottom. | P2
USAGE-C310 | CONFIRMED | LogViewer.svelte:453-458 (seedCancelled) | Clear empties the pane and suppresses the backfill for the window's lifetime. | P2
USAGE-C311 | CONFIRMED | +page.svelte:201-208 (open-logs-folder listener) | The folder can be opened from the tray menu item. | P2
USAGE-C312 | CONFIRMED | logging Macros; AGENTS.md §4 | ERROR marks failures including API and file I/O errors. | P2
USAGE-C313 | CONFIRMED | logging Macros | WARN marks unexpected-but-recoverable conditions. | P2
USAGE-C314 | CONFIRMED | AGENTS.md §4 (info and above land in the log) | INFO covers normal operations. | P2
USAGE-C315 | CONFIRMED | polling/iteration.rs:217,245 (debug! per iteration) | DEBUG logs every polling iteration. | P2
USAGE-C316 | CONFIRMED | config/schema.rs:465 (trace level supported) | TRACE adds per-iteration detail. | P2
USAGE-C317 | CONFIRMED | config/schema.rs:454-465 (enabled=false or level "off" → LevelFilter::Off) | OFF silences the logger like clearing Write a log file. | P2
USAGE-C318 | CONFIRMED | schema.rs logging.log_level; LoggingCard.svelte | The level is set in Settings → Logging and stored as logging.log_level. | P2
USAGE-C319 | CONFIRMED | LoggingCard.svelte:54-83 | The card also turns file logging off and sets the size/retention fields. | P2
USAGE-C320 | DRIFT | cli.rs:8-39 (eight flags: STATUS, SYNC_ONCE, SET_STATUS, SET_STATUS_EXPIRY, CLEAR_STATUS, PROFILE, SERVE, DAEMON); test at 785-827 | The table documents exactly seven CLI flags plus --help, but `--profile` is a documented eighth flag that is missing from the table. | P2
USAGE-C321 | CONFIRMED | cli.rs:88-167 (cli_command returns None for GUI launches) | None of the flags opens the app window. | P2
USAGE-C322 | CONFIRMED | cli.rs:166 (unrecognised → None); test at 742-763 | Any other argument is ignored and the app starts normally. | P2
USAGE-C323 | CONFIRMED | cli.rs:744-755 (test asserts --minimized and the deep-link URL launch the GUI) | --minimized and presencejam:// links are normal GUI launches. | P2
USAGE-C324 | CONFIRMED | cli.rs:476-496 (JSON to stdout, returns 0) | --status prints JSON on stdout and exits 0. | P2
USAGE-C325 | CONFIRMED | commands/sync.rs:143-181 SyncStatus (the seven named fields plus manual_status, recent_manual_statuses, spotify_secret_conflict) | The seven named fields all exist on SyncStatus. | P2
USAGE-C326 | CONFIRMED | cli.rs:279-295 cli_headless_state (no AppHandle); help text | --status builds no window, no tray, and takes no single-instance lock. | P2
USAGE-C327 | CONFIRMED | commands/sync.rs sync_status_from_state (fresh state defaults) | A fresh process reports false/null/empty values. | P2
USAGE-C328 | CONFIRMED | cli.rs:264-295 (config::load_config + cli_read_tokens) | Both flags read the same config.json and tokens.json. | P2
USAGE-C329 | CONFIRMED | cli.rs:477-495 (failures to stderr, JSON still printed) | An unreadable file still prints the JSON with the reason on stderr. | P2
USAGE-C330 | CONFIRMED | cli.rs:609-648 cli_sync_once_iteration (run_oneshot, exit 0/1 with stderr reason) | --sync-once runs exactly one iteration including the Teams write, exiting 0 or 1. | P2
USAGE-C331 | CONFIRMED | cli.rs:508-541 cli_sync_once_preflight (client_id + both providers) | The flag requires a client_id and sign-in to both providers, exiting 1 otherwise. | P2
USAGE-C332 | CONFIRMED | cli.rs:619,628 (log::info via the CLI-mode log plugin) | Logs go to the normal log file. | P2
USAGE-C333 | CONFIRMED | cli.rs:318-377 cli_set_manual_status_from_disk | --set-status writes a manual Teams status and exits. | P2
USAGE-C334 | CONFIRMED | clamp.rs:176 MAX_RULE_STATUS_CHARS = 128; cli.rs:344 | Replacement text is capped at 128 characters. | P2
USAGE-C335 | CONFIRMED | commands/status.rs:283-309 tick_manual_status_expiry; clamp_expiry_minutes 5..=720 | --set-status-expiry sets how long the manual status lives. | P2
USAGE-C336 | CONFIRMED | cli.rs:381-403 cli_clear_manual_status_from_disk | --clear-status clears the manual status immediately. | P2
USAGE-C337 | DRIFT | cli.rs:29-33,179-195 + serve.rs (mutating POST routes) | The doc calls the localhost API "read-only", but alongside `GET /status` and `GET /events` it also serves mutating token-guarded routes — `POST /pause`, `/resume`, `/snooze?minutes=N` and `/profile?id=<id>` — so "read-only" understates the surface a token can reach. | P1
USAGE-C338 | CONFIRMED | cli.rs:34-39,162-164; packaging/ (systemd, launchd, windows units all invoke --daemon) | --daemon runs the headless daemon that the packaging units invoke. | P2
USAGE-C339 | CONFIRMED | cli.rs:197-259 cli_help_text; 787-796 (test asserts every flag appears) | --help prints the usage text and exits 0. | P2
USAGE-C340 | CONFIRMED | cli.rs (no window/tray/single-instance built in CLI mode) | The CLI modes are fully headless. | P2
USAGE-C341 | CONFIRMED | app.rs MINIMIZED_FLAG / has_minimized_flag; cli.rs:252-253 | --minimized starts with the window hidden and is what the autostart plugin passes. | P2
USAGE-C342 | CONFIRMED | cli.rs:744-755 (test asserts GUI launch) | --minimized is a normal GUI launch. | P2
USAGE-C343 | CONFIRMED | main.rs:2 windows_subsystem = "windows" (release) | The Windows release build is a GUI-subsystem executable with no console of its own. | P2
USAGE-C344 | CONFIRMED | app.rs:349-361 attach_parent_console_for_cli (issue #818) | Running from a console prints the output in that window. | P2
USAGE-C345 | CONFIRMED | app.rs:371-378 (attach skipped when a handle is already valid) | A redirected stdout/stderr is left alone. | P2
USAGE-C346 | CONFIRMED | app.rs:379-384 (AttachConsole failure → return, GUI continues) | Launching with no console still starts the GUI normally. | P2
USAGE-C347 | CONFIRMED | cli.rs:279-295 (both flags load from disk) | Both flags read config.json and tokens.json directly, so the app need not be running. | P2
USAGE-C348 | CONFIRMED | app.rs:1067-1080 (single-instance plugin only when !cli_mode) | --sync-once does not take the single-instance lock. | P2
USAGE-C349 | CONFIRMED | polling/iteration.rs:95-132 (run_oneshot honours snooze + quiet-hours pause); tests at 1423,1480 | The one-shot honours an active snooze and quiet-hours pause with no override flag. | P2
USAGE-C350 | CONFIRMED | polling/write.rs:1516-1531 should_skip_identical_write | A one-shot while a track plays posts the status and later dedup applies. | P2
USAGE-C351 | CONFIRMED | cli.rs:279-295 (headless state, no Tauri app); help text | --status and --help need no desktop. | P2
USAGE-C352 | CONFIRMED | cli.rs:499-507 (credentialed run needs an AppHandle); help text ("on Linux it needs a display server") | --sync-once drives the app's own poller through the Tauri runtime. | P2
USAGE-C353 | CONFIRMED | cli.rs:218-219 ("use xvfb-run on a bare machine") | Wrapping with xvfb-run is the documented headless approach. | P2
USAGE-C354 | CONFIRMED | cli.rs:332-332 doc-comment convention; cli_failure_reason / cli_sync_once_exit_code | With no display the process aborts during runtime creation, outside the documented exit codes. | P2
USAGE-C355 | CONFIRMED | cli.rs:508-541 (preflight runs before any runtime work) | The credential gate runs first so the failure exits 1 before the display error. | P2
USAGE-C356 | CONFIRMED | cli.rs:656-662 (failure None → 0) | Exit 0 means the iteration completed. | P2
USAGE-C357 | CONFIRMED | cli.rs:604-608 (no failure signal = completed) | No track, a deduplicated write, and a gate-suppressed write all count as completions. | P2
USAGE-C358 | CONFIRMED | cli.rs:576-594 cli_failure_reason ("{source}: {message}") + 656-662 | Exit 1 carries the poller's own reason on stderr in the documented shape. | P2
USAGE-C359 | CONFIRMED | cli.rs:512-529 (empty client_id or missing tokens → Err) | Exit 1 is also used for the missing-client_id and missing-token cases. | P2
USAGE-C360 | CONFIRMED | cli.rs:636-644 (process::exit on runtime failure) | Other codes mean the app could not start at all. | P2
USAGE-C361 | CONFIRMED | cli.rs:94-165 (first recognised flag returns) | The first recognised flag wins. | P2
USAGE-C362 | CONFIRMED | teams.rs:1166-1170 manual_status_expiry_rfc3339; polling/status_text.rs | Teams custom status messages carry an expiry that Graph honours. | P2
USAGE-C363 | CONFIRMED | schema.rs:354-356 default_expiry_buffer_seconds = 10 | The expiry buffer defaults to 10 s. | P2
USAGE-C364 | CONFIRMED | teams.rs (expiryDateTime set by the client) | The buffer is an app-side choice; Graph does not shorten it. | P2
USAGE-C365 | CONFIRMED | status_text.rs:58-69 paused_status_placeholder / stopped_status_placeholder | The pause and stop placeholders are editable with the stated defaults. | P2
USAGE-C366 | CONFIRMED | polling/timing.rs:236-242 placeholder_expiry_str (now + 60 s) | The placeholder expires 60 s after it is posted. | P2
USAGE-C367 | DRIFT | polling/timing.rs:239-242; used on both the paused (write.rs:1803) and no-track (2181) paths | The 60 s expiry and the both-paths claim are correct, but the cited location is stale: `placeholder_expiry_str()` lives in `src-tauri/src/polling/timing.rs:239-242` (called from `polling/write.rs`), not in the no-longer-existent `poll_once.rs`. | P2
USAGE-C368 | CONFIRMED | polling/timing.rs:236-242 (fixed +60 s, not track-end derived) | The pause placeholder does not inherit the track-end buffer. | P2
USAGE-C369 | CONFIRMED | teams.rs clear_teams_status_message (posts a short-lived placeholder as the clear) | Graph has no explicit clear action; the short-lived placeholder is the documented clear mechanism. | P2
USAGE-C370 | CONFIRMED | polling/timing.rs:236-238 ("so the placeholder self-removes ~1 min after the last successful post even if the app quits") | The placeholder self-removes about a minute after the last post. | P2
USAGE-C371 | CONFIRMED | polling/refresh.rs; spotify.rs refresh path | Spotify tokens are refreshed by PresenceJam when needed. | P2
USAGE-C372 | CONFIRMED | teams.rs refresh_teams_token | Teams tokens are refreshed automatically. | P2
USAGE-C373 | CONFIRMED | teams.rs:26 (offline_access in MICROSOFT_GRAPH_SCOPES) | The device-code sign-in requests offline_access. | P2
USAGE-C374 | CONFIRMED | polling/write.rs:110,228 (one refresh + retry) | A mid-session expiry triggers one refresh-and-retry before prompting. | P2
USAGE-C375 | CONFIRMED | CHANGELOG.md #428 entry | The v4.2.0 behaviour of only prompting when the refresh itself fails is recorded. | P2
USAGE-C376 | CONFIRMED | +layout.svelte:478-497 (reconnect-required → reconnect view) | An unexpected connection drop opens the Reconnect view. | P2
USAGE-C377 | CONFIRMED | SpotifyCard.svelte:137; TeamsCard.svelte:71 (Reconnect buttons in Settings) | Either service can be re-authenticated from Settings. | P2
USAGE-C378 | CONFIRMED | No Disconnect button exists in any settings card (grep) | There is no Disconnect button. | P2
USAGE-C379 | CONFIRMED | Settings.svelte (cards for each service) | The Settings view is the entry point. | P2
USAGE-C380 | CONFIRMED | Settings.svelte:518-578 (invoke reconnect_spotify_session / reconnect_teams); spotify_auth.rs:517-533; onboarding.rs:619-634 | Reconnect clears the stored tokens and starts a fresh sign-in. | P2
USAGE-C381 | CONFIRMED | authFlow store; spotify_auth.rs device/PKCE flow | Completing the browser sign-in finishes the reconnect. | P2
USAGE-C382 | CONFIRMED | token_io.rs:198-211 tokens_file_path_headless | The documented tokens.json location matches. | P2
USAGE-C383 | CONFIRMED | token_io.rs:207-210 (BaseDirs config_dir + BUNDLE_IDENTIFIER) | The Windows path matches. | P2
USAGE-C384 | CONFIRMED | token_io.rs:207-210 | The macOS path matches. | P2
USAGE-C385 | CONFIRMED | token_io.rs:207-210 | The Linux path matches. | P2

## DEFECTS (non-CONFIRMED, by blast radius)

```yaml
claim_id: USAGE-C003
file: USAGE.md:7
class: C2
verdict: DRIFT
evidence: src-tauri/tauri.conf.json:24 (`"windows": [{ ... "visible": true }]`); src-tauri/src/app.rs:558-582 (the window is hidden at startup ONLY when `cfg.teams.start_minimized || launched_minimized`)
finding: "The app window is hidden by default to keep your taskbar clean" is not what the app does out of the box. The main window is declared `visible: true` and is hidden on launch only when the user opts in via `teams.start_minimized` or the autostart plugin's `--minimized`. A fresh install shows the window at startup and hides it only when the user closes it (app.rs:1236-1251).
proposed_fix: replace with "The app window closes to the tray instead of quitting, so the taskbar stays clean; tick **Start minimized** (or set `teams.start_minimized`) if you want it hidden from the first launch."
severity: P1
```

```yaml
claim_id: USAGE-C127
file: USAGE.md:124
class: C2
verdict: DRIFT
evidence: src/lib/components/settings/RulesCard.svelte:712-880 — the issue #868 surfaces "the new match-kind / album / show / device / playlist-uri / duration / negate / action surfaces"; src/lib/i18n/en.ts:679-682 (match kind), 854+ (album/show/device/playlist labels), action kinds "suppress / replace / snoozeminutes / profile / presence" at RulesCard.svelte:787-791; src-tauri/src/config/clamp.rs:227-255 `clamp_track_rule_action` mirrors the legacy fields into the action
finding: "each entry matches case-insensitively on artist and/or track-title substrings, plus an optional replacement status" describes only the legacy half of the model. A track rule also carries a **Match style** (substring / exact / glob) selector, **album / show / device / playlist-URI** substring fields, a **min duration** gate, a **negate** flag, and an **action** picker with five kinds (suppress, replace with text, snooze N minutes, switch presence profile, set presence) — all user-editable in the card. This is a P1-class gap because a user who only reads the documented fields cannot discover the negate, glob, snooze-minutes or profile-switch actions, which have no other documentation page.
proposed_fix: extend the intro to: "**Track rules** — each entry matches on artist and/or track-title substrings (case-insensitively by default), with a **Match style** picker for *substring*, *exact* or *glob* matching, extra album / show / device / playlist-URI fields, an optional **negate** flag, a **minimum duration** gate, and an **action** picker that decides what happens when the rule matches: suppress the write, replace it with text, snooze sync for N minutes, switch the active presence profile, or set your Teams presence."
severity: P1
```

```yaml
claim_id: USAGE-C200
file: USAGE.md:176
class: C4
verdict: DRIFT
evidence: src/lib/components/settings/AppearanceCard.svelte:157-164 — `<option>` list is English, Deutsch, Français, Español, Italiano, Polski, Português (BR), Nederlands; src/lib/i18n/store.svelte.ts:29 `export type Locale = 'en' | 'de' | 'fr' | 'es' | 'it' | 'pl' | 'pt' | 'nl'` and :44 KNOWN array; AGENTS.md:327 "Eight locales ship"
finding: "Interface language: English, Deutsch (German), or Français (French)" lists three of the eight shipped locales. The picker also offers Spanish, Italian, Polish, Brazilian Portuguese and Dutch, and Rust's tray/menu table ships all eight dictionaries (src-tauri/src/i18n.rs, EN/DE/FR/ES/IT/PL/PT/NL at lines 138+).
proposed_fix: replace with "Interface language: English, Deutsch, Français, Español, Italiano, Polski, Português (BR) or Nederlands. The tray menu and the native application menu follow the same choice."
severity: P1
```

```yaml
claim_id: USAGE-C213
file: USAGE.md:183
class: C2
verdict: DRIFT
evidence: src-tauri/src/config/schema.rs:452-466 (`apply_log_level`: `let max_level = if !cfg.enabled { log::LevelFilter::Off }`); src-tauri/src/app.rs:1111-1120 (Stdout, LogDir and Webview targets are all registered on the same `tauri_plugin_log` builder); src/lib/components/LogViewer.svelte (live entries arrive over the plugin's `log://log` event)
finding: "Turns the on-disk log off entirely; the in-app Log Viewer still works from the live buffer" is wrong. `logging.enabled: false` calls `log::set_max_level(LevelFilter::Off)`, which silences the whole logger — the file target, stdout AND the webview event target the Log Viewer streams from. With logging off the Log Viewer shows no new entries either. (Turning the level down to `Error` does keep the viewer working; only the Off switch kills both.)
proposed_fix: replace with "Turns logging off entirely — both the on-disk file and the in-app Log Viewer's live entries stop, because the switch re-arms the global log level rather than just the file target. Takes effect immediately."
severity: P1
```

```yaml
claim_id: USAGE-C337
file: USAGE.md:297
class: C6
verdict: DRIFT
evidence: src-tauri/src/serve.rs (mutating routes); src-tauri/src/cli.rs:236-243 help text: "`POST /pause`, `/resume`, `/snooze?minutes=N` and `/profile?id=<id>` are mutating and require `Authorization: Bearer <token>`"
finding: The doc describes `--serve[=PORT]` as serving "the token-guarded localhost read-only status API". It is not read-only: `GET /status` and `GET /events` are read-only, but the same server also accepts token-guarded `POST /pause`, `/resume`, `/snooze?minutes=N` and `/profile?id=<id>`, and `GET /events` streams live SSE.
proposed_fix: replace "Serves the token-guarded localhost read-only status API." with "Serves the token-guarded localhost API on 127.0.0.1:8649 (default). `GET /status` and `GET /events` are read-only; `POST /pause`, `/resume`, `/snooze?minutes=N` and `/profile?id=<id>` mutate state and need `Authorization: Bearer <token>`."
severity: P1
```

```yaml
claim_id: USAGE-C010
file: USAGE.md:21
class: C2
verdict: DRIFT
evidence: src-tauri/src/config/snooze.rs:58-64 `SnoozePreset { ThirtyMinutes, OneHour, UntilTomorrow, UntilNextMeetingEnds }`; src-tauri/src/tray/snooze.rs:102 builds the `ID_SNOOZE_NEXT_MEETING` item from `strings.snooze_until_next_meeting_ends`; src-tauri/src/i18n.rs:177 `"Until this meeting ends"`
finding: "Snooze the sync: **30 minutes**, **1 hour**, or **until tomorrow**" omits the fourth snooze option. The tray submenu also offers "Until this meeting ends" (issue #867), which reads the Outlook calendar cache and falls back to tomorrow when no meeting is active. The three documented options and the local-midnight semantics are correct.
proposed_fix: change the row to "Snooze the sync: **30 minutes**, **1 hour**, **until tomorrow** (the next *local* midnight), or **until this meeting ends** (reads your Outlook calendar; falls back to tomorrow with no meeting active)."
severity: P2
```

```yaml
claim_id: USAGE-C051
file: USAGE.md:59
class: C2
verdict: DRIFT
evidence: src/lib/components/Dashboard.svelte:43-66 `gatedReasonLabel` — 'quiet-hours', 'track-rule', 'manual-status', 'out of office', 'presenting', 'quiet-time' and 'idle' each map to their own string; src/lib/i18n/en.ts:66-78 (eight distinct presenceGated* strings, only the default falling back to the busy/call/presenting line)
finding: "falls back to a generic *busy, in a call, or presenting* line for a presence-based verdict (busy / Do Not Disturb / focusing / in a meeting / in a call / presenting)" overstates the fallback. The Dashboard maps three more reasons to their own copy: `presenting` → "Status paused — you are presenting or in a full-screen app" (issue #872), `quiet-time` → "Status paused — Focus Assist is on", and `idle` → "Status paused — desktop is idle" (issue #873). Only an unrecognised sample uses the generic line.
proposed_fix: amend the parenthetical to "(busy / Do Not Disturb / focusing / in a meeting / in a call / presenting — plus reason-specific copy for *presenting or full-screen*, *Focus Assist* and an *idle desktop*, which have their own wording)".
severity: P2
```

```yaml
claim_id: USAGE-C084
file: USAGE.md:99
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/ contains clocks.rs, daemon.rs, exit.rs, gate.rs, iteration.rs, loop.rs, mod.rs, presence.rs, refresh.rs, rules.rs, state.rs, status_text.rs, timing.rs, write.rs — no poll_once.rs; src-tauri/src/polling/gate.rs:1-6 "Mechanical split of `poll_once.rs`"
finding: USAGE.md tells the reader that `teams.clear_on_pause` is "consumed at src-tauri/src/polling/poll_once.rs", but that file no longer exists — the module was mechanically split into gate.rs / iteration.rs / write.rs / status_text.rs / timing.rs / presence.rs / rules.rs (issue #754). The behaviour claim (no Settings toggle, edit config.json) is correct; the pointer is not.
proposed_fix: delete the sentence "(consumed at `src-tauri/src/polling/poll_once.rs`)"; if a pointer is wanted, point at `src-tauri/src/polling/write.rs` (the paused-clear / no-track-clear paths both read `config.teams.clear_on_pause` at write.rs:2022-2027).
severity: P2
```

```yaml
claim_id: USAGE-C169
file: USAGE.md:156
class: C1
verdict: DRIFT
evidence: src-tauri/src/config/clamp.rs:7-25 `pub(crate) fn clamp_polling`; there is no `src-tauri/src/config.rs` (the module is `config/` with mod.rs, clamp.rs, io.rs, migrate.rs, patch.rs, schema.rs, snooze.rs, transfer.rs)
finding: The doc cites "the backend (`config.rs::clamp_polling`)". `config.rs` is a directory now; the clamp lives in `config/clamp.rs`. The four clamps and their ranges (default 5–300, min 5–30, max min..300, ceiling 60–3600) are exactly as documented.
proposed_fix: replace "`config.rs::clamp_polling`" with "`config/clamp.rs::clamp_polling`".
severity: P2
```

```yaml
claim_id: USAGE-C209
file: USAGE.md:177
class: C1
verdict: DRIFT
evidence: src-tauri/src/app.rs:544-582 (the start_minimized consumer); src-tauri/src/commands/config.rs:428-440 (macOS activation-policy sync on save). There is no `src-tauri/src/lib.rs` consumer.
finding: USAGE.md says `teams.start_minimized` is "consumed at `src-tauri/src/lib.rs`". The field is consumed in `src-tauri/src/app.rs` (startup hide + macOS Accessory policy) and re-applied on every config save in `commands/config.rs`; `lib.rs` only wires the builder. The claim about behaviour is right, the file pointer is stale.
proposed_fix: replace "(consumed at `src-tauri/src/lib.rs`)" with "(consumed in `src-tauri/src/app.rs`, which hides the window and sets the macOS activation policy; `commands/config.rs` re-applies it on every save)".
severity: P2
```

```yaml
claim_id: USAGE-C320
file: USAGE.md:284
class: C5
verdict: DRIFT
evidence: src-tauri/src/cli.rs:8-39 declares STATUS_FLAG, SYNC_ONCE_FLAG, HELP_FLAG, SET_STATUS_FLAG, SET_STATUS_EXPIRY_FLAG, CLEAR_STATUS_FLAG, PROFILE_FLAG, SERVE_FLAG, DAEMON_FLAG — nine constants; `cli_command` recognises eight of them as commands (cli.rs:88-167); MINIMIZED_FLAG is a GUI launch flag
finding: "the binary also answers seven CLI flags plus `--help`" undercounts. The parser recognises `--profile <id>` as a real command (issue #869, the tray-profile-submenu twin) and `cli_help_text()` documents it, but USAGE.md's table omits it entirely — so the table is missing a flag, and the "seven" count is wrong (eight commands plus `--help`, with `--minimized` and the deep link as normal GUI launches).
proposed_fix: change "seven CLI flags plus `--help`" to "eight CLI flags plus `--help`" and add a row: "`presencejam --profile <id>` | Switches the active presence profile to `<id>` (or `base` to clear it) and exits `0`. The same switch path the tray profile submenu uses; it rewrites `config.json` directly."
severity: P2
```

```yaml
claim_id: USAGE-C367
file: USAGE.md:340
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/timing.rs:236-242 `pub(crate) fn placeholder_expiry_str()` (now + 60 s); call sites at src-tauri/src/polling/write.rs:1803 (paused clear) and write.rs:2181 (no-track clear)
finding: The claim that the 60 s placeholder expiry is "set by `placeholder_expiry_str()` in `src-tauri/src/polling/poll_once.rs`" points at a file that no longer exists. The function lives in `polling/timing.rs` and is called from both the paused and no-track paths in `polling/write.rs`, so the behaviour described is correct.
proposed_fix: replace "`placeholder_expiry_str()` in `src-tauri/src/polling/poll_once.rs`" with "`placeholder_expiry_str()` in `src-tauri/src/polling/timing.rs`, called from `polling/write.rs` on both the paused and the no-track path".
severity: P2
```

```yaml
claim_id: USAGE-C030
file: USAGE.md:34
class: C2
verdict: DRIFT
evidence: src-tauri/src/commands/playback.rs:242 ("detect a missing `user-modify-playback-state` and show the one-time …"); src/lib/components/settings/SpotifyCard.svelte:178-180 renders `t('settings.playbackScopeBanner')`; src/lib/i18n/en.ts:133 `'settings.playbackScopeBanner': 'Spotify added playback controls. Click Reconnect next to this message to enable them.'`
finding: The banner text is "Spotify added playback controls. Click Reconnect next to this message to enable them.", not "Playback control needs a one-time reconnect". The claim that a Settings banner appears is correct.
proposed_fix: replace the quoted string with "a **\"Spotify added playback controls. Click Reconnect next to this message to enable them\"** banner."
severity: P3
```

```yaml
claim_id: USAGE-C056
file: USAGE.md:66
class: C2
verdict: DRIFT
evidence: src/lib/i18n/en.ts:115 `'settings.unsavedChanges': 'Unsaved changes'`; src/lib/components/Settings.svelte:745 renders `t('settings.unsavedChanges')`
finding: The banner text is "Unsaved changes", not "You have unsaved changes" as the doc quotes it. The behaviour (a banner appears when you edit anything) is correct.
proposed_fix: replace the quoted string with "**\"Unsaved changes\"** banner" (or drop the quotes and keep the description).
severity: P3
```

```yaml
claim_id: USAGE-C057
file: USAGE.md:66
class: C2
verdict: DRIFT
evidence: src/lib/i18n/en.ts:37 `'common.resetToDefault': 'Reset to default'`; every `SettingsCard` renders `resetLabel={t('common.resetToDefault')}`
finding: Each section's reset control is labelled "Reset to default", not "Reset" as the doc says. The per-section restore-shipped-defaults behaviour is correct (Settings.svelte:91 "C9: per-section 'Reset to default' using the shared defaults source").
proposed_fix: replace "a **Reset** button" with "a **Reset to default** button".
severity: P3
```

## COUNTS

| Verdict | Count |
| --- | --- |
| CONFIRMED | 370 |
| DRIFT | 15 |
| STALE | 0 |
| MISSING | 0 |
| OVERSTATED | 0 |
| EXTERNAL-UNVERIFIED | 0 |
| UNSOURCED | 0 |
| **Total** | **385** |

Non-CONFIRMED severities (the 15 claims the DEFECTS section details):
P1 × 5 (USAGE-C003, USAGE-C127, USAGE-C200, USAGE-C213, USAGE-C337),
P2 × 7 (USAGE-C010, USAGE-C051, USAGE-C084, USAGE-C169, USAGE-C209, USAGE-C320, USAGE-C367),
P3 × 3 (USAGE-C030, USAGE-C056, USAGE-C057).

Defect classes: C1 × 4, C2 × 9, C4 × 1, C5 × 1, C6 × 1.

No P0 defects were found. Every P1 defect above carries a path:line citation.
