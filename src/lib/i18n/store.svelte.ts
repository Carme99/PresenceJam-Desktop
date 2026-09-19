/**
 * C6 i18n foundation (docs/scope-3.3.md §C6).
 *
 * Locale state store. `$state` requires a `.svelte.ts` module; the
 * public surface (`i18n`, `t`) is re-exported from the `src/lib/i18n.ts`
 * barrel so components import from one place.
 *
 * 4.7.0 (issue #674): `AppConfig::locale` is the single source of truth.
 * The tray and the native application menu render from the same field, so the
 * webview and the native surfaces can never disagree about the language.
 * `localStorage.locale` survives as a pre-paint mirror only: it is read at
 * module load (the config load is an async IPC round-trip and the first frame
 * must already be in the right language), it still converges across webviews
 * through the #620 `storage` listener below, and a value found there while the
 * config carries none is migrated into the config exactly once — after the
 * config has actually been hydrated from the backend, never on the boot-time
 * defaults. When both exist, the config wins and the mirror is rewritten to
 * match.
 *
 * Switching also retags `<html lang>`.
 */

import { configHydrated, configStore, defaultConfig } from '$lib/stores/config';
import { invoke } from '@tauri-apps/api/core';
import type { AppConfig } from '$lib/types';
import { devLog } from '$lib/utils/dev';

export type Locale = 'en' | 'de' | 'fr';

const STORAGE_KEY = 'presencejam:locale';
/**
 * #909: the pre-4.7 mirror lived under a bare `locale` key while every other
 * frontend mirror is namespaced (`presencejam:theme`, `presencejam:density`).
 * A generic key is the likeliest one to collide with anything else writing to
 * the origin, so it is read once, folded into [`STORAGE_KEY`] and dropped —
 * never consulted again.
 */
const LEGACY_STORAGE_KEY = 'locale';
const KNOWN: readonly Locale[] = ['en', 'de', 'fr'];
/**
 * The locale both sides fall back to. Mirrors Rust's `i18n::resolve_tag`,
 * which resolves an absent/unknown `AppConfig::locale` to `"en"` — so writing
 * it into the config would be a no-op and the migration skips it.
 */
const DEFAULT_LOCALE: Locale = 'en';

function isLocale(value: unknown): value is Locale {
  return typeof value === 'string' && (KNOWN as readonly string[]).includes(value);
}

/**
 * Fold the legacy mirror into the namespaced key (issue #909). Runs once, at
 * module load, before the first frame picks a locale; a value the app cannot
 * use is dropped rather than copied, and the legacy key is always removed so a
 * later write cannot resurrect it.
 */
function migrateLegacyStorageKey(): void {
  try {
    const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
    if (legacy === null) return;
    if (isLocale(legacy) && localStorage.getItem(STORAGE_KEY) === null) {
      localStorage.setItem(STORAGE_KEY, legacy);
    }
    localStorage.removeItem(LEGACY_STORAGE_KEY);
  } catch {
    // localStorage unavailable — there is nothing to migrate.
  }
}

migrateLegacyStorageKey();

/**
 * The locale the first frame renders in: the localStorage mirror when present,
 * otherwise the browser language (de/fr prefixes), otherwise English.
 */
function detectInitialLocale(): Locale {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (isLocale(stored)) {
      return stored;
    }
  } catch {
    // localStorage unavailable — fall through to browser detection.
  }
  const candidates =
    typeof navigator !== 'undefined'
      ? navigator.languages ?? [navigator.language]
      : [];
  for (const lang of candidates) {
    const base = (lang ?? '').toLowerCase();
    if (base.startsWith('de')) return 'de';
    if (base.startsWith('fr')) return 'fr';
  }
  return DEFAULT_LOCALE;
}

const initialLocale = detectInitialLocale();
let current = $state<Locale>(initialLocale);

// #620: `<html lang>` drives screen-reader pronunciation and `:lang()`
// styling. `app.html` ships the pre-hydration `lang="en"`; from the first
// paint on, the active locale owns it.
function applyDocumentLang(locale: Locale): void {
  if (typeof document === 'undefined') return;
  document.documentElement.lang = locale;
}

applyDocumentLang(initialLocale);

/**
 * Apply a locale to this webview: reactive state, `<html lang>` and the
 * pre-paint mirror. Never persists — see [`persistLocale`] for the config
 * write and the migration note on the subscription below.
 */
