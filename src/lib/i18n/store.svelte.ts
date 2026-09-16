/**
 * C6 i18n foundation (docs/scope-3.3.md §C6).
 *
 * Locale state store. `$state` requires a `.svelte.ts` module; the
 * public surface (`i18n`, `t`) is re-exported from the `src/lib/i18n.ts`
 * barrel so components import from one place.
 *
 * The locale persists to localStorage under `locale` and defaults to
 * the browser language (de/fr prefixes), falling back to English.
 * Switching it also retags `<html lang>`; a `storage` write from another
 * webview converges on the new locale (#620).
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

export const i18n = {
  /** Reactive current locale — read it inside templates/effects. */
  get locale(): Locale {
    return current;
  },
  /** Switch locale, persist the choice and retag the document. */
  set(next: Locale): void {
    if (!(KNOWN as readonly string[]).includes(next)) return;
    current = next;
    applyDocumentLang(next);
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Persistence is best-effort; the session keeps the new locale.
    }
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
    if (next === null || !(KNOWN as readonly string[]).includes(next)) return;
    // Same-value guard: `set()` would re-persist and re-tag the document.
    if (next === current) return;
    i18n.set(next as Locale);
  });
}
