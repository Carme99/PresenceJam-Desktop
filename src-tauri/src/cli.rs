use super::state::AppState;
use crate::{config, polling, token_io};
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::AppHandle;

/// `--status`: print the current `SyncStatus` as JSON on stdout, exit 0.
pub(crate) const STATUS_FLAG: &str = "--status";
/// `--sync-once`: run exactly one poll iteration, then exit 0/1.
pub(crate) const SYNC_ONCE_FLAG: &str = "--sync-once";
/// `--help`: print usage text and exit 0.
pub(crate) const HELP_FLAG: &str = "--help";
/// `--set-status <message>`: post a manual Teams status with the default
/// expiry and exit. Pairs with `--clear-status` (issue #870).
pub(crate) const SET_STATUS_FLAG: &str = "--set-status";
/// `--set-status-expiry <minutes>`: the expiry override for `--set-status`.
/// Defaults to 60; clamped to the documented `5..=720` window.
pub(crate) const SET_STATUS_EXPIRY_FLAG: &str = "--set-status-expiry";
/// `--clear-status`: clear the user's manual Teams status and exit.
pub(crate) const CLEAR_STATUS_FLAG: &str = "--clear-status";
/// `--profile <id>`: switch the active presence profile to `<id>` (or
/// to "base" — `None` — when the id is missing or unknown) and exit
/// 0. Issue #869: the same switch path the tray profile submenu and
/// the `toggle_profile` hotkey use, so the CLI is the third surface
/// the runtime state machine exposes without writing the on-disk base
/// values.
pub(crate) const PROFILE_FLAG: &str = "--profile";

/// `--serve[=PORT]`: token-guarded localhost control + event API
/// (issue #865). Optional port is split off the flag, so the parser
/// sees `--serve`, `--serve=8649`, etc. The default port lives in
/// [`crate::serve::DEFAULT_PORT`].
pub(crate) const SERVE_FLAG: &str = "--serve";
/// `--daemon`: supervised headless daemon (issue #896). Runs the same
/// poller the GUI runs, but installs SIGTERM/SIGINT handlers, omits
/// every GUI surface (window, tray, app menu, deep-link, single-
/// instance lock), and exits 0 on a clean stop signal. Packaging units
/// live under `packaging/{systemd,launchd,windows}/`.
pub(crate) const DAEMON_FLAG: &str = "--daemon";

/// What the argv asked for. A CLI flag is an *alternative* to launching the
/// GUI, never a modifier of it — which is why an unrecognised argument still
/// launches normally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliCommand {
    Help,
    Status,
    SyncOnce,
    /// Issue #870: post a manual Teams status with the supplied message
    /// and (optionally) expiry. Both are required for the dispatcher to
    /// produce a useful command; an `--set-status` without a message is
    /// treated like `--help` (usage + exit 1) for ergonomic CLI behaviour.
    SetManualStatus {
        message: String,
        expiry_minutes: u32,
    },
    /// Issue #870: clear the user's manual Teams status, no arguments.
    ClearManualStatus,
    /// Issue #869: switch the active presence profile. `name` is
    /// `None` when the user passed `--profile base` (or `--profile`
    /// with no argument), the documented way to clear the active
    /// profile and fall back to the base configuration.
    SetActiveProfile {
        name: Option<String>,
    },
    /// Issue #865: launch the token-guarded localhost HTTP control
    /// + event API. `port` is `None` for `--serve` (default port from
    ///   [`crate::serve::DEFAULT_PORT`]) and `Some(p)` for `--serve=p`.
    Serve(Option<u16>),
    /// Issue #896: supervised headless daemon. Same Tauri runtime as
    /// `--sync-once` (windowless, no GUI surfaces) plus SIGTERM/SIGINT
    /// handling and a bounded poller join. Exits 0 on clean stop.
    Daemon,
}

