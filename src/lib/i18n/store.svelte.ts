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
 * `localStorage['presencejam:locale']` survives as a pre-paint mirror only: it
 * is read at module load (the config load is an async IPC round-trip and the
 * first frame must already be in the right language), it still converges across
 * webviews through the #620 `storage` listener below, and a value found there
 * while the config carries none is migrated into the config exactly once —
 * after the config has actually been hydrated from the backend, never on the
 * boot-time defaults. The pre-4.7 bare `locale` key is folded into it once, at
 * module load (#909). When both exist, the config wins and the mirror is
 * rewritten to match.
 *
 * Switching also retags `<html lang>`.
 */

import { configHydrated, configStore, defaultConfig } from '$lib/stores/config';
import { invoke } from '@tauri-apps/api/core';
import type { AppConfig } from '$lib/types';
import { devLog } from '$lib/utils/dev';

export type Locale = 'en' | 'de' | 'fr' | 'es' | 'it' | 'pl' | 'pt' | 'nl';

const STORAGE_KEY = 'presencejam:locale';
/**
 * #909: the pre-4.7 mirror lived under a bare `locale` key while every other
 * frontend mirror is namespaced (`presencejam:theme`, `presencejam:density`).
 * A generic key is the likeliest one to collide with anything else writing to
 * the origin, so it is read once, folded into [`STORAGE_KEY`] and dropped —
 * never consulted again.
 */
const LEGACY_STORAGE_KEY = 'locale';
/**
 * Canonical tags, in preference order for documentation only — resolution
 * follows the caller's candidate order, never this array's.
 */
const KNOWN: readonly Locale[] = ['en', 'de', 'fr', 'es', 'it', 'pl', 'pt', 'nl'];
/**
 * Exact regional tags that resolve to a shipped base locale (#984). `pt-BR`
 * is Brazilian Portuguese, the variety the `pt` dictionary is written in, so
 * the regional tag resolves before any bare-base fallback is consulted.
 */
const TAG_ALIASES: Readonly<Record<string, Locale>> = { 'pt-br': 'pt' };
/**
 * Mirror of the Settings "follow system language" choice (#984). When set,
 * the stored mirror is ignored and the browser language is re-resolved on
 * every boot (and on `languagechange`); the last resolution is still
 * persisted into `config.locale` so the tray and the native menu render the
 * same language. An explicit language choice clears it.
 */
const FOLLOW_SYSTEM_KEY = 'presencejam:locale-follow-system';
/**
 * The locale both sides fall back to. Mirrors Rust's `i18n::resolve_tag`,
 * which resolves an absent/unknown `AppConfig::locale` to `"en"` — so writing
 * it into the config would be a no-op and the migration skips it.
 */
const DEFAULT_LOCALE: Locale = 'en';

function resolveLocale(value: unknown): Locale | null {
  if (typeof value !== 'string') return null;
  const raw = value.trim().toLowerCase();
  if (raw.length === 0) return null;
  // Longest-tag-first (#984): an exact regional tag beats its bare base, so
  // `pt-BR` resolves to the Brazilian-Portuguese dictionary even though a
  // bare-base fallback would also match.
  const aliased = TAG_ALIASES[raw];
  if (aliased !== undefined) return aliased;
  if ((KNOWN as readonly string[]).includes(raw)) return raw as Locale;
  const base = raw.split(/[-_]/, 1)[0].trim();
  return (KNOWN as readonly string[]).includes(base) ? (base as Locale) : null;
}

/**
 * Whether the user asked the app to track the OS language (#984). Stored
 * beside the locale mirror rather than in the config: it is a resolution
 * policy, not a language, so Rust never needs to read it.
 */
export function followsSystemLanguage(): boolean {
  try {
    return localStorage.getItem(FOLLOW_SYSTEM_KEY) === '1';
  } catch {
    return false;
  }
}

function setFollowsSystemLanguage(follow: boolean): void {
  try {
    if (follow) localStorage.setItem(FOLLOW_SYSTEM_KEY, '1');
    else localStorage.removeItem(FOLLOW_SYSTEM_KEY);
  } catch {
    // The mode is a convenience, not state — a session without storage
    // keeps the explicitly resolved locale either way.
  }
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
    const legacyLocale = resolveLocale(legacy);
    if (legacyLocale !== null && localStorage.getItem(STORAGE_KEY) === null) {
      localStorage.setItem(STORAGE_KEY, legacyLocale);
    }
    localStorage.removeItem(LEGACY_STORAGE_KEY);
  } catch {
    // localStorage unavailable — there is nothing to migrate.
  }
}

migrateLegacyStorageKey();

/**
 * The locale the first frame renders in (#984): when the user follows the
 * system language, the localStorage mirror is skipped and the browser
 * language is re-resolved on every boot; otherwise the mirror wins when
 * present, then the browser language, then English. Every candidate goes
 * through [`resolveLocale`], so an unsupported system language degrades to
 * English with no signal — the documented fallback.
 */
function detectSystemLocale(): Locale | null {
  const candidates =
    typeof navigator !== 'undefined'
      ? navigator.languages ?? [navigator.language]
      : [];
  for (const lang of candidates) {
    const resolved = resolveLocale(lang);
    if (resolved !== null) return resolved;
  }
  return null;
}
function detectInitialLocale(): Locale {
  if (!followsSystemLanguage()) {
    try {
      const storedLocale = resolveLocale(localStorage.getItem(STORAGE_KEY));
      if (storedLocale !== null) {
        return storedLocale;
      }
    } catch {
      // localStorage unavailable — fall through to browser detection.
    }
  }
  return detectSystemLocale() ?? DEFAULT_LOCALE;
}

