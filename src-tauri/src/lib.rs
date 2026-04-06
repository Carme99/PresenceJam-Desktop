use std::sync::Arc;
use parking_lot::RwLock;
use tauri::Manager;

pub struct AppState {
    pub config: RwLock<Option<crate::config::AppConfig>>,
    pub spotify_tokens: RwLock<Option<crate::spotify::SpotifyTokens>>,
    pub teams_tokens: RwLock<Option<crate::teams::TeamsTokens>>,
    pub current_track: RwLock<Option<crate::spotify::TrackInfo>>,
    pub is_syncing: RwLock<bool>,
    pub polling_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
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
        }
    }
}

pub mod config;
pub mod spotify;
pub mod teams;
pub mod polling;
pub mod tray;
pub mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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

            log::info!("PresenceJam 2.0 started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_config,
            commands::save_config,
            commands::get_config_dir,
            commands::start_spotify_auth,
            commands::complete_spotify_auth,
            commands::get_spotify_tokens,
            commands::refresh_spotify,
            commands::start_teams_auth,
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