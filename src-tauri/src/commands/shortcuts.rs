//! Global-shortcut bindings (issue #676, 4.7.0 slice S8).
//!
//! Two bindings the user owns in `AppConfig::shortcuts` drive actions the app
//! already has: `toggle_playback` runs the same Spotify play/pause path the
//! `playback_play` / `playback_pause` commands and the tray run, and
//! `toggle_sync` drives `commands::sync`'s start/stop lifecycle. Nothing here
//! re-implements playback or the polling lifecycle — the shortcut and the IPC
//! commands share one implementation (`playback::player_with_refresh`, and the
//! `*_with` inner fns in `commands::sync`), so the two cannot drift apart.
//!
//! Registration runs at startup from the persisted config (`lib.rs`'s setup)
//! and again on every Settings save, and it is *never* fatal: a desktop that
//! refuses a grab (Wayland compositors commonly do; any desktop can refuse a
//! combo another application owns) records `Some(reason)` for that slot and
//! leaves the other binding — and the rest of the app — untouched. The reason
//! reaches the UI through [`ShortcutsStatus`], never through a panic.
//!
//! The plugin cannot tell whether a *foreign* application owns a combo (its
//! `is_registered` documents that it answers for this process only), so that
//! case is not detectable up front: it surfaces as a refused registration at
//! apply time, which is exactly the state the Settings card renders.

use crate::commands;
use crate::config::{AppConfig, ShortcutsConfig};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcut, GlobalShortcutExt, Shortcut, ShortcutState};

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.SHORTCUTS]";

/// The two bindable actions. The declaration order is the registration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutSlot {
    TogglePlayback,
    ToggleSync,
}

impl ShortcutSlot {
    /// Every slot, in registration order.
    pub const ALL: [ShortcutSlot; 2] = [ShortcutSlot::TogglePlayback, ShortcutSlot::ToggleSync];

    /// The wire name the Settings card sends and `config.json` keys on.
    pub fn name(self) -> &'static str {
        match self {
            ShortcutSlot::TogglePlayback => "toggle_playback",
            ShortcutSlot::ToggleSync => "toggle_sync",
        }
    }

    /// Inverse of [`Self::name`]. `None` for an unknown name — a caller typo
    /// must be an error, never a silent no-op.
    pub fn from_name(name: &str) -> Option<Self> {
        ShortcutSlot::ALL.into_iter().find(|s| s.name() == name)
    }

    /// The slot that must never carry the same accelerator.
    pub fn other(self) -> Self {
        match self {
            ShortcutSlot::TogglePlayback => ShortcutSlot::ToggleSync,
            ShortcutSlot::ToggleSync => ShortcutSlot::TogglePlayback,
        }
    }

    /// This slot's raw persisted binding.
    fn raw_binding(self, cfg: &ShortcutsConfig) -> Option<&str> {
        match self {
            ShortcutSlot::TogglePlayback => cfg.toggle_playback.as_deref(),
            ShortcutSlot::ToggleSync => cfg.toggle_sync.as_deref(),
        }
    }
}

/// The binding a slot should be *shown* with: trimmed, with a blank string
/// collapsed to "unbound". A hand-edited `config.json` can carry `""` or
/// whitespace, neither of which is a shortcut.
pub fn configured_binding(cfg: &ShortcutsConfig, slot: ShortcutSlot) -> Option<String> {
    slot.raw_binding(cfg)
        .map(str::trim)
        .filter(|accelerator| !accelerator.is_empty())
        .map(str::to_string)
}

// ── validation ──────────────────────────────────────────────────────────

/// Validates one accelerator for one slot and returns the form the caller must
/// register (never a re-parse of the raw string).
///
/// Two rules, both pre-save so the Settings field can name the reason:
///   * it must parse as a plugin accelerator;
///   * it must not collide with the other slot's binding — compared by parsed
///     identity, so `Ctrl+P` and `CONTROL + p` are one accelerator, and
///     `CmdOrCtrl+P` collides with `Ctrl+P` only on the platforms where they
///     mean the same key.
///
/// A binding the *other* slot cannot itself parse is ignored rather than
/// treated as a conflict: it is that slot's problem, and it registers nothing.
pub fn validate_accelerator(
    slot: ShortcutSlot,
    accelerator: &str,
    other_binding: Option<&str>,
) -> Result<Shortcut, String> {
    let trimmed = accelerator.trim();
    if trimmed.is_empty() {
        return Err("Empty shortcut — use Clear to remove a binding".to_string());
    }
    let parsed = trimmed
        .parse::<Shortcut>()
        .map_err(|e| format!("\"{trimmed}\" is not a recognised shortcut ({e})"))?;
    if let Some(other) = other_binding {
        if let Ok(other_parsed) = other.trim().parse::<Shortcut>() {
            if other_parsed.id() == parsed.id() {
                return Err(format!(
                    "Conflicts with the {} shortcut (\"{}\") — one accelerator cannot drive both actions",
                    slot.other().name(),
                    other.trim()
                ));
            }
        }
    }
    Ok(parsed)
}

