import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import type { AppConfig } from '../types';

export const defaultConfig: AppConfig = {
  spotify: {
    client_id: '',
    client_secret_set: false,
    redirect_uri: 'presencejam://callback'
  },
  teams: {
    status_format: '🎵 {artist} - {track} 🎧',
    clear_on_pause: true,
    profanity_filter: true,
    // NOTE: The Rust backend (profanity::safe_placeholder_default) is the canonical source.
    // This default is only used if load_config fails. Both must stay in sync manually.
    profanity_placeholder: 'Currently Listening to Spotify',
    start_minimized: false,
    availability_sync: false,
    presence_gate: true
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