const initialLocale = detectInitialLocale();
let current = $state<Locale>(initialLocale);

type LocalePersistenceRequest = {
  locale: Locale;
  generation: number;
  settled: Promise<void>;
  resolve: () => void;
};

let localePersistenceGeneration = 0;
let pendingLocalePersistence: LocalePersistenceRequest | null = null;
let localePersistenceDraining = false;

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
  // #892: every config emission lands here, and an unchanged locale must cost
  // nothing — the mirror write is synchronous and the `lang` write can force
  // style/layout work. Mirrors the same-value guard on the listener below.
  if (next === current) return;
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
 * Requests are serialized so native surfaces cannot apply an older completion
 * after a newer selection. While one write is in flight, only the latest
 * requested locale remains queued. A generation guard then lets only the
 * current request update the shared config store after its IPC succeeds.
 */
async function drainLocalePersistence(): Promise<void> {
  try {
    while (pendingLocalePersistence !== null) {
      const request = pendingLocalePersistence;
      pendingLocalePersistence = null;
      try {
        await invoke('set_locale', { locale: request.locale });
        if (request.generation === localePersistenceGeneration) {
          // Keep the shared config store in step, so Settings is not left
          // comparing its own draft against a stale locale.
          configStore.update((cfg) => ({ ...cfg, locale: request.locale }));
        }
      } catch (err) {
        devLog(
          `[I18N] set_locale failed for '${request.locale}' ` +
            `(current locale remains '${current}'): ${String(err)}`
        );
      } finally {
        request.resolve();
      }
    }
  } finally {
    localePersistenceDraining = false;
  }
}

function persistLocale(next: Locale): Promise<void> {
  let resolveRequest!: () => void;
  const request: LocalePersistenceRequest = {
    locale: next,
    generation: ++localePersistenceGeneration,
    settled: new Promise<void>((resolve) => {
      resolveRequest = resolve;
    }),
    resolve: () => resolveRequest()
  };

  // A request that has not started is superseded immediately; the in-flight
  // request retains its own promise until its IPC finishes.
  pendingLocalePersistence?.resolve();
  pendingLocalePersistence = request;
  if (!localePersistenceDraining) {
    localePersistenceDraining = true;
    void drainLocalePersistence();
  }
  return request.settled;
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
  const locale = resolveLocale(cfg.locale);
  if (locale !== null) {
    // #892: an unrelated config write (a Settings save, a toggle, a snooze)
    // carries the same locale — no locale work at all for it.
    if (locale !== current) applyLocale(locale);
    return;
  }
  if (typeof cfg.locale === 'string') {
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
  /** Whether the app tracks the OS language instead of a fixed choice (#984). */
  get followSystem(): boolean {
    return followsSystemLanguage();
  },
  /**
   * Switch locale: applies it to this webview immediately, mirrors it for the
   * pre-paint frame, and persists it to `AppConfig::locale` (the single source
   * of truth) via the `set_locale` command. An unknown value is ignored. An
   * explicit choice leaves follow-system mode.
   */
  async set(next: Locale): Promise<void> {
    const locale = resolveLocale(next);
    if (locale === null) return;
    setFollowsSystemLanguage(false);
    applyLocale(locale);
    await persistLocale(locale);
  },
  /**
   * Follow the system language (#984): re-resolve from the browser language
   * right now and persist the resolution, so the tray and the native menu
   * render the same language; later OS changes re-resolve through the
   * `languagechange` listener below.
   */
  async followSystemLanguage(): Promise<void> {
    setFollowsSystemLanguage(true);
    const resolved = detectSystemLocale() ?? DEFAULT_LOCALE;
    applyLocale(resolved);
    await persistLocale(resolved);
  }
};

// #620: cross-window convergence — every webview (main + detached Logs /
// Settings) owns an independent locale instance, so a detached window kept
// rendering the language it loaded with after the user switched language in
// the main window. `storage` fires in every OTHER same-origin webview on
// write; this mirrors the #423 theme listener (stores/theme.ts).
if (typeof window !== 'undefined') {
  window.addEventListener('storage', (e) => {
    // A peer entering follow-system mode writes only the flag: apply the
    // mode without touching the locale (the entering window persists the
    // resolution itself, which arrives as its own event).
    if (e.key === FOLLOW_SYSTEM_KEY) {
      if (e.newValue === '1') applyLocale(detectSystemLocale() ?? current);
      return;
    }
    if (e.key !== STORAGE_KEY) return;
    const next = resolveLocale(e.newValue);
    if (next === null) return;
    // Same-value guard: `applyLocale` would re-tag the document.
    if (next === current) return;
    // An explicit choice from another window wins over follow-system mode.
    setFollowsSystemLanguage(false);
    // Local only — the window that switched already wrote the config, so
    // persisting again here would be a redundant round-trip.
    applyLocale(next);
  });
  // #984: while follow-system mode is on, an OS language change
  // re-resolves and persists, so the tray and the native menu follow the
  // new language without a restart.
  window.addEventListener('languagechange', () => {
    if (!followsSystemLanguage()) return;
    void i18n.followSystemLanguage();
  });
}
