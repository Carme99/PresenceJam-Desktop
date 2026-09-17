/**
 * 4.7.0 (issue #674) — `AppConfig::locale` is the single source of truth for
 * the UI language (webview, tray and native application menu).
 *
 * Covers what the pre-existing #620 tests cannot: the precedence between the
 * config and the legacy `localStorage.locale` mirror, the one-shot migration
 * of that mirror into the config, and the write-through of a user switch.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import type { Mock } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));

import { invoke } from '@tauri-apps/api/core';
import { i18n } from '$lib/i18n';
import { configStore, defaultConfig } from '$lib/stores/config';

const invokeMock = invoke as unknown as Mock;

/** Only the locale write-through calls, not the store's own IPC traffic. */
function setLocaleCalls(): unknown[][] {
  return invokeMock.mock.calls.filter((call) => call[0] === 'set_locale');
}

describe('locale source of truth (#674)', () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockClear();
  });

  it('the config wins over the localStorage mirror the webview painted from', () => {
    i18n.set('en');
    localStorage.setItem('locale', 'de');
    configStore.set({ ...defaultConfig, locale: 'fr' });
    expect(i18n.locale).toBe('fr');
    expect(localStorage.getItem('locale')).toBe('fr');
    expect(document.documentElement.lang).toBe('fr');
  });

  it('falls back to English for a tag the app does not know', () => {
    // A hand-edited config: Rust renders English and logs the fallback, so
    // the webview must not keep painting the mirrored language.
    i18n.set('de');
    configStore.set({ ...defaultConfig, locale: 'zz' });
    expect(i18n.locale).toBe('en');
    expect(document.documentElement.lang).toBe('en');
  });

  it('a user switch persists through set_locale and updates the shared store', async () => {
    configStore.set({ ...defaultConfig, locale: 'en' });
    await i18n.set('de');
    expect(i18n.locale).toBe('de');
    expect(setLocaleCalls()).toEqual([['set_locale', { locale: 'de' }]]);
    expect(get(configStore).locale).toBe('de');
    expect(localStorage.getItem('locale')).toBe('de');
  });

  it('an unknown locale is ignored: no persistence, no retag', async () => {
    configStore.set({ ...defaultConfig, locale: 'en' });
    await i18n.set('zz' as never);
    expect(i18n.locale).toBe('en');
    expect(setLocaleCalls()).toEqual([]);
  });

  it('migrates a legacy localStorage locale into the config once', async () => {
    localStorage.setItem('locale', 'fr');
    invokeMock.mockClear();
    // Re-import the store: the migration runs at module load, which is the
    // module-loading boundary the static-import rule explicitly excepts.
    vi.resetModules();
    configStore.set({ ...defaultConfig, locale: null });
    await import('$lib/i18n/store.svelte');
    expect(setLocaleCalls()).toEqual([['set_locale', { locale: 'fr' }]]);
  });

  it('does not migrate English — it is already the default of an absent field', async () => {
    localStorage.setItem('locale', 'en');
    invokeMock.mockClear();
    vi.resetModules();
    configStore.set({ ...defaultConfig, locale: null });
    await import('$lib/i18n/store.svelte');
    expect(setLocaleCalls()).toEqual([]);
  });
});
