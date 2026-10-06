pub mod app;
pub mod calendar;
pub mod cli;
pub mod commands;
pub mod config;
pub mod deep_link;
pub mod diagnostics;
pub mod events;
pub mod history;
pub mod http;
pub mod i18n;
pub mod keychain;
pub mod macos_deeplink;
pub mod menu;
pub mod pkce;
pub mod platform;
pub mod polling;
pub mod profanity;
pub mod redact;
pub mod serve;
pub mod sources;
pub mod spotify;
pub mod state;
pub mod teams;
pub mod token_io;
pub mod tray;
pub mod updater_bg;

pub use app::run;
pub use state::{
    AppState, Config, DeepLinkDedup, OnboardingCache, PendingAuths, PendingSpotifyAuth, Polling,
    RecoveryMarkers, TokenCommitOutcome, TokenProvider, Tokens, TokensLoadGate, TokensLoadState,
};
