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
    default_interval_seconds: BigInt(30),
    minimum_interval_seconds: BigInt(10),
    max_interval_seconds: BigInt(60),
    expiry_buffer_seconds: BigInt(10)
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

function normalizeLoadedConfig(cfg: AppConfig): AppConfig {
  const c = cfg as unknown as { polling: Record<string, unknown> };
  if (c.polling) {
    const p = c.polling;
    if (typeof p.default_interval_seconds === 'number') {
      p.default_interval_seconds = BigInt(p.default_interval_seconds as number);
    } else if (typeof p.default_interval_seconds === 'string') {
      p.default_interval_seconds = BigInt(p.default_interval_seconds as string);
    }
    if (typeof p.minimum_interval_seconds === 'number') {
      p.minimum_interval_seconds = BigInt(p.minimum_interval_seconds as number);
    } else if (typeof p.minimum_interval_seconds === 'string') {
      p.minimum_interval_seconds = BigInt(p.minimum_interval_seconds as string);
    }
    if (typeof p.max_interval_seconds === 'number') {
      p.max_interval_seconds = BigInt(p.max_interval_seconds as number);
    } else if (typeof p.max_interval_seconds === 'string') {
      p.max_interval_seconds = BigInt(p.max_interval_seconds as string);
    }
    if (typeof p.expiry_buffer_seconds === 'number') {
      p.expiry_buffer_seconds = BigInt(p.expiry_buffer_seconds as number);
    } else if (typeof p.expiry_buffer_seconds === 'string') {
      p.expiry_buffer_seconds = BigInt(p.expiry_buffer_seconds as string);
    }
  }
  return cfg;
}

export function toSavePayload(cfg: AppConfig): AppConfig {
  const payload = structuredClone(cfg) as unknown as { polling: Record<string, unknown> };
  const p = payload.polling;
  if (p) {
    p.default_interval_seconds = Number(p.default_interval_seconds as bigint | number | string);
    p.minimum_interval_seconds = Number(p.minimum_interval_seconds as bigint | number | string);
    p.max_interval_seconds = Number(p.max_interval_seconds as bigint | number | string);
    p.expiry_buffer_seconds = Number(p.expiry_buffer_seconds as bigint | number | string);
  }
  return payload as unknown as AppConfig;
}

export async function loadConfig(): Promise<AppConfig> {
  if (loadPromise) return loadPromise;

  loadPromise = (async () => {
    try {
      const cfg = await invoke<AppConfig>('load_config');
      const normalized = normalizeLoadedConfig(cfg);
      configStore.set(normalized);
      return normalized;
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
      const payload = toSavePayload(cfg);
      await invoke('save_config', { config: payload });
      configStore.set(cfg);
    } finally {
      savePromise = null;
    }
  })();

  await savePromise;
}
