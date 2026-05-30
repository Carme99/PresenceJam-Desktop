use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use parking_lot::RwLock;
use tauri::{Manager, Emitter, AppHandle};
use tauri_plugin_store::StoreExt;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PendingSpotifyAuth {
    pub verifier: String,
    pub state: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PendingTeamsAuth {
    pub verifier: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub struct AppState {
    pub config: RwLock<Option<crate::config::AppConfig>>,
    pub spotify_tokens: RwLock<Option<crate::spotify::SpotifyTokens>>,
    pub teams_tokens: RwLock<Option<crate::teams::TeamsTokens>>,
    pub current_track: RwLock<Option<crate::spotify::TrackInfo>>,
    pub is_syncing: RwLock<bool>,
    pub polling_handle: RwLock<Option<thread::JoinHandle<()>>>,
    pub pending_spotify_auth: RwLock<Option<PendingSpotifyAuth>>,
    pub pending_teams_auth: RwLock<Option<PendingTeamsAuth>>,
    pub stop_tx: RwLock<Option<mpsc::Sender<()>>>,
}

impl AppState {
    pub fn new() -> Self {
        log::info!("[APP_STATE] AppState::new: creating new AppState");
        Self {
            config: RwLock::new(None),
            spotify_tokens: RwLock::new(None),
            teams_tokens: RwLock::new(None),
            current_track: RwLock::new(None),
            is_syncing: RwLock::new(false),
            polling_handle: RwLock::new(None),
            pending_spotify_auth: RwLock::new(None),
            pending_teams_auth: RwLock::new(None),
            stop_tx: RwLock::new(None),
        }
    }
}

pub mod config;
pub mod profanity;
pub mod spotify;
pub mod teams;
pub mod polling;
pub mod tray;
pub mod menu;
pub mod commands;

async fn handle_spotify_callback(code: &str, state_param: Option<&str>, app: &AppHandle) -> Result<(), String> {
    log::info!("[CALLBACK] handle_spotify_callback: ENTRY - code.len={}", code.len());

    let app_state = app.state::<Arc<AppState>>();
    log::info!("[CALLBACK] handle_spotify_callback: got app state");

    let pending = {
        let mut guard = app_state.pending_spotify_auth.write();
        log::info!("[CALLBACK] handle_spotify_callback: taking pending Spotify auth from state");
        let pending = guard.take().ok_or_else(|| {
            log::error!("[CALLBACK] handle_spotify_callback: No pending Spotify auth found");
            "No pending Spotify auth".to_string()
        })?;
        // Check if pending auth has expired
        if chrono::Utc::now() > pending.expires_at {
            log::warn!("[CALLBACK] handle_spotify_callback: pending Spotify auth expired");
            return Err("Spotify auth expired. Please try again.".to_string());
        }
        pending
    };
    log::debug!("[CALLBACK] handle_spotify_callback: pending auth found - verifier.len={}", pending.verifier.len());

    // Verify state matches to prevent CSRF attacks
    if let Some(state_str) = state_param {
        if state_str != pending.state {
            log::error!("[CALLBACK] handle_spotify_callback: state mismatch - CSRF attack detected");
            return Err("State mismatch - possible CSRF attack".to_string());
        }
        log::info!("[CALLBACK] handle_spotify_callback: state verified successfully");
    } else {
        log::error!("[CALLBACK] handle_spotify_callback: missing state parameter in callback URL");
        return Err("Missing state parameter - possible CSRF attack".to_string());
    }
    
    log::info!("[CALLBACK] handle_spotify_callback: calling complete_spotify_auth");
    let tokens = crate::spotify::complete_spotify_auth(
        code,
        &pending.verifier,
        &pending.client_id,
        &pending.client_secret,
        &pending.redirect_uri,
    )?;
    log::debug!("[CALLBACK] handle_spotify_callback: token exchange successful - access_token.len={}", tokens.access_token.len());
    
    log::info!("[CALLBACK] handle_spotify_callback: saving tokens to store");
    crate::polling::save_spotify_tokens(app, &tokens)?;
    
    {
        let mut guard = app_state.spotify_tokens.write();
        *guard = Some(tokens.clone());
        log::info!("[CALLBACK] handle_spotify_callback: tokens stored in AppState");
    }
    
    log::info!("[CALLBACK] handle_spotify_callback: EMIT spotify-auth-complete event");
    let _ = app.emit("spotify-auth-complete", ());
    
    log::info!("[CALLBACK] handle_spotify_callback: SUCCESS");
    Ok(())
}

async fn handle_teams_callback(code: &str, app: &AppHandle) -> Result<(), String> {
    log::info!("[CALLBACK] handle_teams_callback: ENTRY - code.len={}", code.len());
    
    let state = app.state::<Arc<AppState>>();
    log::info!("[CALLBACK] handle_teams_callback: got app state");
    
    let pending = {
        let mut guard = state.pending_teams_auth.write();
        log::info!("[CALLBACK] handle_teams_callback: taking pending Teams auth from state");
        let pending = guard.take().ok_or_else(|| {
            log::error!("[CALLBACK] handle_teams_callback: No pending Teams auth found");
            "No pending Teams auth".to_string()
        })?;
        // Check if pending auth has expired
        if chrono::Utc::now() > pending.expires_at {
            log::warn!("[CALLBACK] handle_teams_callback: pending Teams auth expired");
            return Err("Teams auth expired. Please try again.".to_string());
        }
        pending
    };
    log::info!("[CALLBACK] handle_teams_callback: pending auth found");
    
    log::info!("[CALLBACK] handle_teams_callback: calling complete_teams_auth");
    let tokens = crate::teams::complete_teams_auth(
        code,
        &pending.verifier,
        &pending.client_id,
        &pending.redirect_uri,
    )?;
    log::debug!("[CALLBACK] handle_teams_callback: token exchange successful - access_token.len={}", tokens.access_token.len());
    
    log::info!("[CALLBACK] handle_teams_callback: saving tokens to store");
    crate::polling::save_teams_tokens(app, &tokens)?;
    
    {
        let mut guard = state.teams_tokens.write();
        *guard = Some(tokens);
        log::info!("[CALLBACK] handle_teams_callback: tokens stored in AppState");
    }
    
    log::info!("[CALLBACK] handle_teams_callback: EMIT teams-auth-complete event");
    let _ = app.emit("teams-auth-complete", ());
    
    log::info!("[CALLBACK] handle_teams_callback: SUCCESS");
    Ok(())
}

fn handle_deep_link(url: &str, app: AppHandle) {
    log::info!("[DEEP_LINK] handle_deep_link: ENTRY - url={}", url);
    
    if let Ok(parsed) = url::Url::parse(url) {
        log::info!("[DEEP_LINK] handle_deep_link: URL parsed successfully");
        let scheme = parsed.scheme();
        log::info!("[DEEP_LINK] handle_deep_link: scheme={}", scheme);
        
        if scheme == "presencejam" {
            log::info!("[DEEP_LINK] handle_deep_link: recognized as presencejam scheme");
            let path = parsed.path();
            log::info!("[DEEP_LINK] handle_deep_link: path={}", path);
            
            let code = parsed.query_pairs().find(|(k, _)| k == "code").map(|(_, v)| v.to_string());
            let state_param = parsed.query_pairs().find(|(k, _)| k == "state").map(|(_, v)| v.to_string());

            if let Some(code_str) = code {
                log::info!("[DEEP_LINK] handle_deep_link: code found - code.len={}", code_str.len());
                let app_clone = app.clone();
                let code_clone = code_str.clone();
                let state_clone = state_param.clone();

                if path == "/teams-callback" {
                    log::info!("[DEEP_LINK] handle_deep_link: routing to Teams callback");
                    tauri::async_runtime::spawn(async move {
                        log::info!("[DEEP_LINK] handle_deep_link: spawning Teams callback handler");
                        if let Err(e) = handle_teams_callback(&code_clone, &app_clone).await {
                            log::error!("[DEEP_LINK] handle_teams_callback: FAILED - {}", e);
                            log::info!("[DEEP_LINK] handle_deep_link: EMIT teams-auth-failed event");
                            let _ = app_clone.emit("teams-auth-failed", e);
                        }
                    });
                } else {
                    log::info!("[DEEP_LINK] handle_deep_link: routing to Spotify callback");
                    tauri::async_runtime::spawn(async move {
                        log::info!("[DEEP_LINK] handle_deep_link: spawning Spotify callback handler");
                        if let Err(e) = handle_spotify_callback(&code_clone, state_clone.as_deref(), &app_clone).await {
                            log::error!("[DEEP_LINK] handle_spotify_callback: FAILED - {}", e);
                            log::info!("[DEEP_LINK] handle_deep_link: EMIT spotify-auth-failed event");
                            let _ = app_clone.emit("spotify-auth-failed", e);
                        }
                    });
                }
            } else {
                log::warn!("[DEEP_LINK] handle_deep_link: no code found in URL");
            }
        } else {
            log::warn!("[DEEP_LINK] handle_deep_link: unknown scheme - {}", scheme);
        }
    } else {
        log::error!("[DEEP_LINK] handle_deep_link: failed to parse URL");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    log::info!("[APP] run: ENTRY");
    
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        use tauri_plugin_single_instance::init as single_instance_init;

        builder = builder.plugin(single_instance_init(|_app, argv, _cwd| {
            log::info!("[APP] single_instance: New instance opened with argv: {:?}", argv);
        }));

        builder = builder.plugin(tauri_plugin_deep_link::init());
        log::info!("[APP] run: deep_link plugin registered");
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_log::Builder::new()
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::Stdout,
            ))
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::LogDir { file_name: Some("PresenceJam".into()) },
            ))
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::Webview,
            ))
            .build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Set panic hook to log crashes
            std::panic::set_hook(Box::new(|panic_info| {
                let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                
                let location = if let Some(loc) = panic_info.location() {
                    format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
                } else {
                    "unknown location".to_string()
                };
                
                log::error!("[PANIC] {} at {}", msg, location);
                eprintln!("[PANIC] {} at {}", msg, location);
            }));
            

            log::info!("[APP] setup: ENTRY");
            
            let state = Arc::new(AppState::new());
            app.manage(state.clone());
            log::info!("[APP] setup: AppState created and managed");

            // Load config into AppState
            match config::load_config() {
                Ok(cfg) => {
                    let mut config_guard = state.config.write();
                    *config_guard = Some(cfg.clone());
                    log::info!("[APP] setup: config loaded into AppState");

                    // Handle start_minimized setting
                    if cfg.teams.start_minimized {
                        log::info!("[APP] setup: start_minimized enabled, hiding window");
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                }
                Err(e) => {
                    log::warn!("[APP] setup: no config found: {}", e);
                }
            }

            // Load Spotify tokens into AppState
            match polling::load_spotify_tokens(app.handle()) {
                Ok(Some(tokens)) => {
                    let mut guard = state.spotify_tokens.write();
                    *guard = Some(tokens);
                    log::info!("[APP] setup: spotify_tokens loaded into AppState");
                }
                Ok(None) => {
                    log::info!("[APP] setup: no spotify_tokens found");
                }
                Err(e) => {
                    log::warn!("[APP] setup: failed to load spotify_tokens: {}", e);
                }
            }

            // Load Teams tokens into AppState
            match polling::load_teams_tokens(app.handle()) {
                Ok(Some(tokens)) => {
                    let mut guard = state.teams_tokens.write();
                    *guard = Some(tokens);
                    log::info!("[APP] setup: teams_tokens loaded into AppState");
                }
                Ok(None) => {
                    log::info!("[APP] setup: no teams_tokens found");
                }
                Err(e) => {
                    log::warn!("[APP] setup: failed to load teams_tokens: {}", e);
                }
            }

            // Load pending Teams auth from store (for crash recovery during device code flow)
            {
                let store = app.store("tokens").map_err(|e| {
                    log::warn!("[APP] setup: failed to open store for pending teams auth: {}", e);
                    e
                });
                if let Ok(store) = store {
                    if let Some(value) = store.get("pending_teams_auth") {
                        if let Ok(pending) = serde_json::from_value::<crate::PendingTeamsAuth>(value.clone()) {
                            // Check if not expired
                            if pending.expires_at > chrono::Utc::now() {
                                let mut guard = state.pending_teams_auth.write();
                                *guard = Some(pending);
                                log::info!("[APP] setup: pending_teams_auth loaded from store (not expired)");
                            } else {
                                log::info!("[APP] setup: pending_teams_auth expired, clearing from store");
                                store.delete("pending_teams_auth");
                                let _ = store.save();
                            }
                        }
                    }
                }
            }

            // Load pending Spotify auth from store (for crash recovery during PKCE flow)
            {
                let store = app.store("tokens").map_err(|e| {
                    log::warn!("[APP] setup: failed to open store for pending spotify auth: {}", e);
                    e
                });
                if let Ok(store) = store {
                    if let Some(value) = store.get("pending_spotify_auth") {
                        if let Ok(pending) = serde_json::from_value::<crate::PendingSpotifyAuth>(value.clone()) {
                            // Check if not expired
                            if pending.expires_at > chrono::Utc::now() {
                                let mut guard = state.pending_spotify_auth.write();
                                *guard = Some(pending);
                                log::info!("[APP] setup: pending_spotify_auth loaded from store (not expired)");
                            } else {
                                log::info!("[APP] setup: pending_spotify_auth expired, clearing from store");
                                store.delete("pending_spotify_auth");
                                let _ = store.save();
                            }
                        }
                    }
                }
            }

            #[cfg(desktop)]
            {
                use tauri_plugin_deep_link::DeepLinkExt;

                #[cfg(windows)]
                {
                    log::info!("[APP] setup: registering deep links");
                    if let Err(e) = app.deep_link().register_all() {
                        log::error!("[APP] setup: Failed to register deep links: {}", e);
                    } else {
                        log::info!("[APP] setup: deep links registered successfully");
                    }
                }

                // Clean up stale temp files from previous crashes
                log::info!("[APP] setup: cleaning up stale config temp files");
                if let Ok(config_dir) = crate::config::config_dir() {
                    if let Ok(entries) = std::fs::read_dir(&config_dir) {
                        for entry in entries.flatten() {
                            if entry
                                .path()
                                .extension()
                                .map_or(false, |e| e == "tmp")
                            {
                                let _ = std::fs::remove_file(entry.path());
                                log::info!(
                                    "[APP] setup: removed stale temp file: {}",
                                    entry.path().display()
                                );
                            }
                        }
                    }
                }

                // Setup system tray
                log::info!("[APP] setup: setting up system tray");
                if let Err(e) = tray::setup_tray(app) {
                    log::error!("[APP] setup: Failed to setup system tray: {}", e);
                } else {
                    log::info!("[APP] setup: System tray initialized successfully");
                }

                // Setup application menu bar using window menu (not app menu)
                // This ensures click events are properly routed via on_menu_event
                log::info!("[APP] setup: setting up application menu");
                if let Some(window) = app.get_webview_window("main") {
                    if let Err(e) = menu::setup_app_menu(app, &window) {
                        log::error!("[APP] setup: Failed to setup application menu: {}", e);
                    } else {
                        log::info!("[APP] setup: Application menu initialized successfully");
                    }

                    // Register menu event handler on the window
                    // This is critical for macOS - window menus receive click events properly
                    let app_handle = app.handle().clone();
                    log::info!("[APP] setup: registering menu event handler on window");
                    window.on_menu_event(move |_app, event| {
                        let id = event.id().as_ref();
                        log::info!("[APP] window.on_menu_event: id={}", id);
                        menu::handle_app_menu_event(&app_handle, id);
                    });
                } else {
                    log::error!("[APP] setup: could not get main window for menu");
                }

                // Check for deep links on startup
                let start_urls = app.deep_link().get_current();
                log::info!("[APP] setup: checking for start URLs");
                if let Ok(Some(urls)) = start_urls {
                    log::info!("[APP] setup: found {} start URL(s)", urls.len());
                    for url in urls {
                        log::info!("[APP] setup: processing start URL: {}", url);
                        handle_deep_link(url.as_str(), app.handle().clone());
                    }
                } else {
                    log::info!("[APP] setup: no start URLs found");
                }

                // Register deep link callback
                let app_handle = app.handle().clone();
                log::info!("[APP] setup: registering on_open_url callback");
                app.deep_link().on_open_url(move |event| {
                    let urls = event.urls();
                    log::info!("[APP] on_open_url: received {} URL(s)", urls.len());
                    for url in urls {
                        log::info!("[APP] on_open_url: processing URL: {}", url);
                        handle_deep_link(url.as_str(), app_handle.clone());
                    }
                });
            }

            log::info!("[APP] setup: PresenceJam 2.0 started successfully");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_config,
            commands::save_config,
            commands::get_config_dir,
            commands::start_spotify_auth,
            commands::complete_spotify_auth,
            commands::complete_spotify_auth_manual,
            commands::get_spotify_tokens,
            commands::refresh_spotify,
            commands::start_teams_auth_device_code,
            commands::poll_teams_auth,
            commands::get_teams_tokens,
            commands::refresh_teams,
            commands::start_syncing,
            commands::stop_syncing,
            commands::get_sync_status,
            commands::show_window,
            commands::hide_window,
            commands::get_autostart_enabled,
            commands::set_autostart_enabled,
            commands::open_logs_folder,
            commands::open_external,
            commands::open_external_url,
            commands::get_current_track,
            commands::is_onboarding_complete,
            commands::complete_onboarding,
            commands::reconnect_spotify,
            commands::reconnect_teams,
            commands::app_exit,
            commands::update_tray_menu_state,
            commands::get_recent_logs,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                log::info!("[APP] window_event: CloseRequested received, hiding window");
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
