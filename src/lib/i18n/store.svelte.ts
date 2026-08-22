/**
 * C6 i18n foundation (docs/scope-3.3.md §C6).
 *
 * Locale state store. `$state` requires a `.svelte.ts` module; the
 * public surface (`i18n`, `t`) is re-exported from the `src/lib/i18n.ts`
 * barrel so components import from one place.
 *
 * The locale persists to localStorage under `locale` and defaults to
 * the browser language (de/fr prefixes), falling back to English.
 */

export type Locale = 'en' | 'de' | 'fr';

const STORAGE_KEY = 'locale';
const KNOWN: readonly Locale[] = ['en', 'de', 'fr'];

function detectInitialLocale(): Locale {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored && (KNOWN as readonly string[]).includes(stored)) {
      return stored as Locale;
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
  return 'en';
}

let current = $state<Locale>(detectInitialLocale());

export const i18n = {
  /** Reactive current locale — read it inside templates/effects. */
  get locale(): Locale {
    return current;
  },
  /** Switch locale and persist the choice. */
  set(next: Locale): void {
    if (!(KNOWN as readonly string[]).includes(next)) return;
    current = next;
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Persistence is best-effort; the session keeps the new locale.
    }
  }
};