function applyLocale(next: Locale): void {
  current = next;
  applyDocumentLang(next);
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // Mirroring for the next pre-paint frame is best-effort; the session
    // keeps the new locale either way.
  }
}

/**
 * Write the locale into the config (the source of truth) through the dedicated
 * `set_locale` command, which also relabels the tray and the native
 * application menu without a restart.
 *
 * Best-effort: the webview has already switched, so a failed write must not
 * surface an error for what is a cosmetic change — the next launch simply
 * falls back to the stored value.
 */
async function persistLocale(next: Locale): Promise<void> {
  try {
    await invoke('set_locale', { locale: next });
    // Keep the shared config store in step, so Settings is not left comparing
    // its own draft against a stale locale.
    configStore.update((cfg) => ({ ...cfg, locale: next }));
  } catch (err) {
    devLog(`[I18N] set_locale failed (locale stays mirror-only): ${String(err)}`);
  }
}

/**
 * 4.7.0 (issue #674): fold a legacy `localStorage.locale` value (or the
 * browser-detected language) into `config.locale` on first run, so the native
 * surfaces pick up the user's existing choice instead of defaulting to
 * English. Runs at most once per session, and never for English — that is
 * already the documented default of an absent field.
 */
let migrationAttempted = false;
function migrateLegacyLocale(): void {
  if (migrationAttempted) return;
  migrationAttempted = true;
  const detected = current; // the mirror, or the browser language
  if (detected === DEFAULT_LOCALE) return;
  devLog(`[I18N] migrating locale '${detected}' into the config`);
  void persistLocale(detected);
}

/**
 * Reconcile this webview with the authoritative config (issue #674): a locale
 * the app knows is applied (the config wins over the mirror); a tag it does
 * not know renders English, exactly as Rust's `resolve_tag` does; an absent
 * value is the pre-4.7 default and may be filled from the mirror — but only
 * once the config has actually been hydrated from the backend.
 *
 * The hydration gate is load-bearing: the config store emits the frontend
 * defaults at module load, before `loadConfig()` resolves, and migrating on
 * that emission would write the mirror's value over a locale that is persisted
 * on disk.
 */
function reconcile(cfg: AppConfig, hydrated: boolean): void {
  if (isLocale(cfg.locale)) {
    applyLocale(cfg.locale);
    return;
  }
  if (typeof cfg.locale === 'string' && cfg.locale.length > 0) {
    applyLocale(DEFAULT_LOCALE);
    return;
  }
  if (hydrated) {
    migrateLegacyLocale();
  }
}

// Both stores are needed: the config carries the value, the hydration flag says
// whether that value came from the backend or is still the frontend default.
let latestConfig: AppConfig = defaultConfig;
let configLoaded = false;
configStore.subscribe((cfg) => {
  latestConfig = cfg;
  reconcile(latestConfig, configLoaded);
});
configHydrated.subscribe((hydrated) => {
  configLoaded = hydrated;
  reconcile(latestConfig, configLoaded);
});

export const i18n = {
  /** Reactive current locale — read it inside templates/effects. */
  get locale(): Locale {
    return current;
  },
  /**
   * Switch locale: applies it to this webview immediately, mirrors it for the
   * pre-paint frame, and persists it to `AppConfig::locale` (the single source
   * of truth) via the `set_locale` command. An unknown value is ignored.
   */
  async set(next: Locale): Promise<void> {
    if (!isLocale(next)) return;
    applyLocale(next);
    await persistLocale(next);
  }
};

// #620: cross-window convergence — every webview (main + detached Logs /
// Settings) owns an independent locale instance, so a detached window kept
// rendering the language it loaded with after the user switched language in
// the main window. `storage` fires in every OTHER same-origin webview on
// write; this mirrors the #423 theme listener (stores/theme.ts).
if (typeof window !== 'undefined') {
  window.addEventListener('storage', (e) => {
    if (e.key !== STORAGE_KEY) return;
    const next = e.newValue;
    if (next === null || !isLocale(next)) return;
    // Same-value guard: `applyLocale` would re-tag the document.
    if (next === current) return;
    // Local only — the window that switched already wrote the config, so
    // persisting again here would be a redundant round-trip.
    applyLocale(next);
  });
}
