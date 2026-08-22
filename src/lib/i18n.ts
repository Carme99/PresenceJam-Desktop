/**
 * C6 i18n foundation (docs/scope-3.3.md §C6).
 *
 * Public barrel. Components import `{ i18n, t }` from `$lib/i18n`.
 *
 * `t(key, params)` resolves the key against the active locale's
 * dictionary (falling back to English) and substitutes `{name}`
 * placeholders from `params`. Reading `t(...)` inside a template is
 * reactive: it tracks `i18n.locale`, so switching locales re-renders.
 *
 * Known limitation (per scope doc §C6): Rust-side error strings
 * surfaced through `invoke()` rejections and event payloads remain
 * English — see src/lib/i18n/en.ts.
 */

import { en } from './i18n/en';
import type { Dict } from './i18n/en';
import { de } from './i18n/de';
import { fr } from './i18n/fr';
import { i18n } from './i18n/store.svelte';
import type { Locale } from './i18n/store.svelte';

export { i18n };
export type { Locale } from './i18n/store.svelte';

const DICTS: Record<Locale, Dict> = { en, de, fr };

export type TKey = keyof typeof en;

export function t(
  key: TKey,
  params?: Record<string, string | number>
): string {
  let text: string = DICTS[i18n.locale][key] ?? en[key];
  if (params) {
    for (const [name, value] of Object.entries(params)) {
      text = text.split(`{${name}}`).join(String(value));
    }
  }
  return text;
}