// ── planning ────────────────────────────────────────────────────────────

/// What one slot's config entry means for the next registration pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotPlan {
    /// Nothing to register: the slot is absent, `null` or blank.
    Unbound,
    /// Register this accelerator.
    Bound {
        accelerator: String,
        parsed: Shortcut,
    },
    /// Keep the slot unbound and surface the reason.
    Invalid { accelerator: String, reason: String },
}

/// Plans both slots from the config, in registration order.
///
/// Interpreting a config entry is *per slot and independent*: an unbound,
/// blank or unparsable entry in one slot is a result for that slot only and
/// must never drop, skip or overwrite the other slot's binding.
pub fn plan_shortcuts(cfg: &ShortcutsConfig) -> [SlotPlan; 2] {
    let mut plans = [SlotPlan::Unbound, SlotPlan::Unbound];
    for (index, slot) in ShortcutSlot::ALL.into_iter().enumerate() {
        let Some(accelerator) = configured_binding(cfg, slot) else {
            continue;
        };
        let other = configured_binding(cfg, slot.other());
        plans[index] = match validate_accelerator(slot, &accelerator, other.as_deref()) {
            Ok(parsed) => SlotPlan::Bound {
                accelerator,
                parsed,
            },
            Err(reason) => SlotPlan::Invalid {
                accelerator,
                reason,
            },
        };
    }
    plans
}

// ── applying ────────────────────────────────────────────────────────────

/// A binding's press handler: runs the slot's effect.
pub type SlotHandler = dyn Fn(&AppHandle, ShortcutSlot) + Send + Sync + 'static;

/// One slot's registration outcome, as the Settings card renders it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct SlotRegistration {
    /// The binding the config asks for, trimmed — `None` when the slot is
    /// unbound. Set even when the grab is not held, so the card can show what
    /// the user configured next to why it is not live.
    pub accelerator: Option<String>,
    /// Whether the OS currently holds the grab for `accelerator`.
    pub registered: bool,
    /// Why the grab is not held: this desktop refused it, or the stored string
    /// does not parse. `None` whenever `registered` is true, and `None` for a
    /// deliberate release (see [`release_all`]).
    pub error: Option<String>,
}

/// Both slots' registration outcomes (issue #676).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct ShortcutsStatus {
    pub toggle_playback: SlotRegistration,
    pub toggle_sync: SlotRegistration,
}

impl ShortcutsStatus {
    /// The registration record for one slot.
    pub fn slot(&self, slot: ShortcutSlot) -> &SlotRegistration {
        match slot {
            ShortcutSlot::TogglePlayback => &self.toggle_playback,
            ShortcutSlot::ToggleSync => &self.toggle_sync,
        }
    }

    fn set(&mut self, slot: ShortcutSlot, registration: SlotRegistration) {
        match slot {
            ShortcutSlot::TogglePlayback => self.toggle_playback = registration,
            ShortcutSlot::ToggleSync => self.toggle_sync = registration,
        }
    }
}

/// The plugin surface this module uses, narrowed so a test can inject a refused
/// registration — the Wayland case — without a compositor or a running app.
pub trait ShortcutRegistrar {
    /// Grabs `accelerator` for `slot`, calling `handler` on every press.
    fn register(
        &self,
        slot: ShortcutSlot,
        accelerator: &str,
        handler: Arc<SlotHandler>,
    ) -> Result<(), String>;

    /// Releases every grab this process holds.
    fn unregister_all(&self) -> Result<(), String>;
}

/// Runs the registered handler for one plugin event, filtering the release
/// edge: a grab reports both edges, and acting on each would toggle twice per
/// press. Returns whether the handler ran.
pub fn dispatch_event(
    slot: ShortcutSlot,
    state: ShortcutState,
    handler: &dyn Fn(ShortcutSlot),
) -> bool {
    if !matches!(state, ShortcutState::Pressed) {
        return false;
    }
    handler(slot);
    true
}