/// Parse the CLI intent out of argv; `None` means "launch the GUI".
///
/// Matching is exact and left-to-right: the first recognised flag wins, and
/// every unrecognised argument is ignored. Generic over the argv element type
/// so the parser is unit-testable without touching the real process argv
/// (mirroring `has_minimized_flag`).
///
/// Issue #870: `--set-status <message>` + the optional
/// `--set-status-expiry <minutes>` are parsed together, so a CLI invocation
/// is a single atomic decision — the partial-flag case (`--set-status`
/// without a message) is treated as "argument missing", not as a
/// separate CliCommand variant.
pub(crate) fn cli_command<I, S>(args: I) -> Option<CliCommand>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut args_iter = args.into_iter();
    while let Some(arg) = args_iter.next() {
        let arg = arg.as_ref();
        if arg == std::ffi::OsStr::new(HELP_FLAG) {
            return Some(CliCommand::Help);
        }
        if arg == std::ffi::OsStr::new(STATUS_FLAG) {
            return Some(CliCommand::Status);
        }
        if arg == std::ffi::OsStr::new(SYNC_ONCE_FLAG) {
            return Some(CliCommand::SyncOnce);
        }
        if arg == std::ffi::OsStr::new(CLEAR_STATUS_FLAG) {
            return Some(CliCommand::ClearManualStatus);
        }
        if arg == std::ffi::OsStr::new(SET_STATUS_FLAG) {
            // Collect the rest of argv as owned strings; the manual status
            // parse is a flat two-pair shape (`--set-status <message>` plus
            // an optional `--set-status-expiry <minutes>`), so a single
            // vector is simpler than juggling iterator clones (issue #928
            // — `args_iter.clone()` does not exist for owned iterators).
            let rest: Vec<String> = args_iter
                .map(|m| m.as_ref().to_string_lossy().into_owned())
                .collect();
            let mut message = String::new();
            let mut expiry_minutes: u32 = 60;
            let mut i = 0;
            if let Some(first) = rest.first() {
                message = first.clone();
                i = 1;
            }
            while i + 1 < rest.len() {
                if rest[i] == SET_STATUS_EXPIRY_FLAG {
                    if let Ok(parsed) = rest[i + 1].parse::<u32>() {
                        expiry_minutes = parsed;
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            return Some(CliCommand::SetManualStatus {
                message,
                expiry_minutes,
            });
        }
        if arg == std::ffi::OsStr::new(PROFILE_FLAG) {
            // The next token — if any — is the profile id. `--profile` with
            // no argument (or `--profile base`) clears the active profile;
            // any other id attempts a switch and the dispatcher validates
            // against the on-disk list (unknown → "base", with a warning).
            let name = args_iter.next().and_then(|s| {
                let s = s.as_ref().to_string_lossy().into_owned();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            });
            return Some(CliCommand::SetActiveProfile { name });
        }
        // Issue #865: `--serve[=PORT]`. The port is part of the same argv
        // token (`--serve=8649`), not a separate arg — a stray `--serve`
        // followed by a numeric token is the pre-#865 behaviour (unknown
        // flag, GUI launches) and we don't change it.
        if let Some(port) = parse_serve_arg(arg) {
            return Some(CliCommand::Serve(port));
        }
        // Issue #896: `--daemon` is an exact-match bare flag.
        if arg == std::ffi::OsStr::new(DAEMON_FLAG) {
            return Some(CliCommand::Daemon);
        }
    }
    None
}

/// Recognise `--serve` (default port) and `--serve=PORT` (issue #865).
/// Returns `Some(None)` for the bare flag, `Some(Some(port))` for the
/// explicit form, and `None` for any other argument.
///
/// Valid ports are 1..=65535. Port 0 is the OS-assigned "give me any free
/// port" sentinel, which the serve path does not bind directly — the
/// security model is "operator-controlled port" so we reject it and fall
/// through to the GUI launch (better than a silent rebind). Malformed
/// port strings (`--serve=abc`, `--serve=99999`) likewise leave the GUI
/// path alone: a typo is louder than a silent error.
pub(crate) fn parse_serve_arg(arg: &std::ffi::OsStr) -> Option<Option<u16>> {
    let bytes = arg.as_encoded_bytes();
    if bytes == SERVE_FLAG.as_bytes() {
        return Some(None);
    }
    let prefix = SERVE_FLAG.as_bytes();
    if bytes.len() <= prefix.len() + 1 || !bytes.starts_with(prefix) || bytes[prefix.len()] != b'='
    {
        return None;
    }
    let port_str = std::str::from_utf8(&bytes[prefix.len() + 1..]).ok()?;
    let port: u16 = port_str.parse().ok()?;
    if port == 0 {
        return None;
    }
    Some(Some(port))
}

/// Usage text for `--help`.
pub(crate) fn cli_help_text() -> String {
    format!(
        "\
PresenceJam {version}

USAGE:
  presencejam [FLAG]

FLAGS:
  --status      Print the current sync status as JSON on stdout and exit 0.
                Runs without a window, a tray icon or the single-instance lock,
                so it also works on a headless machine. The fields are the ones
                the app's `get_sync_status` command returns; a freshly started
                process has no poller, so `is_syncing`, `current_track` and the
                presence fields are empty unless this process polls.
  --sync-once   Run exactly one poll iteration (including the Teams status
                write) and exit 0 on success, or exit 1 with the reason on
                stderr. Logs go to the normal log file. Requires the app to be
                signed in to Spotify and Teams; without credentials it exits 1
                before anything else happens, and on Linux it needs a display
                server (it drives the app's own poller) — use xvfb-run on a
                bare machine.
  --set-status <message>            Post a manual Teams status (issue #870) and
                exit 0 on success, or exit 1 on stderr. The message is
                profanity-filtered (using teams.profanity_extra_words) and
                bounded to 128 characters, exactly like a rule's replacement
                text. A pair of `--set-status` + `--set-status-expiry` is
                the documented way to script a \"Right back in 30\" button
                from CI; the expiry defaults to 60 minutes and is clamped to
                the documented 5..=720 minute window.
  --set-status-expiry <minutes>     The expiry override for `--set-status`.
                Clamped to 5..=720; the Dashboard composer reads the same
                bounds.
  --clear-status                    Clear any manual Teams status (issue #870)
                and exit 0 on success, or exit 1 on stderr.
  --serve[=PORT]                    Start the token-guarded localhost HTTP
                control + event API (issue #865) on 127.0.0.1:PORT (default
                8649) and run until interrupted. `GET /status` returns the
                same JSON shape as `--status`; `GET /events` streams the
                three presence-related Tauri events as SSE. `POST /pause`,
                `/resume`, `/snooze?minutes=N` and `/profile?id=<id>` are
                mutating and require `Authorization: Bearer <token>`. The
                token is 32 random bytes stored in the OS keychain (not
                on disk in plaintext); an operator retrieves it via
                `secret-tool`/`security`/`Credential Manager` on first
                boot. No route writes configuration or token material.
  --daemon     Run as a supervised headless daemon (issue #896): no
                window, no tray, no app menu, no deep-link registration,
                no single-instance lock. The poller starts automatically
                and exits 0 on SIGTERM (Unix; systemd / launchd) or on a
                `taskkill` (Windows; Task Scheduler). Packaging units for
                systemd, launchd and Task Scheduler live under
                `packaging/`.
  --help        Print this help and exit 0.
  --minimized   Start with the window hidden. The autostart plugin passes
                this, and it still launches the GUI.

Any other argument is ignored and the app starts normally, as it always has.
",
        version = env!("CARGO_PKG_VERSION")
    )
}

/// Read the stored tokens without a `tauri::AppHandle` (issues #679 and
/// #840). CLI commands are observational until their own command-specific
/// write path, so legacy plaintext is parsed without migration or chmod.
pub(crate) fn cli_read_tokens() -> Result<token_io::TokensFile, String> {
    let path = token_io::tokens_file_path_headless()?;
    token_io::read_tokens_at_path(&path, token_io::TokenReadMode::ReadOnly)
        .map_err(token_io::TokensLoadError::into_message)
}

/// Build the `AppState` the GUI's setup builds, without a Tauri app.
///
/// Config and tokens come from the same files (`config::load_config`, the
/// headless tokens read), so a CLI process reports the state the app would
/// report. A missing or unreadable file is not fatal here — setup degrades to
/// "no config / no tokens" the same way, and `--status` must still answer the
/// question it was asked. Returns the load failures instead of logging them:
/// the log plugin only exists on the app path, so a headless `log::warn!`
/// would go nowhere and the caller decides what to say on stderr.
pub(crate) fn cli_headless_state() -> (Arc<AppState>, Vec<String>) {
    let state = Arc::new(AppState::new());
    let mut failures = Vec::new();
    match config::load_config(&state.caches) {
        Ok(cfg) => *state.config.get_mut() = Some(Arc::new(cfg)),
        Err(e) => failures.push(format!(
            "no config loaded ({e}); reporting the built-in defaults"
        )),
    }
    match cli_read_tokens() {
        Ok(tokens) => state.tokens_load.install_loaded(&state.tokens, tokens),
        Err(e) => failures.push(format!(
            "no tokens loaded ({e}); reporting both providers as disconnected"
        )),
    }
    (state, failures)
}
/// Apply the manual-status filter using the config snapshot loaded by the
/// headless CLI. The locale is passed explicitly rather than installed into
/// the process-global native-language slot, so concurrent CLI surfaces cannot
/// race one another while a different config is being published.
pub(crate) fn filter_cli_manual_status(
    text: &str,
    config: Option<&std::sync::Arc<crate::config::AppConfig>>,
) -> String {
    let placeholder = config
        .map(|cfg| cfg.teams.profanity_placeholder.as_str())
        .unwrap_or_default();
    let extra_words = crate::config::profanity_extra_words_for_filter(config);
    let locale = config.and_then(|cfg| cfg.locale.as_deref());
    crate::profanity::filter_status_for_locale(text, placeholder, true, extra_words, locale)
}

/// Issue #870: `--set-status <message>` body. Same filter + clamp + Graph
/// POST pipeline the Dashboard composer runs, with the same error strings,
/// just without an `AppHandle` (the emit is a no-op on this path — there is
/// no Dashboard listening for the event). Builds the `AppState` from disk
/// the same way `cli_headless_state` does, so the CLI flag and the GUI
/// share the profanity lexicon and the placeholder text.
pub(crate) fn cli_set_manual_status_from_disk(
    message: &str,
    expiry_minutes: u32,
) -> Result<(), String> {
    use crate::commands::status as status_cmd;
    let (state, failures) = cli_headless_state();
    for failure in &failures {
        eprintln!("presencejam: {SET_STATUS_FLAG}: {failure}");
    }
    // Issue #870: the preflight mirrors `cli_sync_once_preflight`. The Teams
    // token must be live; the Spotify token is irrelevant for the manual
    // status POST (no Spotify data is read).
    let tokens = state.tokens.teams().clone();
    let Some(tokens) = tokens else {
        return Err("Teams is not connected; cannot set a manual status".to_string());
    };
    if tokens.expires_at <= chrono::Utc::now() {
        return Err(
            "Teams token is expired; sign in again from the app before using this flag".to_string(),
        );
    }

    // Step 1: trim + clamp + profanity filter (issue #538). Mirror the
    // Dashboard composer exactly: empty text is a clear, oversized text
    // is truncated to `MAX_RULE_STATUS_CHARS`.
    let mut text = message.trim().to_string();
    crate::config::clamp_rule_text(&mut text);
    let expiry_minutes = status_cmd::clamp_expiry_public(expiry_minutes);
    if text.is_empty() {
        // Same UX as the Dashboard: blank submit clears.
        return cli_clear_manual_status_from_disk();
    }
    let cfg_guard = state.config.get();
    let posted_text = filter_cli_manual_status(&text, cfg_guard.as_ref());

    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::minutes(expiry_minutes as i64);
    let expiry_str = crate::teams::manual_status_expiry_rfc3339(expires_at);
    crate::teams::set_teams_status_message(&tokens.access_token, &posted_text, Some(&expiry_str))
        .map_err(|e| format!("failed to post manual status to Teams: {}", e))?;
    let manual = status_cmd::ManualStatus {
        message: posted_text.clone(),
        expires_at,
        set_at: now,
    };
    status_cmd::record_manual_status_cli(
        manual,
        status_cmd::RecentManualStatus {
            message: text.clone(),
            used_at: now,
        },
    );
    log::info!(
        "[CLI] set_manual_status: posted {} chars (filtered={}), expires in {} min",
        posted_text.chars().count(),
        posted_text != text,
        expiry_minutes
    );
    Ok(())
}

/// Issue #870: `--clear-status` body. Same Teams clear path the Dashboard
/// composer's Clear button runs. No Spotify data is read.
pub(crate) fn cli_clear_manual_status_from_disk() -> Result<(), String> {
    let (state, failures) = cli_headless_state();
    for failure in &failures {
        eprintln!("presencejam: {CLEAR_STATUS_FLAG}: {failure}");
    }
    let tokens = state.tokens.teams().clone();
    let Some(tokens) = tokens else {
        // No Teams session — clear the local record and report success
        // (the next sign-in starts from a clean slate).
        crate::commands::status::clear_manual_status_record_cli();
        return Ok(());
    };
    let placeholder = crate::commands::sync::safe_placeholder_text(&state);
    crate::teams::clear_teams_status_message(
        &tokens.access_token,
        &placeholder,
        Some(&crate::teams::placeholder_expiry_rfc3339()),
    )
    .map_err(|e| format!("failed to clear manual status on Teams: {}", e))?;
    crate::commands::status::clear_manual_status_record_cli();
    log::info!("[CLI] clear_manual_status: manual status cleared");
    Ok(())
}

/// Issue #869: `--profile <id>` body. Switches the active presence
/// profile through the same `clamped_config` write path every other
/// config change uses; returns the (clamped) new value so the
/// dispatcher prints a single-line confirmation. `--profile base`
/// (or no argument) clears the active profile back to the base
/// configuration; an unknown id is treated as "base" with a warning
/// — `clamped_config` already does the same thing, so the dispatcher's
/// only job here is to validate the input shape.
pub(crate) fn cli_set_active_profile_from_disk(
    name: Option<String>,
) -> Result<Option<String>, String> {
    let (state, failures) = cli_headless_state();
    for failure in &failures {
        eprintln!("presencejam: {PROFILE_FLAG}: {failure}");
    }
    let mut cfg = state
        .config
        .snapshot()
        .map(|c| (*c).clone())
        .ok_or_else(|| "no config loaded".to_string())?;
    let target: Option<String> = match name {
        None => None,
        Some(raw) if raw.eq_ignore_ascii_case("base") || raw.eq_ignore_ascii_case("none") => None,
        Some(raw) => {
            // Match against the clamped list (the same names the tray /
            // hotkey use). The clamp trims + truncates + dedupes so a
            // raw CLI id can only match a clamped id, never a phantom.
            let exists = cfg.presence_profiles.iter().any(|p| p.name == raw);
            if !exists {
                log::warn!(
                    "[CLI] set_active_profile: profile {:?} not found — falling back to base",
                    raw
                );
                None
            } else {
                Some(raw)
            }
        }
    };
    cfg.active_profile = target.clone();
    let clamped = crate::config::clamped_config(&cfg);
    // Persist + republish the active-profile change so the running
    // app picks it up on the next poll. The CLI flag is
    // deliberately NOT a process restart — the doc says it just
    // rewrites `active_profile`.
    let path = crate::config::get_config_path()
        .map_err(|e| format!("failed to resolve config path: {}", e))?;
    let serialized = serde_json::to_string_pretty(&clamped)
        .map_err(|e| format!("failed to serialize: {}", e))?;
    std::fs::write(&path, serialized)
        .map_err(|e| format!("failed to persist to {}: {}", path.display(), e))?;
    log::info!("[CLI] set_active_profile: {:?}", clamped.active_profile);
    Ok(clamped.active_profile)
}

/// `--profile <id>`: localised confirmation strings for the CLI
/// dispatcher. The keys live in `en` / `de` / `fr`; the CLI never
/// reads the i18n table directly because it is built before the i18n
/// module is reachable. Inline copies are intentional — the CLI is
/// the only surface that prints these messages and keeping them out
/// of the i18n table means a CLI run never has to load the
/// dictionaries.
pub(crate) fn t_cli_profile_active(name: &str) -> String {
    format!("Active profile is now \"{name}\".")
}

pub(crate) fn t_cli_profile_active_base() -> String {
    "Active profile cleared — using base configuration.".to_string()
}

/// `--status`: print the status JSON and return the process exit code.
pub(crate) fn cli_status_exit_code() -> i32 {
    let (state, failures) = cli_headless_state();
    for failure in &failures {
        // stderr, not the log file: this path never registers the log plugin
        // (no app is built), and stdout must stay parseable JSON.
        eprintln!("presencejam: {STATUS_FLAG}: {failure}");
    }
    let status = crate::commands::sync::sync_status_from_state(&state);
    match serde_json::to_string_pretty(&status) {
        Ok(json) => {
            // `println!` is this flag's output channel, not logging: see the
            // note above — a caller pipes stdout into `jq`.
            println!("{json}");
            0
        }
        Err(e) => {
            eprintln!("presencejam: {STATUS_FLAG}: failed to serialise the status: {e}");
            1
        }
    }
}

/// The credential test a `--sync-once` run must pass before anything else
/// happens (issue #679). Pure, so the "with credentials it would exit 0" half
/// of the contract is pinned without a signed-in machine: the credentialed
/// run itself needs a display (the poller works through a `tauri::AppHandle`,
/// which only exists once the GUI runtime does).
///
/// Both providers are required: the iteration's whole point is the Teams
/// status write, and with no Spotify tokens the poller logs "No Spotify tokens
/// available, waiting..." and does nothing — a silent no-op is the worst
/// possible exit-0.
pub(crate) fn cli_sync_once_preflight(
    config: &crate::config::AppConfig,
    tokens: &token_io::TokensFile,
) -> Result<(), String> {
    if config.spotify.client_id.trim().is_empty() {
        return Err(
            "no Spotify client_id configured — sign in from the app before using this flag"
                .to_string(),
        );
    }
    if tokens.spotify_tokens.is_none() {
        return Err(
            "not signed in to Spotify (no Spotify tokens stored) — sign in from the app first"
                .to_string(),
        );
    }
    if tokens.teams_tokens.is_none() {
        return Err(
            "not signed in to Microsoft Teams (no Teams tokens stored) — the status write needs it"
                .to_string(),
        );
    }
    Ok(())
}

/// Load the files [`cli_sync_once_preflight`] decides on, turning a load
/// failure into the reason the CLI prints.
pub(crate) fn cli_sync_once_preflight_from_disk() -> Result<(), String> {
    let caches = crate::state::AppCaches::new();
    let config =
        config::load_config(&caches).map_err(|e| format!("cannot read the stored config: {e}"))?;
    let tokens = cli_read_tokens().map_err(|e| format!("cannot read the stored tokens: {e}"))?;
    cli_sync_once_preflight(&config, &tokens)
}

/// Subscribe to the poller's failure signals for the duration of one
/// `--sync-once` iteration: first failure wins.
pub(crate) fn cli_listen_for_failures(
    handle: &AppHandle,
    sink: Arc<Mutex<Option<String>>>,
) -> Vec<tauri::EventId> {
    use tauri::Listener;

    // `error` is `polling::emit_error`'s centralised shape (every Spotify
    // fetch/refresh failure and every failed Teams write goes through it);
    // `reconnect-required` is the poller's "the user must sign in again"
    // signal, and `spotify-reconnect-required` / `teams-reconnect-required`
    // are its provider-specific siblings (the Teams one is emitted on its own).
    const FAILURE_EVENTS: [&str; 4] = [
        "error",
        "reconnect-required",
        "spotify-reconnect-required",
        "teams-reconnect-required",
    ];
    let mut ids = Vec::with_capacity(FAILURE_EVENTS.len());
    for event in FAILURE_EVENTS {
        let sink = Arc::clone(&sink);
        ids.push(handle.listen(event, move |message| {
            let mut slot = sink.lock();
            if slot.is_none() {
                *slot = Some(cli_failure_reason(event, message.payload()));
            }
        }));
    }
    ids
}

/// One line for stderr out of a poller failure event.
pub(crate) fn cli_failure_reason(event: &str, payload: &str) -> String {
    if event != "error" {
        return format!("{event}: a provider needs to be reconnected");
    }
    match serde_json::from_str::<serde_json::Value>(payload) {
        Ok(value) => {
            let source = value
                .get("source")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let message = value
                .get("message")
                .and_then(|s| s.as_str())
                .unwrap_or(payload);
            format!("{source}: {message}")
        }
        Err(_) => payload.to_string(),
    }
}

/// `--sync-once` inside the CLI-mode app: run one iteration through the
/// poller's own one-shot entry point — the same `polling::run_oneshot` the
/// tray's "Refresh" uses, so the status write, the dedup clocks and the
/// presence gate are the ones a loop iteration gets — then exit with its
/// verdict.
///
/// The verdict comes from the poller's failure events rather than from
/// `run_oneshot`'s return value: that fn deliberately discards the iteration
/// verdict (in `RunMode::OneShot` every parking sleep is an immediate Break),
/// but every failure it can hit announces itself — `polling::emit_error` for a
/// Spotify/Teams error, the `*-reconnect-required` pair for dead credentials.
/// Anything else (no track playing, a deduped write, a suppressed gate) is a
/// completed iteration.
pub(crate) fn cli_sync_once_iteration(
    app: &tauri::App,
    state: Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Listener;

    let handle = app.handle().clone();
    let failure: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let listeners = cli_listen_for_failures(&handle, Arc::clone(&failure));

    log::info!("[CLI] {SYNC_ONCE_FLAG}: running one poll iteration");
    polling::run_oneshot(&state, &handle);

    for id in listeners {
        handle.unlisten(id);
    }
    match failure.lock().clone() {
        None => {
            log::info!("[CLI] {SYNC_ONCE_FLAG}: iteration completed");
            // Graceful teardown: the runtime flushes the log plugin in
            // `cleanup_before_exit`, and 0 is the code it exits with anyway.
            handle.exit(cli_sync_once_exit_code(None));
        }
        Some(reason) => {
            let code = cli_sync_once_exit_code(Some(&reason));
            eprintln!("presencejam: {SYNC_ONCE_FLAG}: {reason}");
            log::warn!("[CLI] {SYNC_ONCE_FLAG}: iteration failed: {reason}");
            // Issue #679 review round 2: `AppHandle::exit(code)` cannot report a
            // non-zero code — tauri-runtime-wry turns `RequestExit(code)` into
            // `ControlFlow::Exit`, which tao maps to `process::exit(0)` (the
            // string `ExitWithCode` appears nowhere in the runtime) — so a
            // scripted caller would read a failed iteration as success. Exit the
            // process here instead, after flushing the logger (the plugin's file
            // target buffers, so an unflushed exit would lose this very line).
            log::logger().flush();
            std::process::exit(code);
        }
    }
    Ok(())
}

/// The process exit code for one `--sync-once` iteration: a completed
/// iteration is success, a captured failure signal is not.
///
/// Split out so the mapping is pinned by a test (issue #679 review round 2:
/// the failure branch used to hand its code to `AppHandle::exit`, which drops
/// it — the flag printed a reason and then exited 0).
pub(crate) fn cli_sync_once_exit_code(failure: Option<&str>) -> i32 {
    if failure.is_some() {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::super::app::MINIMIZED_FLAG;
    use super::*;
    /// Issue #679: the three CLI flags are recognised, matched exactly (never
    /// as a prefix of something else) and nothing else is. Issue #865
    /// extends the contract with `--serve[=PORT]`.
    #[test]
    fn test_cli_command_matches_only_the_exact_flags() {
        for (argv, expected) in [
            (vec!["presencejam", "--help"], Some(CliCommand::Help)),
            (vec!["presencejam", "--status"], Some(CliCommand::Status)),
            (
                vec!["presencejam", "--sync-once"],
                Some(CliCommand::SyncOnce),
            ),
            // Issue #865: `--serve` defaults the port; `--serve=PORT` carries
            // it in the same argv token.
            (
                vec!["presencejam", "--serve"],
                Some(CliCommand::Serve(None)),
            ),
            (
                vec!["presencejam", "--serve=8649"],
                Some(CliCommand::Serve(Some(8649))),
            ),
            (
                vec!["presencejam", "--serve=1"],
                Some(CliCommand::Serve(Some(1))),
            ),
            // Issue #896: `--daemon` is a bare flag (no `=PORT` form).
            (vec!["presencejam", "--daemon"], Some(CliCommand::Daemon)),
            // Exact match only: a longer argument that merely starts with a
            // flag is not that flag (issue #589's rule, applied here too).
            (vec!["presencejam", "--statuses"], None),
            (vec!["presencejam", "--sync-once-now"], None),
            (vec!["presencejam", "--help-me"], None),
            (vec!["presencejam", "-s"], None),
            (vec!["presencejam", "--status=1"], None),
            // `--serve` typos and edge cases must fall through (GUI launches
            // — a typo is louder than a silent error).
            (vec!["presencejam", "--serve="], None),
            (vec!["presencejam", "--serve=abc"], None),
            (vec!["presencejam", "--serve=0"], None),
            (vec!["presencejam", "--serve=99999"], None),
            (vec!["presencejam", "--server"], None),
            // `--daemon` typos and edge cases must fall through too.
            (vec!["presencejam", "--daemon=8080"], None),
            (vec!["presencejam", "--daemons"], None),
            (Vec::<&str>::new(), None),
        ] {
            assert_eq!(
                cli_command(argv.clone()),
                expected,
                "argv {:?} must parse to {:?}",
                argv,
                expected
            );
        }

        // OsString argv elements must work too — that is what the real process
        // argv hands the parser.
        assert_eq!(
            cli_command(vec![
                std::ffi::OsString::from("presencejam"),
                std::ffi::OsString::from(SYNC_ONCE_FLAG),
            ]),
            Some(CliCommand::SyncOnce),
            "OsString argv elements must be recognised, like has_minimized_flag"
        );
    }

    /// Issue #679: the flags are an alternative to launching the GUI, never a
    /// modifier of it — every argv shape the app already receives (a bare
    /// launch, the autostart plugin's `--minimized`, a `presencejam://` deep
    /// link and the occasional stray argument) must still launch as it did
    /// before, i.e. parse to no CLI command at all.
    #[test]
    fn test_cli_command_leaves_every_gui_launch_alone() {
        for argv in [
            vec!["presencejam"],
            vec!["/usr/bin/presence-jam"],
            vec!["presence-jam.exe", MINIMIZED_FLAG],
            vec![
                "presencejam",
                "--minimized",
                "presencejam://callback?code=abc&state=def",
            ],
            vec!["presencejam", "presencejam://callback"],
            vec!["presencejam", "--some-future-flag", "value"],
            vec!["presencejam", "-"],
        ] {
            assert_eq!(
                cli_command(argv.clone()),
                None,
                "argv {:?} must launch the GUI, not a CLI mode",
                argv
            );
        }
    }

    /// Issue #679: the parser is documented as "first recognised flag wins",
    /// left to right.
    #[test]
    fn test_cli_command_first_recognised_flag_wins() {
        assert_eq!(
            cli_command(vec!["presencejam", SYNC_ONCE_FLAG, STATUS_FLAG]),
            Some(CliCommand::SyncOnce),
            "the leftmost recognised flag decides"
        );
        assert_eq!(
            cli_command(vec!["presencejam", "--unknown", STATUS_FLAG, HELP_FLAG]),
            Some(CliCommand::Status),
            "unknown arguments are skipped, not treated as a choice"
        );
    }

    /// Issue #679: `--help` (and the README/USAGE docs, asserted in the docs
    /// themselves) must document all four flags, and say what happens to
    /// anything else.
    #[test]
    fn test_cli_help_text_documents_every_flag() {
        let help = cli_help_text();
        for flag in [
            STATUS_FLAG,
            SYNC_ONCE_FLAG,
            HELP_FLAG,
            MINIMIZED_FLAG,
            SERVE_FLAG,
            DAEMON_FLAG,
        ] {
            assert!(help.contains(flag), "the usage text must document {}", flag);
        }
        assert!(
            help.contains("exit 0") && help.contains("exit 1"),
            "the usage text must state the exit codes"
        );
        assert!(
            help.to_lowercase().contains("ignored"),
            "the usage text must state that unknown arguments are ignored"
        );
        // Issue #865: the serve surface's two load-bearing claims — the
        // `Authorization: Bearer` requirement and the keychain-stored
        // token — must both appear, so a future copy edit cannot silently
        // regress the security model.
        assert!(
            help.contains("Bearer"),
            "the serve flag must call out the bearer-token requirement"
        );
        assert!(
            help.contains("keychain"),
            "the serve flag must state the token lives in the OS keychain"
        );
        // Issue #896: the daemon's two non-negotiables — SIGTERM → exit 0
        // and the omission of the GUI surfaces — must both appear.
        assert!(
            help.contains("SIGTERM"),
            "the daemon flag must call out SIGTERM as the clean-stop signal"
        );
        assert!(
            help.contains("single-instance"),
            "the daemon flag must call out the single-instance-lock omission"
        );
    }

    /// Issue #679: the `--sync-once` credential gate. This is the seam the
    /// contract's "with credentials it would exit 0" half is proven at: the
    /// credentialed run itself needs a display (the poller works through an
    /// `AppHandle`, which only exists once a GUI runtime does), so the decision
    /// is pinned here instead of being assumed.
    #[test]
    fn test_sync_once_preflight_requires_both_providers() {
        use crate::spotify::SpotifyTokens;
        use crate::teams::TeamsTokens;

        let complete_tokens = token_io::TokensFile {
            spotify_tokens: Some(SpotifyTokens {
                access_token: "at".to_string(),
                refresh_token: "rt".to_string(),
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(3600),
            }),
            teams_tokens: Some(TeamsTokens {
                access_token: "tat".to_string(),
                refresh_token: None,
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(3600),
            }),
        };
        let mut config = crate::config::AppConfig::default();

        // Functionally signed in: the one-shot may run.
        config.spotify.client_id = "client-id".to_string();
        assert_eq!(
            cli_sync_once_preflight(&config, &complete_tokens),
            Ok(()),
            "configured + signed in to both providers must pass the gate"
        );

        // Not configured at all.
        config.spotify.client_id = String::new();
        let reason = cli_sync_once_preflight(&config, &complete_tokens)
            .expect_err("an unconfigured client_id must be rejected");
        assert!(
            reason.contains("client_id"),
            "the reason must name the missing client_id, got: {reason}"
        );

        // Configured but no Spotify session.
        config.spotify.client_id = "client-id".to_string();
        let no_spotify = token_io::TokensFile {
            spotify_tokens: None,
            teams_tokens: complete_tokens.teams_tokens.clone(),
        };
        let reason = cli_sync_once_preflight(&config, &no_spotify)
            .expect_err("no Spotify tokens must be rejected");
        assert!(
            reason.contains("Spotify"),
            "the reason must name Spotify, got: {reason}"
        );

        // Spotify ok, but the Teams status write has no session to use.
        let no_teams = token_io::TokensFile {
            spotify_tokens: complete_tokens.spotify_tokens.clone(),
            teams_tokens: None,
        };
        let reason = cli_sync_once_preflight(&config, &no_teams)
            .expect_err("no Teams tokens must be rejected");
        assert!(
            reason.contains("Teams"),
            "the reason must name Teams, got: {reason}"
        );
    }

    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is that the headless publish path filters through the locale-aware `filter_cli_manual_status` with its loaded config instead of the raw `profanity::filter_status`; driving the path writes a real Teams status over HTTPS, so the call-site wiring is pinned at the source.
    #[test]
    fn test_headless_manual_status_uses_loaded_locale_for_safe_fallbacks() {
        let source = include_str!("cli.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("cli.rs has no #[cfg(test)] mod tests block");
        let cli_body = body_of(prod_source, "fn cli_set_manual_status_from_disk(");
        assert!(
                cli_body.contains("filter_cli_manual_status(&text, cfg_guard.as_ref())"),
                "the real headless publish path must pass its loaded config snapshot to the locale-aware filter"
            );
        assert!(
            !cli_body.contains("profanity::filter_status("),
            "the headless publish path must not fall back to process-global locale state"
        );

        for (locale, localized) in [
            ("de", "Hört gerade Spotify"),
            ("fr", "Écoute actuellement Spotify"),
        ] {
            let mut config = crate::config::AppConfig {
                locale: Some(locale.to_string()),
                ..Default::default()
            };

            for placeholder in ["", crate::profanity::safe_placeholder_default()] {
                config.teams.profanity_placeholder = placeholder.to_string();
                let snapshot = Arc::new(config.clone());
                assert_eq!(
                    filter_cli_manual_status("what the fuck", Some(&snapshot)),
                    localized,
                    "a blank or shipped-English placeholder must use the loaded locale"
                );
            }

            config.teams.profanity_placeholder = "Eigener Status".to_string();
            let snapshot = Arc::new(config);
            assert_eq!(
                filter_cli_manual_status("what the fuck", Some(&snapshot)),
                "Eigener Status",
                "a custom safe placeholder must remain byte-identical"
            );
            let custom_text = "Eigener Status ✨ — café";
            assert_eq!(
                filter_cli_manual_status(custom_text, Some(&snapshot)),
                custom_text,
                "a clean custom status must remain byte-identical"
            );
        }
    }

    /// Issue #679 review round 2: a failed `--sync-once` iteration must report
    /// failure to the shell. The verdict used to be handed to
    /// `AppHandle::exit`, whose code the runtime drops (`RequestExit` →
    /// `ControlFlow::Exit` → `process::exit(0)`), so this pins the mapping the
    /// flag now exits with itself: no captured failure signal is success,
    /// anything the poller announced is 1.
    #[test]
    fn test_sync_once_exit_code_maps_the_verdict() {
        assert_eq!(
            cli_sync_once_exit_code(None),
            0,
            "a completed iteration (no failure signal) must exit 0"
        );
        assert_eq!(
            cli_sync_once_exit_code(Some("spotify: Failed to get currently playing: boom")),
            1,
            "a captured poller failure must exit 1, not report success"
        );
        assert_eq!(
            cli_sync_once_exit_code(Some(
                "reconnect-required: a provider needs to be reconnected"
            )),
            1,
            "a reconnect signal is a failure too"
        );
    }

    /// Brace-counted body isolation for a top-level `fn` in this file (house
    /// style — order-independent, never anchored on the following fn, which
    /// drifts).
    fn body_of<'a>(prod_source: &'a str, sig: &str) -> &'a str {
        let after_sig = prod_source
            .split(sig)
            .nth(1)
            .unwrap_or_else(|| panic!("cli.rs has no `{}`", sig));
        let open = after_sig
            .find('{')
            .unwrap_or_else(|| panic!("{} has no opening brace", sig));
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        &after_sig[..end.unwrap_or_else(|| panic!("{} body never closed", sig))]
    }
}
