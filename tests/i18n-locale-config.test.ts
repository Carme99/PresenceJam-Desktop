/**
 * 4.7.0 (issue #674) — `AppConfig::locale` is the single source of truth for
 * the UI language (webview, tray and native application menu).
 *
 * Covers what the pre-existing #620 tests cannot: the precedence between the
 * config and the legacy `localStorage.locale` mirror, the one-shot migration of
 * that mirror into the config — gated on the config actually having been
 * hydrated from the backend — and the write-through of a user switch.
 *
 * Every case re-imports the store modules: the boot-time emission, the hydrated
 * one and their order are the behaviour under test, so it can only be observed
 * at the module-loading boundary (the one case the static-import rule excepts).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import type { Mock } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));

import { defaultConfig } from '$lib/stores/config';

type ConfigModule = typeof import('$lib/stores/config');
type I18nModule = typeof import('$lib/i18n');

/**
 * A fresh copy of the config + i18n modules with their own `invoke` mock.
 * `beforeEach` clears storage first, so each load starts from a clean mirror.
 */
async function loadStores(): Promise<{
  config: ConfigModule;
  i18n: I18nModule['i18n'];
  setLocaleCalls: () => unknown[][];
}> {
  vi.resetModules();
  const core = await import('@tauri-apps/api/core');
  const invoke = core.invoke as unknown as Mock;
  // Cleared BEFORE the stores load: a write the module-load path performs is
  // part of the behaviour under test, so it must stay visible.
  invoke.mockClear();
  const config = await import('$lib/stores/config');
  const i18nModule = await import('$lib/i18n');
  return {
    config,
    i18n: i18nModule.i18n,
    // Only the locale write-through calls, not the store's other IPC traffic.
    setLocaleCalls: () => invoke.mock.calls.filter((call) => call[0] === 'set_locale')
  };
}

describe('locale source of truth (#674)', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('a persisted locale survives the boot-time defaults and the legacy mirror', async () => {
    // Pre-4.7 bare mirror: the store folds it into the namespaced key at load,
    // so this case also pins that the legacy value still paints and survives.
    localStorage.setItem('locale', 'fr');
    const { config, i18n, setLocaleCalls } = await loadStores();
    expect(i18n.locale).toBe('fr');

    // `load_config` resolves, then the store is hydrated — the real order.
    config.configStore.set({ ...defaultConfig, locale: 'de' });
    config.configHydrated.set(true);

    expect(i18n.locale).toBe('de');
    expect(localStorage.getItem('presencejam:locale')).toBe('de');
    expect(localStorage.getItem('locale')).toBeNull();
    expect(document.documentElement.lang).toBe('de');
    expect(setLocaleCalls()).toEqual([]);
  });

  it('does not migrate before the config has hydrated', async () => {
    localStorage.setItem('presencejam:locale', 'fr');
    const { config, i18n, setLocaleCalls } = await loadStores();

    // The mirror still paints the first frame...
    expect(i18n.locale).toBe('fr');
    expect(setLocaleCalls()).toEqual([]);

    config.configHydrated.set(true);
    expect(setLocaleCalls()).toEqual([['set_locale', { locale: 'fr' }]]);
    // The write-through adopts the persisted value in the shared store.
    await vi.waitFor(() => expect(get(config.configStore).locale).toBe('fr'));
  });

  it('does not migrate English — it is already the default of an absent field', async () => {
    localStorage.setItem('presencejam:locale', 'en');
    const { config, setLocaleCalls } = await loadStores();

    config.configHydrated.set(true);
    expect(setLocaleCalls()).toEqual([]);
  });

  it('an imported config converges the webview without a write-back', async () => {
    // Reverse direction: anything that replaces the config document — the
    // 4.7.0 export/import path, a config reload — must move the UI too, not
    // just the native surfaces. The store is the subscriber, so the import
    // only has to land there; the webview then follows and must NOT echo the
    // value back through `set_locale` (that would fight the writer).
    const { config, i18n, setLocaleCalls } = await loadStores();
    config.configHydrated.set(true);
    expect(i18n.locale).toBe('en');

    config.configStore.set({ ...defaultConfig, locale: 'fr' });
    expect(i18n.locale).toBe('fr');
    expect(document.documentElement.lang).toBe('fr');
    expect(localStorage.getItem('presencejam:locale')).toBe('fr');
    expect(setLocaleCalls()).toEqual([]);
  });

  it('a user switch persists through set_locale and updates the shared store', async () => {
    const { config, i18n, setLocaleCalls } = await loadStores();
    config.configHydrated.set(true);
    await i18n.set('de');
    expect(i18n.locale).toBe('de');
    expect(setLocaleCalls()).toEqual([['set_locale', { locale: 'de' }]]);
    expect(get(config.configStore).locale).toBe('de');
    expect(localStorage.getItem('presencejam:locale')).toBe('de');
  });

  it('an unknown locale is ignored: no persistence, no retag', async () => {
    const { config, i18n, setLocaleCalls } = await loadStores();
    config.configHydrated.set(true);
    await i18n.set('zz' as never);
    expect(i18n.locale).toBe('en');
    expect(setLocaleCalls()).toEqual([]);
  });

  it('falls back to English for a tag the app does not know', async () => {
    localStorage.setItem('presencejam:locale', 'de');
    const { config, i18n, setLocaleCalls } = await loadStores();
    expect(i18n.locale).toBe('de');

    // A hand-edited config: Rust renders English for an unknown tag and logs
    // the fallback, so the webview must not keep painting the mirrored
    // language — and a tag that *is* set is never a migration trigger.
    config.configStore.set({ ...defaultConfig, locale: 'zz' });
    expect(i18n.locale).toBe('en');
    expect(document.documentElement.lang).toBe('en');
    config.configHydrated.set(true);
    expect(setLocaleCalls()).toEqual([]);
  });

  /**
   * #892 — the store subscribes to `configStore` at module level and every
   * emission used to re-write the mirror and the document language, even when
   * the locale was the one already installed.
   */
  it('an unrelated config write performs no locale work (#892)', async () => {
    localStorage.setItem('presencejam:locale', 'fr');
    const { config, i18n } = await loadStores();
    expect(i18n.locale).toBe('fr');

    // Only this store's writes are under test: other mirrors on the same
    // origin write their own keys on unrelated emissions.
    const setItem = vi.spyOn(Storage.prototype, 'setItem');
    const langBefore = document.documentElement.lang;
    try {
      // A notification toggle: the locale field is unchanged.
      config.configStore.set({
        ...defaultConfig,
        locale: 'fr',
        notifications: { ...defaultConfig.notifications, track_change: true }
      });
      expect(setItem.mock.calls.filter(([key]) => key === 'presencejam:locale')).toEqual([]);
      expect(document.documentElement.lang).toBe(langBefore);

      // A real change still persists the mirror exactly once.
      setItem.mockClear();
      config.configStore.set({ ...defaultConfig, locale: 'de' });
      expect(setItem.mock.calls.filter(([key]) => key === 'presencejam:locale')).toEqual([
        ['presencejam:locale', 'de']
      ]);
      expect(document.documentElement.lang).toBe('de');
    } finally {
      setItem.mockRestore();
    }
  });
});