/// Applies a plan: every previously held grab is released first, so a re-apply
/// (a config save) cannot leave a stale accelerator live — and so a
/// re-registration of an unchanged accelerator does not trip the plugin's
/// duplicate-grab error.
///
/// Failures are recorded per slot and never propagate: a desktop that refuses
/// one grab must not lose the other binding, and must not take the app down.
pub fn apply_plan(
    registrar: &dyn ShortcutRegistrar,
    plans: &[SlotPlan; 2],
    handler: &Arc<SlotHandler>,
) -> ShortcutsStatus {
    if let Err(e) = registrar.unregister_all() {
        // Not fatal: this only clears this process's own grabs, and the two
        // registrations below overwrite the plugin's bookkeeping anyway.
        log::warn!("{CMD} apply: releasing the previous grabs failed: {e}");
    }

    let mut status = ShortcutsStatus::default();
    for (index, slot) in ShortcutSlot::ALL.into_iter().enumerate() {
        let registration = match &plans[index] {
            SlotPlan::Unbound => SlotRegistration::default(),
            SlotPlan::Invalid {
                accelerator,
                reason,
            } => SlotRegistration {
                accelerator: Some(accelerator.clone()),
                registered: false,
                error: Some(reason.clone()),
            },
            SlotPlan::Bound { accelerator, .. } => {
                match registrar.register(slot, accelerator, Arc::clone(handler)) {
                    Ok(()) => SlotRegistration {
                        accelerator: Some(accelerator.clone()),
                        registered: true,
                        error: None,
                    },
                    Err(reason) => SlotRegistration {
                        accelerator: Some(accelerator.clone()),
                        registered: false,
                        error: Some(reason),
                    },
                }
            }
        };

        match (&registration.accelerator, &registration.error) {
            (Some(accelerator), None) => {
                log::info!("{CMD} apply: {} registered on {accelerator}", slot.name())
            }
            (Some(accelerator), Some(reason)) => log::warn!(
                "{CMD} apply: {} is not registered — {accelerator}: {reason}",
                slot.name()
            ),
            (None, _) => log::info!("{CMD} apply: {} unbound", slot.name()),
        }
        status.set(slot, registration);
    }
    status
}

// ── effects ─────────────────────────────────────────────────────────────

/// The action a press performs, named after the existing behaviour it drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutEffect {
    PlaybackPlay,
    PlaybackPause,
    SyncStart,
    SyncStop,
}

/// The playback press's effect. `is_playing` is the API's answer at press
/// time; resuming is the direction that cannot destroy anything, so anything
/// other than "it is playing" resumes.
pub fn playback_effect(is_playing: bool) -> ShortcutEffect {
    if is_playing {
        ShortcutEffect::PlaybackPause
    } else {
        ShortcutEffect::PlaybackPlay
    }
}

/// The sync press's effect: a press toggles the live flag, so it pauses when
/// syncing and resumes when not.
pub fn sync_effect(is_syncing: bool) -> ShortcutEffect {
    if is_syncing {
        ShortcutEffect::SyncStop
    } else {
        ShortcutEffect::SyncStart
    }
}

/// Runs one effect. Kept as a function pointer table so the slot → command
/// mapping is a value a test can assert, instead of a `match` reached only
/// through a running app.
pub type EffectRunner = fn(&AppHandle);

/// The implementation of one effect.
pub fn runner_for(effect: ShortcutEffect) -> EffectRunner {
    match effect {
        ShortcutEffect::PlaybackPlay => run_playback_play,
        ShortcutEffect::PlaybackPause => run_playback_pause,
        ShortcutEffect::SyncStart => run_sync_start,
        ShortcutEffect::SyncStop => run_sync_stop,
    }
}

/// Runs `effect` through its runner.
fn run_effect(app: &AppHandle, effect: ShortcutEffect) {
    (runner_for(effect))(app)
}

/// The app's shared state, cloned out of the manager so it can move onto a
/// worker thread (`tauri::State` itself cannot outlive the call that made it).
fn state_arc(app: &AppHandle) -> Option<Arc<crate::AppState>> {
    app.try_state::<Arc<crate::AppState>>()
        .map(|state| state.inner().clone())
}

/// The persisted config, or the built-in defaults when the app has not loaded
/// one yet (registration runs inside setup, and tests have no state at all).
fn config_or_default(app: &AppHandle) -> AppConfig {
    // The config guard is cloned out inside the closure: `Config::get` returns
    // a read guard, which cannot leave the expression that created it.
    app.try_state::<Arc<crate::AppState>>()
        .and_then(|state| state.config.get().as_ref().cloned())
        .unwrap_or_default()
}

/// Runs a player action through the same refresh-aware policy the
/// `playback_play` / `playback_pause` commands and the tray use (issues
/// #375/#428/#586), so a shortcut can never call Spotify with a stale token,
/// and its failure wording is the commands'.
fn run_player(
    app: &AppHandle,
    label: &'static str,
    call: impl Fn(&str) -> Result<(), crate::spotify::SpotifyApiError> + Send + 'static,
) {
    let Some(state) = state_arc(app) else {
        log::warn!("{CMD} {label}: AppState is not registered yet, skipping");
        return;
    };
    let app = app.clone();
    // The shortcut thread must not block on HTTP (10 s timeouts), exactly as
    // the command handlers offload theirs.
    tauri::async_runtime::spawn_blocking(move || {
        match commands::playback::player_with_refresh(&state, &app, label, call) {
            Ok(()) => {
                log::info!("{CMD} {label}: SUCCESS");
                // Ask the shell to repaint from backend state: the tray's
                // label and marks are built from backend truth, and a shortcut
                // is a state change the tray did not initiate.
                crate::tray::refresh_tray_from_state(&app);
            }
            Err(e) => {
                log::warn!("{CMD} {label}: failed: {e}");
                // Same event the tray and the polling loop use for playback
                // failures, so the Dashboard surfaces a shortcut's failure the
                // way it surfaces a tray click's.
                let _ = app.emit("playback-error", e);
            }
        }
    });
}

