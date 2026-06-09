import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

// ---------------------------------------------------------------------------
// IMPORTANT: Defaults in this file are FALLBACK-ONLY.
//
// The Rust backend (`src-tauri/src/config.rs`) is the source of truth for
// the runtime config values. The fields here mirror them for first-render
// and disconnected-frontend scenarios only — `loadConfig()` overwrites
// them on startup by calling the Rust `load_config` command.
//
// If you change a default in Rust, change it here too. Drift between the
// two layers causes silent UX bugs (e.g. a Settings slider that the
// backend ignores, or a UI default that the backend immediately
// overwrites). The long-term fix is to generate this file from Rust
// via a build script (see GH issue #13).
// ---------------------------------------------------------------------------

export interface SpotifyConfig {
  client_id: string;
  /**
   * True iff the Spotify `client_secret` is currently stored in the OS
   * keychain. This is a derived/display field — it is populated by
   * `load_config` (and not persisted to disk). The actual secret lives
   * in the keychain, not in `config.json`. See issue #9.
   */
  client_secret_set: boolean;
  redirect_uri: string;
  scopes: string[];
}

export interface TeamsConfig {
  status_format: string;
  clear_on_pause: boolean;
  profanity_filter: boolean;
  profanity_placeholder: string;
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
  // retention_days removed (was a no-op) — see GH #13
}

export interface AppConfig {
  spotify: SpotifyConfig;
  teams: TeamsConfig;
  polling: PollingConfig;
  logging: LoggingConfig;
  autostart: boolean;
}

export const defaultConfig: AppConfig = {
  spotify: {
    client_id: '',
    client_secret_set: false,
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
    start_minimized: false
  },
  polling: {
    default_interval_seconds: 30,
    minimum_interval_seconds: 10,
    max_interval_seconds: 60,
    expiry_buffer_seconds: 10
  },
  logging: {
    enabled: true,
    log_level: 'Info'
  },
  autostart: false
};

export const configStore = writable<AppConfig>(defaultConfig);

let loadPromise: Promise<AppConfig> | null = null;
let savePromise: Promise<void> | null = null;

export async function loadConfig(): Promise<AppConfig> {
  if (loadPromise) return loadPromise;

  loadPromise = (async () => {
    try {
      const cfg = await invoke<AppConfig>('load_config');
      configStore.set(cfg);
      return cfg;
    } catch (e) {
      console.error('[CONFIG] loadConfig failed:', e);
      configStore.set(defaultConfig);
      return defaultConfig;
    } finally {
      loadPromise = null;
    }
  })();

  return loadPromise;
}

export async function saveConfig(cfg: AppConfig): Promise<void> {
  if (savePromise) await savePromise;

  savePromise = (async () => {
    try {
      await invoke('save_config', { config: cfg });
      configStore.set(cfg);
    } finally {
      savePromise = null;
    }
  })();

  await savePromise;
}
