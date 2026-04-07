use std::sync::Arc;
use parking_lot::RwLock;
use tauri::{Manager, Emitter, AppHandle};

#[derive(Debug, Clone)]
pub struct PendingSpotifyAuth {
    pub verifier: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

#[derive(Debug, Clone)]
pub struct PendingTeamsAuth {
    pub verifier: String,
    pub client_id: String,
    pub redirect_uri: String,
}

pub struct AppState {
    pub config: RwLock<Option<crate::config::AppConfig>>,
    pub spotify_tokens: RwLock<Option<crate::spotify::SpotifyTokens>>,
    pub teams_tokens: RwLock<Option<crate::teams::TeamsTokens>>,
    pub current_track: RwLock<Option<crate::spotify::TrackInfo>>,
    pub is_syncing: RwLock<bool>,
    pub polling_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
    pub pending_spotify_auth: RwLock<Option<PendingSpotifyAuth>>,
    pub pending_teams_auth: RwLock<Option<PendingTeamsAuth>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: RwLock::new(None),
            spotify_tokens: RwLock::new(None),
            teams_tokens: RwLock::new(None),
            current_track: RwLock::new(None),
            is_syncing: RwLock::new(false),
            polling_handle: RwLock::new(None),
            pending_spotify_auth: RwLock::new(None),
            pending_teams_auth: RwLock::new(None),
        }
    }
}

pub mod config;
pub mod spotify;
pub mod teams;
pub mod polling;
pub mod tray;
pub mod commands;

async fn handle_spotify_callback(code: &str, app: &AppHandle) -> Result<(), String> {
    let state = app.state::<Arc<AppState>>();
    
    let pending = {
        let mut guard = state.pending_spotify_auth.write();
        guard.take().ok_or("No pending Spotify auth")?
    };
    
    log::info!("Completing Spotify auth with code");
    
    let tokens = crate::spotify::complete_spotify_auth(
        code,
        &pending.verifier,
        &pending.client_id,
        &pending.client_secret,
        &pending.redirect_uri,
    )?;
    
    crate::polling::save_spotify_tokens(app, &tokens)?;
    
    {
        let mut guard = state.spotify_tokens.write();
        *guard = Some(tokens);
    }
    
    let _ = app.emit("spotify-auth-complete", ());
    
    log::info!("Spotify auth completed via deep link");
    Ok(())
}

async fn handle_teams_callback(code: &str, app: &AppHandle) -> Result<(), String> {
    let state = app.state::<Arc<AppState>>();
    
    let pending = {
        let mut guard = state.pending_teams_auth.write();
        guard.take().ok_or("No pending Teams auth")?
    };
    
    log::info!("Completing Teams auth with code");
    
    let tokens = crate::teams::complete_teams_auth(
        code,
        &pending.verifier,
        &pending.client_id,
        &pending.redirect_uri,
    )?;
    
    crate::polling::save_teams_tokens(app, &tokens)?;
    
    {
        let mut guard = state.teams_tokens.write();
        *guard = Some(tokens);
    }
    
    let _ = app.emit("teams-auth-complete", ());
    
    log::info!("Teams auth completed via deep link");
    Ok(())
}

fn handle_deep_link(url: &str, app: AppHandle) {
    if let Ok(parsed) = url::Url::parse(url) {
        if parsed.scheme() == "presencejam" {
            let path = parsed.path();
            let code = parsed.query_pairs().find(|(k, _)| k == "code").map(|(_, v)| v.to_string());
            
            if let Some(code_str) = code {
                log::info!("Deep link received for path: {}", path);
                let app_clone = app.clone();
                let code_clone = code_str.clone();
                
                if path == "/teams-callback" {
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = handle_teams_callback(&code_clone, &app_clone).await {
                            log::error!("Teams auth failed: {}", e);
                            let _ = app_clone.emit("teams-auth-failed", e);
                        }
                    });
                } else {
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = handle_spotify_callback(&code_clone, &app_clone).await {
                            log::error!("Spotify auth failed: {}", e);
                            let _ = app_clone.emit("spotify-auth-failed", e);
                        }
                    });
                }
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        use tauri_plugin_single_instance::init as single_instance_init;

        builder = builder.plugin(single_instance_init(|_app, argv, _cwd| {
            log::info!("New app instance opened with deep link: {:?}", argv);
        }));

        builder = builder.plugin(tauri_plugin_deep_link::init());
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
            .build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let state = Arc::new(AppState::new());
            app.manage(state);

            #[cfg(desktop)]
            {
                use tauri_plugin_deep_link::DeepLinkExt;

                #[cfg(any(windows, all(debug_assertions, windows)))]
                {
                    if let Err(e) = app.deep_link().register_all() {
                        log::warn!("Failed to register deep links: {}", e);
                    }
                }

                let start_urls = app.deep_link().get_current();
                if let Ok(Some(urls)) = start_urls {
                    for url in urls {
                        handle_deep_link(url.as_str(), app.handle().clone());
                    }
                }

                let app_handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        handle_deep_link(url.as_str(), app_handle.clone());
                    }
                });
            }

            log::info!("PresenceJam 2.0 started");
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
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}