fn run_playback_play(app: &AppHandle) {
    run_player(app, "playback_play", |token| {
        crate::spotify::player_play(token, None)
    });
}

fn run_playback_pause(app: &AppHandle) {
    run_player(app, "playback_pause", |token| {
        crate::spotify::player_pause(token, None)
    });
}

fn run_sync_start(app: &AppHandle) {
    let Some(state) = state_arc(app) else {
        log::warn!("{CMD} start_syncing: AppState is not registered yet, skipping");
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match commands::sync::start_syncing_with(state, &app).await {
            Ok(()) => {
                log::info!("{CMD} start_syncing: SUCCESS");
                crate::tray::refresh_tray_from_state(&app);
            }
            Err(e) => log::warn!("{CMD} start_syncing: failed: {e}"),
        }
    });
}

fn run_sync_stop(app: &AppHandle) {
    let Some(state) = state_arc(app) else {
        log::warn!("{CMD} stop_syncing: AppState is not registered yet, skipping");
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match commands::sync::stop_syncing_with(state, &app).await {
            Ok(()) => {
                log::info!("{CMD} stop_syncing: SUCCESS");
                crate::tray::refresh_tray_from_state(&app);
            }
            Err(e) => log::warn!("{CMD} stop_syncing: failed: {e}"),
        }
    });
}

/// The API's playing state for the playback toggle, or `None` when it cannot be
/// read — in which case the press does nothing rather than guessing (the tray's
/// play/pause item makes the same choice, and a read failure means Spotify is
/// unreachable, so the action would have failed too).
fn read_playing_state(app: &AppHandle) -> Option<bool> {
    let state = state_arc(app)?;
    // Unconditional GET (`None`): a user-initiated one-off press carries no
    // ETag validator, exactly as the tray's play/pause read.
    match commands::playback::player_with_refresh_typed(
        &state,
        app,
        "shortcut playback state",
        |token| crate::spotify::get_currently_playing(token, None),
    ) {
        Ok(crate::spotify::CurrentlyPlaying::Modified { now: Some(now), .. }) => {
            Some(now.media.is_playing)
        }
        // A 204 / ad / unknown item means nothing is playing: resume.
        Ok(_) => Some(false),
        Err(e) => {
            log::warn!("{CMD} toggle_playback: playback state read failed: {e}");
            let _ = app.emit("playback-error", e.to_string());
            None
        }
    }
}

/// Runs a binding's effect. Called from the plugin's event handler, which
/// global-hotkey runs on a thread of its own — never the UI thread — so the
/// Spotify read and every HTTP call are moved onto a worker first.
fn handle_press(app: &AppHandle, slot: ShortcutSlot) {
    match slot {
        ShortcutSlot::ToggleSync => {
            let is_syncing = app
                .try_state::<Arc<crate::AppState>>()
                .map(|state| state.polling.is_syncing(Ordering::Acquire))
                .unwrap_or(false);
            run_effect(app, sync_effect(is_syncing));
        }
        ShortcutSlot::TogglePlayback => {
            let app_owned = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                if let Some(is_playing) = read_playing_state(&app_owned) {
                    run_effect(&app_owned, playback_effect(is_playing));
                }
            });
        }
    }
}

/// The one press handler both registrations share.
fn press_handler() -> Arc<SlotHandler> {
    Arc::new(|app: &AppHandle, slot: ShortcutSlot| handle_press(app, slot))
}

// ── plugin plumbing ─────────────────────────────────────────────────────

/// The plugin-backed registrar. `GlobalShortcut` marshals every grab onto the
/// main thread itself (`run_on_main_thread`), so this stays a thin adapter.
struct PluginRegistrar<'a> {
    shortcuts: &'a GlobalShortcut<tauri::Wry>,
}

impl<'a> PluginRegistrar<'a> {
    fn new(app: &'a AppHandle) -> Self {
        Self {
            shortcuts: app.global_shortcut(),
        }
    }
}

impl ShortcutRegistrar for PluginRegistrar<'_> {
    fn register(
        &self,
        slot: ShortcutSlot,
        accelerator: &str,
        handler: Arc<SlotHandler>,
    ) -> Result<(), String> {
        self.shortcuts
            .on_shortcut(accelerator, move |app, _shortcut, event| {
                dispatch_event(slot, event.state, &|pressed| handler(app, pressed));
            })
            .map_err(|e| e.to_string())
    }

    fn unregister_all(&self) -> Result<(), String> {
        self.shortcuts.unregister_all().map_err(|e| e.to_string())
    }
}

