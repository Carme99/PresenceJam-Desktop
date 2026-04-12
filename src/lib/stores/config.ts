import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export interface SpotifyConfig {
  client_id: string;
  client_secret: string;
  redirect_uri: string;
  scopes: string[];
}

export interface TeamsConfig {
  status_format: string;
  clear_on_pause: boolean;
  profanity_filter: boolean;
  profanity_placeholder: string;
  launch_at_login: boolean;
  start_minimized: boolean;
}

export interface PollingConfig {
  default_interval_seconds: number;
  minimum_interval_seconds: number;
  max_interval_seconds: number;
  expiry_buffer_seconds: number;
}

export interface LoggingConfig {
  enabled: boolean;
  log_level: string;
  retention_days: number;
}

export interface AppConfig {
  spotify: SpotifyConfig;
  teams: TeamsConfig;
  polling: PollingConfig;
  logging: LoggingConfig;
}

export const defaultConfig: AppConfig = {
  spotify: {
    client_id: '',
    client_secret: '',
    redirect_uri: 'presencejam://callback',
    scopes: ['user-read-currently-playing', 'user-read-playback-state']
  },
  teams: {
    status_format: '🎵 {artist} - {track} 🎧',
    clear_on_pause: true,
    profanity_filter: true,
    // NOTE: The Rust backend (profanity::safe_placeholder_default) is the canonical source.
    // This default is only used if load_config fails. Both must stay in sync manually.
    profanity_placeholder: 'Currently Listening to Spotify',
    launch_at_login: false,
    start_minimized: false
  },
  polling: {
    default_interval_seconds: 30,
    minimum_interval_seconds: 5,
    max_interval_seconds: 10,
    expiry_buffer_seconds: 10
  },
  logging: {
    enabled: true,
    log_level: 'Info',
    retention_days: 30
  }
};

export const configStore = writable<AppConfig>(defaultConfig);

export async function loadConfig(): Promise<AppConfig> {
  try {
    const cfg = await invoke<AppConfig>('load_config');
    configStore.set(cfg);
    return cfg;
  } catch {
    return defaultConfig;
  }
}

export async function saveConfig(cfg: AppConfig): Promise<void> {
  await invoke('save_config', { config: cfg });
  configStore.set(cfg);
}
