/**
 * C6 i18n foundation (docs/scope-3.3.md §C6).
 *
 * Public barrel. Components import `{ i18n, t }` from `$lib/i18n`.
 * `t(key, params)` resolves the key against the active locale's dictionary
 * (falling back to English) and substitutes `{name}` placeholders from
 * `params`. Reading `t(...)` inside a template is
 * reactive: it tracks `i18n.locale`, so switching locales re-renders.
 * Numeric params go through `Intl.NumberFormat`, so an interpolated count,
 * interval or version carries the locale's grouping separators (#616).
 *
 * `tCount(key, count, params)` is the plural-aware sibling: it selects the
 * CLDR plural category for `count` via `Intl.PluralRules` and interpolates
 * `{count}` itself (#616).
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
import { devLog } from './utils/dev';

export { i18n };
export type { Locale } from './i18n/store.svelte';

const DICTS: Record<Locale, Dict> = { en, de, fr };

// #616: built once per locale — `t()` runs on every render, and constructing
// an `Intl` formatter per call would be the expensive part of it.
const NUMBER_FORMATS: Record<Locale, Intl.NumberFormat> = {
  en: new Intl.NumberFormat('en'),
  de: new Intl.NumberFormat('de'),
  fr: new Intl.NumberFormat('fr')
};
// #616: CLDR categories. `fr` puts 0 in `one`, so the choice is not
// `count === 1` ("0 entrée", not "0 entrées").
const PLURAL_RULES: Record<Locale, Intl.PluralRules> = {
  en: new Intl.PluralRules('en'),
  de: new Intl.PluralRules('de'),
  fr: new Intl.PluralRules('fr')
};

export type TKey = keyof typeof en;

type StripPluralSuffix<K> = K extends `${infer Base}_one` ? Base : never;

/**
 * Base keys that carry a `_one`/`_other` pair — the argument type of
 * `tCount`. Derived from the dictionary, so a key cannot be plural-formatted
 * without its plural entries existing.
 */
export type PluralKey = StripPluralSuffix<Extract<TKey, `${string}_one`>>;

export function t(
  key: TKey,
  params?: Record<string, string | number>
): string {
  // #424: runtime degradation — Dict parity makes a miss impossible at
  // compile time, but a stale chunk / hand-cast TKey can still miss at
  // runtime. Fall back to the key itself instead of throwing in split.
  // Warn in dev so the fallback never masks a real dict bug (#review-4).
  const hit = DICTS[i18n.locale][key] ?? en[key];
  if (hit === undefined) devLog(`[I18N] missing key '${key as string}' for locale '${i18n.locale}' — falling back to key`);
  const text: string = hit ?? (key as string);
  if (params) {
    let out = text;
    const numbers = NUMBER_FORMATS[i18n.locale];
    for (const [name, value] of Object.entries(params)) {
      const rendered = typeof value === 'number' ? numbers.format(value) : String(value);
      out = out.split(`{${name}}`).join(rendered);
    }
    return out;
  }
  return text;
}

/**
 * Plural-aware `t()` (#616). `key` is the base key; the dictionary holds
 * `{key}_one` / `{key}_other`, and any other category the locale reports for
 * `count` falls back to `_other`. `count` is interpolated as `{count}` —
 * an explicit `count` in `params` wins.
 */
export function tCount(
  key: PluralKey,
  count: number,
  params?: Record<string, string | number>
): string {
  const dictionary = DICTS[i18n.locale];
  const categoryKey = `${key}_${PLURAL_RULES[i18n.locale].select(count)}` as TKey;
  return t(dictionary[categoryKey] === undefined ? (`${key}_other` as TKey) : categoryKey, {
    count,
    ...params
  });
}