/// The last registration pass's outcome, so a Settings pane opened later can
/// render a refusal that happened at startup.
#[derive(Default)]
pub struct ShortcutStatusState(parking_lot::Mutex<ShortcutsStatus>);

/// Records a pass's outcome, managing the state on first use.
fn store_status(app: &AppHandle, status: ShortcutsStatus) {
    if app.manage(ShortcutStatusState(parking_lot::Mutex::new(status.clone()))) {
        return;
    }
    // Already managed: a second pass (startup, then a fast Settings save) lost
    // the race above; the lock is the serialisation point.
    if let Some(state) = app.try_state::<ShortcutStatusState>() {
        *state.0.lock() = status;
    }
}

/// The last recorded registration status (all-unbound before the first pass).
pub fn status_of(app: &AppHandle) -> ShortcutsStatus {
    app.try_state::<ShortcutStatusState>()
        .map(|state| state.0.lock().clone())
        .unwrap_or_default()
}

/// Registers every configured binding, replacing whatever was registered
/// before. Called once from `lib.rs`'s setup and again on every Settings save.
/// Never fatal: a slot that cannot be bound records its reason and leaves the
/// other slot — and the app — alone.
pub fn register_from_config(app: &AppHandle) {
    let plans = plan_shortcuts(&config_or_default(app).shortcuts);
    let registrar = PluginRegistrar::new(app);
    let status = apply_plan(&registrar, &plans, &press_handler());
    store_status(app, status);
}

/// Releases every grab the app holds, leaving the config untouched.
///
/// The Settings card calls this while a row is capturing a combo: the currently
/// registered accelerator would otherwise fire its action mid-capture (the OS
/// delivers the key to the grab, not to the input element), which would make
/// re-recording an existing binding impossible. The returned status still names
/// each configured accelerator — it is simply not held right now.
pub fn release_all(app: &AppHandle) -> ShortcutsStatus {
    let registrar = PluginRegistrar::new(app);
    if let Err(e) = registrar.unregister_all() {
        log::warn!("{CMD} release: releasing the grabs failed: {e}");
    }
    let cfg = config_or_default(app);
    let mut status = ShortcutsStatus::default();
    for slot in ShortcutSlot::ALL {
        status.set(
            slot,
            SlotRegistration {
                accelerator: configured_binding(&cfg.shortcuts, slot),
                registered: false,
                error: None,
            },
        );
    }
    log::info!("{CMD} release: all grabs released (config unchanged)");
    store_status(app, status.clone());
    status
}

// ── commands ────────────────────────────────────────────────────────────

/// Re-registers the persisted bindings and reports what is live now.
///
/// Reads the *persisted* config rather than taking accelerators from the
/// caller: the config is the single source of truth both windows agree on, so a
/// detached Settings pane cannot register a binding the main window has not
/// saved (nor leave the two disagreeing).
#[tauri::command]
pub fn register_shortcuts(app: AppHandle) -> ShortcutsStatus {
    register_from_config(&app);
    status_of(&app)
}

/// Releases every grab the app holds and reports the released state — see
/// [`release_all`] for why the Settings capture path needs it.
#[tauri::command]
pub fn unregister_shortcuts(app: AppHandle) -> ShortcutsStatus {
    release_all(&app)
}

/// Validates one accelerator for one slot, so the Settings field can name the
/// reason a combo is rejected instead of only refusing to save it.
///
/// `action` is the slot's wire name ([`ShortcutSlot::name`]). The *other* slot's
/// binding is resolved from the persisted config, never taken from the caller,
/// so a pane cannot validate against a binding that is not in force.
#[tauri::command]
pub fn validate_shortcut(
    accelerator: String,
    action: String,
    app: AppHandle,
) -> Result<(), String> {
    let slot = ShortcutSlot::from_name(&action)
        .ok_or_else(|| format!("Unknown shortcut action \"{action}\""))?;
    let cfg = config_or_default(&app);
    let other = configured_binding(&cfg.shortcuts, slot.other());
    validate_accelerator(slot, &accelerator, other.as_deref()).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stands in for the plugin: records what was registered, and can refuse
    /// selected accelerators — the desktop/compositor refusal this slice exists
    /// to surface.
    #[derive(Default)]
    struct RecordingRegistrar {
        registrations: parking_lot::Mutex<Vec<(ShortcutSlot, String)>>,
        releases: parking_lot::Mutex<usize>,
        refuse: Vec<&'static str>,
    }

    impl RecordingRegistrar {
        /// A registrar that refuses `refuse`, as a desktop that already has the
        /// combo taken (or cannot grab it) does.
        fn refusing(refuse: &[&'static str]) -> Self {
            Self {
                refuse: refuse.to_vec(),
                ..Self::default()
            }
        }

        fn registered(&self) -> Vec<(ShortcutSlot, String)> {
            self.registrations.lock().clone()
        }

        fn release_count(&self) -> usize {
            *self.releases.lock()
        }
    }

    impl ShortcutRegistrar for RecordingRegistrar {
        fn register(
            &self,
            slot: ShortcutSlot,
            accelerator: &str,
            _handler: Arc<SlotHandler>,
        ) -> Result<(), String> {
            if self.refuse.contains(&accelerator) {
                return Err(format!("accelerator {accelerator} already in use"));
            }
            self.registrations
                .lock()
                .push((slot, accelerator.to_string()));
            Ok(())
        }

        fn unregister_all(&self) -> Result<(), String> {
            *self.releases.lock() += 1;
            self.registrations.lock().clear();
            Ok(())
        }
    }

    fn handler() -> Arc<SlotHandler> {
        Arc::new(|_app: &AppHandle, _slot: ShortcutSlot| {})
    }

    fn cfg(playback: Option<&str>, sync: Option<&str>) -> ShortcutsConfig {
        ShortcutsConfig {
            toggle_playback: playback.map(str::to_string),
            toggle_sync: sync.map(str::to_string),
        }
    }

    // ── validation ──────────────────────────────────────────────────────

    #[test]
    fn unparsable_accelerator_is_rejected_by_name() {
        let err = validate_accelerator(ShortcutSlot::TogglePlayback, "NotAKey+Alt", None)
            .expect_err("a key the plugin parser cannot read must be rejected");
        assert!(
            err.contains("not a recognised shortcut"),
            "the reason must name the failure, got: {err}"
        );
        assert!(
            err.contains("NotAKey"),
            "the reason must quote the offending accelerator, got: {err}"
        );
    }

    #[test]
    fn conflicting_accelerator_is_rejected_against_the_other_slot() {
        let err = validate_accelerator(
            ShortcutSlot::ToggleSync,
            "CmdOrCtrl+Alt+P",
            Some("CmdOrCtrl+Alt+P"),
        )
        .expect_err("both slots cannot share one accelerator");
        assert!(
            err.contains("toggle_playback"),
            "the reason must name the slot that already holds it, got: {err}"
        );
    }

    /// The collision test is on the *parsed* accelerator, not the text: a user
    /// who typed a different spelling of the same combination still collides.
    #[test]
    fn conflict_detection_compares_parsed_identities() {
        assert!(
            validate_accelerator(
                ShortcutSlot::ToggleSync,
                "control + alt + p",
                Some("Ctrl+Alt+P")
            )
            .is_err(),
            "whitespace and spelling do not make a different accelerator"
        );
        assert!(
            validate_accelerator(
                ShortcutSlot::ToggleSync,
                "Ctrl+Alt+S",
                Some("CmdOrCtrl+Alt+P")
            )
            .is_ok(),
            "distinct accelerators must validate"
        );
    }

    /// The other slot's binding being unreadable is *its* problem: it registers
    /// nothing, so it cannot be the reason this slot is refused.
    #[test]
    fn an_unparsable_other_binding_does_not_block_this_one() {
        assert!(
            validate_accelerator(
                ShortcutSlot::ToggleSync,
                "CmdOrCtrl+Alt+S",
                Some("this-is-not-a-key")
            )
            .is_ok(),
            "a broken binding in the other slot must not block a valid one"
        );
    }

    // ── planning ────────────────────────────────────────────────────────

    #[test]
    fn blank_and_null_bindings_leave_the_other_slot_bound() {
        for blank in [None, Some(""), Some("   ")] {
            let plans = plan_shortcuts(&cfg(blank, Some("CmdOrCtrl+Alt+S")));
            assert_eq!(
                plans[0],
                SlotPlan::Unbound,
                "an absent/blank/whitespace binding means unbound ({blank:?})"
            );
            assert!(
                matches!(&plans[1], SlotPlan::Bound { accelerator, .. } if accelerator == "CmdOrCtrl+Alt+S"),
                "clearing one slot must not drop the other's binding, got {:?}",
                plans[1]
            );
        }
    }

    #[test]
    fn an_unparsable_binding_marks_only_its_own_slot() {
        let plans = plan_shortcuts(&cfg(Some("NotAKey"), Some("CmdOrCtrl+Alt+S")));
        match &plans[0] {
            SlotPlan::Invalid {
                accelerator,
                reason,
            } => {
                assert_eq!(accelerator, "NotAKey");
                assert!(!reason.is_empty(), "an invalid slot must carry a reason");
            }
            other => panic!("expected the bad slot to be Invalid, got {other:?}"),
        }
        assert!(
            matches!(&plans[1], SlotPlan::Bound { .. }),
            "the other slot must still be registered, got {:?}",
            plans[1]
        );
    }

    #[test]
    fn both_slots_sharing_one_accelerator_register_neither() {
        let plans = plan_shortcuts(&cfg(Some("CmdOrCtrl+Alt+P"), Some("CmdOrCtrl+Alt+P")));
        for plan in &plans {
            assert!(
                matches!(plan, SlotPlan::Invalid { .. }),
                "a shared accelerator must bind neither slot, got {plan:?}"
            );
        }
        let registrar = RecordingRegistrar::default();
        let status = apply_plan(&registrar, &plans, &handler());
        assert!(registrar.registered().is_empty());
        assert!(!status.toggle_playback.registered);
        assert!(!status.toggle_sync.registered);
    }

    // ── applying ────────────────────────────────────────────────────────

    #[test]
    fn apply_registers_every_configured_binding_in_slot_order() {
        let plans = plan_shortcuts(&ShortcutsConfig::default());
        let registrar = RecordingRegistrar::default();
        let status = apply_plan(&registrar, &plans, &handler());

        assert_eq!(
            registrar.registered(),
            vec![
                (ShortcutSlot::TogglePlayback, "CmdOrCtrl+Alt+P".to_string()),
                (ShortcutSlot::ToggleSync, "CmdOrCtrl+Alt+S".to_string()),
            ],
            "the defaults must be registered, in slot order"
        );
        assert_eq!(
            status.toggle_playback,
            SlotRegistration {
                accelerator: Some("CmdOrCtrl+Alt+P".to_string()),
                registered: true,
                error: None,
            }
        );
        assert!(status.toggle_sync.registered);
    }

    /// The acceptance case for a desktop that refuses a grab (a Wayland
    /// compositor, or a combo another application already owns): the refusal is
    /// *reported*, and the other binding is untouched.
    #[test]
    fn a_refused_registration_is_reported_and_leaves_the_other_slot_working() {
        let plans = plan_shortcuts(&ShortcutsConfig::default());
        let registrar = RecordingRegistrar::refusing(&["CmdOrCtrl+Alt+P"]);
        let status = apply_plan(&registrar, &plans, &handler());

        let refused = &status.toggle_playback;
        assert!(
            !refused.registered,
            "a refused grab must not be reported as live"
        );
        assert!(
            refused
                .error
                .as_deref()
                .is_some_and(|e| e.contains("already in use")),
            "the refusal must carry the plugin's reason, got {:?}",
            refused.error
        );
        assert_eq!(
            refused.accelerator.as_deref(),
            Some("CmdOrCtrl+Alt+P"),
            "the card must still be able to show what the user configured"
        );
        assert!(
            status.toggle_sync.registered && status.toggle_sync.error.is_none(),
            "one refused slot must not take the other binding down, got {:?}",
            status.toggle_sync
        );
        assert_eq!(
            registrar.registered(),
            vec![(ShortcutSlot::ToggleSync, "CmdOrCtrl+Alt+S".to_string())],
            "only the refused slot stays unregistered"
        );
    }

    /// A re-apply (every Settings save) must not leave the previous accelerator
    /// live: the plugin rejects a duplicate grab, and a stale grab would keep
    /// firing the old action.
    #[test]
    fn reapply_releases_the_previous_grabs_first() {
        let registrar = RecordingRegistrar::default();
        apply_plan(
            &registrar,
            &plan_shortcuts(&cfg(Some("CmdOrCtrl+Alt+P"), None)),
            &handler(),
        );
        assert_eq!(registrar.release_count(), 1);

        let status = apply_plan(
            &registrar,
            &plan_shortcuts(&cfg(Some("CmdOrCtrl+Shift+P"), None)),
            &handler(),
        );
        assert_eq!(registrar.release_count(), 2, "each apply releases first");
        assert_eq!(
            registrar.registered(),
            vec![(
                ShortcutSlot::TogglePlayback,
                "CmdOrCtrl+Shift+P".to_string()
            )],
            "the replaced accelerator must not still be registered"
        );
        assert!(status.toggle_sync.accelerator.is_none());
        assert!(!status.toggle_sync.registered);
    }

    // ── dispatch ────────────────────────────────────────────────────────

    #[test]
    fn only_the_press_edge_dispatches() {
        let seen = std::cell::RefCell::new(Vec::new());
        assert!(
            dispatch_event(ShortcutSlot::ToggleSync, ShortcutState::Pressed, &|slot| {
                seen.borrow_mut().push(slot)
            }),
            "the press edge runs the binding"
        );
        assert_eq!(*seen.borrow(), vec![ShortcutSlot::ToggleSync]);

        assert!(
            !dispatch_event(ShortcutSlot::ToggleSync, ShortcutState::Released, &|slot| {
                seen.borrow_mut().push(slot)
            }),
            "the release edge must not run the binding (it would toggle twice)"
        );
        assert_eq!(seen.borrow().len(), 1, "no dispatch happened on release");
    }

    #[test]
    fn playback_effect_resumes_unless_the_api_says_playing() {
        assert_eq!(
            playback_effect(true),
            ShortcutEffect::PlaybackPause,
            "a playing track is paused by the toggle"
        );
        assert_eq!(
            playback_effect(false),
            ShortcutEffect::PlaybackPlay,
            "anything else resumes — the direction that cannot destroy anything"
        );
    }

    #[test]
    fn sync_effect_toggles_the_live_flag() {
        assert_eq!(sync_effect(true), ShortcutEffect::SyncStop);
        assert_eq!(sync_effect(false), ShortcutEffect::SyncStart);
    }

    /// The slot → existing-command mapping is the thing a shortcut must not get
    /// wrong (a shortcut that starts sync when it means to stop it is
    /// user-visible), so pin it by resolved runner. The comparisons go through
    /// the code address because a function *item* cannot be cast to an integer
    /// directly and a fn-pointer `==` is lint-mooted.
    #[test]
    fn effect_runners_are_the_expected_implementations() {
        fn addr(runner: EffectRunner) -> usize {
            runner as usize
        }
        for (effect, expected) in [
            (
                ShortcutEffect::PlaybackPlay,
                run_playback_play as EffectRunner,
            ),
            (
                ShortcutEffect::PlaybackPause,
                run_playback_pause as EffectRunner,
            ),
            (ShortcutEffect::SyncStart, run_sync_start as EffectRunner),
            (ShortcutEffect::SyncStop, run_sync_stop as EffectRunner),
        ] {
            assert_eq!(
                addr(runner_for(effect)),
                addr(expected),
                "{effect:?} must resolve to its own implementation"
            );
        }
    }

    /// Brace-counted body isolation (house style — never boundary anchors,
    /// which drift).
    fn fn_body<'a>(prod_source: &'a str, sig: &str) -> &'a str {
        let after_sig = prod_source
            .split(sig)
            .nth(1)
            .unwrap_or_else(|| panic!("shortcuts.rs has no `{sig}`"));
        let open = after_sig
            .find('{')
            .unwrap_or_else(|| panic!("{sig} has no opening brace"));
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
        &after_sig[..end.unwrap_or_else(|| panic!("{sig} body never closed"))]
    }

    /// Issue #676: every runner must go through the *existing* implementation —
    /// the shared refresh-aware player policy for playback, and
    /// `commands::sync`'s lifecycle for sync. A runner that grew its own HTTP
    /// call or its own claim/emit sequence would be a second implementation
    /// that can drift from the commands and the tray.
    #[test]
    fn runners_delegate_to_the_existing_implementations() {
        let source = include_str!("shortcuts.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("shortcuts.rs has no #[cfg(test)] mod tests block");

        let play = fn_body(prod_source, "fn run_playback_play(");
        assert!(
            play.contains("run_player("),
            "run_playback_play must route through the shared player helper"
        );
        let pause = fn_body(prod_source, "fn run_playback_pause(");
        assert!(
            pause.contains("run_player("),
            "run_playback_pause must route through the shared player helper"
        );
        let player = fn_body(prod_source, "fn run_player(");
        assert!(
            player.contains("commands::playback::player_with_refresh("),
            "run_player must use the commands' refresh-aware policy"
        );
        assert!(
            !player.contains("state.tokens.spotify()"),
            "run_player must never snapshot the token itself (issues #375/#428)"
        );

        let start = fn_body(prod_source, "fn run_sync_start(");
        assert!(
            start.contains("commands::sync::start_syncing_with("),
            "run_sync_start must drive the existing sync lifecycle"
        );
        let stop = fn_body(prod_source, "fn run_sync_stop(");
        assert!(
            stop.contains("commands::sync::stop_syncing_with("),
            "run_sync_stop must drive the existing sync lifecycle"
        );
        for (sig, forbidden) in [
            ("fn run_sync_start(", "try_claim()"),
            ("fn run_sync_stop(", "stop_polling_and_join("),
        ] {
            assert!(
                !fn_body(prod_source, sig).contains(forbidden),
                "{sig} must not re-implement the polling lifecycle ({forbidden})"
            );
        }
    }

    /// The read that decides the playback direction goes through the same
    /// refresh-aware policy as the tray's play/pause item, and a failed read
    /// performs no action at all.
    #[test]
    fn a_failed_playback_state_read_performs_no_action() {
        let source = include_str!("shortcuts.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("shortcuts.rs has no #[cfg(test)] mod tests block");
        let body = fn_body(prod_source, "fn read_playing_state(");
        assert!(
            body.contains("player_with_refresh_typed("),
            "the playing-state read must use the shared refresh-aware policy"
        );
        assert!(
            body.contains("None"),
            "an unreadable state must be None, never a guessed direction"
        );
        let press = fn_body(prod_source, "fn handle_press(");
        assert!(
            press.contains("if let Some(is_playing) = read_playing_state("),
            "the toggle must do nothing when the state could not be read"
        );
    }
}
