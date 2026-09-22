/**
 * #488 (fe slice) — real t() behavior + call-site key coverage.
 * #616 adds the plural/number layer (`tCount`), #620 the `<html lang>` tag
 * and cross-webview convergence, #619 the removal of a duplicated key.
 *
 * Fail pre-fix: a runtime-missing key threw TypeError instead of
 * degrading; plural choice was `count === 1` in the component (so French
 * rendered "0 entrées"), numbers interpolated raw, `lang` never updated and
 * a locale switch never reached an already-open detached window.
 * (Dict copy — PageHeader default, Onboarding placeholders,
 * reconnect.reconnectSpotify delete — is owned by the ux slice.)
 * #752 adds placeholder-shape parity: per-key `{param}` sets across
 * en/de/fr, plus the call-site direction (literal params vs en template).
 */
import { describe, it, expect, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
// Static: the i18n barrel is a pure TS module (only .svelte.ts leaf
// imports need the svelte plugin, configured in vitest.config.js).
import { t, tCount, i18n } from '$lib/i18n';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..');
const src = join(root, 'src');

function read(p: string): string {
  return readFileSync(join(root, p), 'utf8');
}

// en dict keys parsed straight from source. Keys are the quoted 'a.b'
// literals.
function dictKeys(source: string): string[] {
  const keys: string[] = [];
  const re = /^  '([^']+)':/gm;
  let m: RegExpExecArray | null;
  while ((m = re.exec(source)) !== null) keys.push(m[1]);
  return keys;
}

// Key/value pairs parsed from a dictionary source. Values are either
// single-quoted (the common case) or double-quoted when the copy itself
// contains an apostrophe; prettier wraps the long ones onto the next line,
// so the value may start on the line after the key. Escaped quotes inside
// the literal are tolerated — without that, `logs.openFolderError` (fr,
// `\'`) and `onboarding.playbackSourceMacNote` (en, `\"`) silently drop
// out of every value-sweeping test below (#752).
function dictEntries(source: string): { key: string; value: string }[] {
  const out: { key: string; value: string }[] = [];
  const re = /^  '([^']+)':\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)")\s*,?$/gm;
  let m: RegExpExecArray | null;
  while ((m = re.exec(source)) !== null) {
    out.push({ key: m[1], value: m[2] ?? m[3] });
  }
  return out;
}

function dictMap(source: string): Map<string, string> {
  return new Map<string, string>(dictEntries(source).map((e): [string, string] => [e.key, e.value]));
}

// #752: interpolation slots — every `{name}` group in a value. Sorted and
// deduplicated, so per-key set comparison is a string comparison.
function placeholdersOf(value: string): string[] {
  return [...new Set([...value.matchAll(/\{([A-Za-z0-9_]+)\}/g)].map((m) => m[1]))].sort();
}

// #752: values whose braces are documentation content, never interpolation
// slots. Each is rendered through `t()` WITHOUT params (the call-site test
// below fails params on these), so the braces must survive verbatim — in
// every locale. Named individually on purpose: a new brace-bearing hint is
// allowlisted here deliberately, never by pattern.
const BRACE_LITERAL: Record<string, true> = {
  'settings.formatTemplatePlaceholder': true,
  'settings.placeholdersHint': true,
  'onboarding.placeholdersHint': true,
  'settings.episodeFormatHint': true,
  'settings.placeholderTextHint': true,
};

describe('i18n key coverage (#488)', () => {
  const enSrc = read('src/lib/i18n/en.ts');
  const deSrc = read('src/lib/i18n/de.ts');
  const frSrc = read('src/lib/i18n/fr.ts');

  it('en/de/fr carry exactly the same key set', () => {
    const en = dictKeys(enSrc).sort();
    const de = dictKeys(deSrc).sort();
    const fr = dictKeys(frSrc).sort();
    expect(de).toEqual(en);
    expect(fr).toEqual(en);
  });

  // #752, dictionary direction: every `{name}` slot must exist under the
  // same name in all three locales. A typo (`{timm}`) or a dropped slot in
  // de/fr renders raw braces to the user while en stays correct — the
  // key-set test above cannot see it. Brace-literal hints (user-facing
  // token documentation, rendered WITHOUT params) are exempt by
  // individual name and guarded by the next test instead.
  it('placeholders match across en/de/fr (#752)', () => {
    const de = dictMap(deSrc);
    const fr = dictMap(frSrc);
    const offenders: string[] = [];
    for (const { key, value } of dictEntries(enSrc)) {
      if (BRACE_LITERAL[key]) continue;
      const want = JSON.stringify(placeholdersOf(value));
      for (const [file, dict] of [['de.ts', de], ['fr.ts', fr]] as const) {
        const got = JSON.stringify(placeholdersOf(dict.get(key) ?? ''));
        if (got !== want) offenders.push(`${file}: ${key} uses ${got}, en uses ${want}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  // #752: the allowlisted hints still document their tokens after
  // translation — a locale that lost every `{token}` teaches nothing.
  it('brace-literal hints keep their documented tokens in every locale (#752)', () => {
    const offenders: string[] = [];
    for (const key of Object.keys(BRACE_LITERAL)) {
      for (const [file, source] of [['en.ts', enSrc], ['de.ts', deSrc], ['fr.ts', frSrc]] as const) {
        const value = dictMap(source).get(key) ?? '';
        if (placeholdersOf(value).length === 0) offenders.push(`${file}: ${key} documents no tokens`);
      }
    }
    expect(offenders).toEqual([]);
  });

  // #906: the wait-state copy uses the ellipsis character (U+2026). Three
  // ASCII dots occupy a different width, so a `common.loading` label and the
  // `common.reconnecting` sibling rendered in the same region wrap at
  // different points — and every new key copies whichever form it sits next
  // to. Fail on the ASCII sequence in ANY dictionary value.
  it('spells the ellipsis with U+2026 in every dictionary value (#906)', () => {
    const offenders: string[] = [];
    for (const [file, source] of [
      ['en.ts', enSrc],
      ['de.ts', deSrc],
      ['fr.ts', frSrc],
    ] as const) {
      for (const { key, value } of dictEntries(source)) {
        if (value.includes('...')) offenders.push(`${file}: ${key}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('renders the same ellipsis for loading and reconnecting in en/de/fr (#906)', () => {
    for (const locale of ['en', 'de', 'fr'] as const) {
      void i18n.set(locale);
      expect(t('common.loading')).toContain('…');
      expect(t('common.reconnecting')).toContain('…');
    }
    void i18n.set('en');
  });

  // #907: fr.ts mixed ASCII apostrophes with the typographic U+2019, and used
  // a plain space before `: ? ! ;` — a legal line-break opportunity in French
  // typesetting, which is the defect the non-breaking space exists to prevent
  // (a wrapped toast can otherwise start a line with a bare `:`).
  it('uses the typographic apostrophe and a non-breaking space before French punctuation (#907)', () => {
    // U+00A0, spelled out: an invisible literal in the source is a trap.
    const NBSP = '\u00a0';
    const offenders: string[] = [];
    for (const { key, value } of dictEntries(frSrc)) {
      if (/[A-Za-zÀ-ÿ]'[A-Za-zÀ-ÿ]/.test(value)) {
        offenders.push(`${key}: ASCII apostrophe between letters`);
      }
      for (const m of value.matchAll(/\s([:?!;])/g)) {
        if (m[0][0] !== NBSP) offenders.push(`${key}: plain space before '${m[1]}'`);
      }
      if (/«(?!\u00a0)/.test(value) || /(?<!\u00a0)»/.test(value)) {
        offenders.push(`${key}: guillemet without a non-breaking space`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('renders the French punctuation without breaking placeholders (#907)', () => {
    void i18n.set('fr');
    // The sweep is mechanical: `{seconds}` still interpolates, and the colon
    // that introduces it now carries the non-breaking space.
    const label = t('settings.defaultIntervalLabel', { seconds: 30 });
    expect(label).toContain('30s');
    expect(label).toMatch(/\u00a0: 30s$/);
    void i18n.set('en');
  });

  it('real t() resolves known keys and substitutes params', () => {
    expect(t('common.back')).toBe('Back');
    expect(t('common.codeExpiresIn', { time: '5m' })).toBe('Code expires in 5m');
    expect(i18n.locale).toBe('en');
  });

  it('real t() degrades to the key on runtime miss (#424)', () => {
    expect(t('dashZZZ' as never)).toBe('dashZZZ');
    expect(t('dashZZZ' as never, { x: 1 })).toBe('dashZZZ');
  });

  it('status-template placeholder hint braces survive without params', () => {
    const m = enSrc.match(/'settings\.formatTemplatePlaceholder': '([^']+)'/);
    expect(m?.[1]).toContain('{artist}');
  });

  // Issue #580/#581: the Settings hint is the only place a user can learn
  // which tokens exist, so every token the formatter substitutes must be
  // listed there — and the braces must survive `t()` unsubstituted (only
  // the listed names are interpolated). The episode template has no config
  // key yet, so its own hint must state the template instead of offering
  // tokens the music template cannot use.
  it('lists every status-format token, and the episode template, in the hints', () => {
    for (const token of [
      '{artist}',
      '{track}',
      '{album}',
      '{emoji}',
      '{device}',
      '{playlist}',
      '{context}',
      '{progress}',
      '{shuffle}',
      '{repeat}',
    ]) {
      expect(t('settings.placeholdersHint')).toContain(token);
      expect(t('onboarding.placeholdersHint')).toContain(token);
      expect(t('settings.episodeFormatHint')).not.toContain(token);
    }
    for (const token of ['{show}', '{episode}']) {
      expect(t('settings.episodeFormatHint')).toContain(token);
    }
  });

  it('every static t() call-site resolves against the en key set', () => {
    const enKeys = dictKeys(enSrc);
    // Collect every t('a.b') literal across Svelte/TS sources.
    const files: string[] = [];
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = join(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (/\.(svelte|ts)$/.test(e.name)) files.push(p);
      }
    };
    walk(src);
    const missing: string[] = [];
    for (const f of files) {
      const body = readFileSync(f, 'utf8');
      // `tCount('a.b', n)` reads the `a.b_one` / `a.b_other` pair, so it
      // resolves against the suffixed keys rather than the base name.
      const re = /\b(t|tCount)\(\s*'([^']+)'/g;
      let m: RegExpExecArray | null;
      while ((m = re.exec(body)) !== null) {
        const keys = m[1] === 'tCount' ? [`${m[2]}_one`, `${m[2]}_other`] : [m[2]];
        for (const key of keys) {
          if (!enKeys.includes(key))
            missing.push(`${f.replace(root + '/', '')}: ${key}`);
        }
      }
    }
    expect(missing).toEqual([]);
  });

  // #752, call-site direction: every literal `t('key', {...})` must pass
  // exactly the slots the en template declares — no omission (renders
  // `{name}` verbatim), no misspelling, no stale extra (silently ignored
  // by `t()`). `tCount('base', n, {...})` passes extras atop the injected
  // `{count}`, checked against the `_one`/`_other` union minus `count`.
  // Shorthand (`{ shown }`) and `key: <expr>` both count as passing `key`;
  // a spread or any non-literal params object fails loudly, so the parser
  // below never silently under-reads a call. Unknown keys are skipped —
  // the key-coverage test above owns them.
  it('every static t()/tCount() call-site passes exactly the en placeholders (#752)', () => {
    const en = dictMap(enSrc);

    // Skip whitespace and `//` / `/* */` comments — Svelte call-sites wrap
    // params across lines with `//` notes inside the literal.
    function skipTrivia(body: string, i: number): number {
      while (i < body.length) {
        const c = body[i];
        if (c === ' ' || c === '\t' || c === '\n' || c === '\r') { i++; continue; }
        if (c === '/' && body[i + 1] === '/') { while (i < body.length && body[i] !== '\n') i++; continue; }
        if (c === '/' && body[i + 1] === '*') {
          i += 2;
          while (i < body.length && !(body[i] === '*' && body[i + 1] === '/')) i++;
          i += 2;
          continue;
        }
        break;
      }
      return i;
    }

    // Skip one balanced value expression, stopping (unconsumed) at the
    // top-level `,`, `}` or `)`. Strings in all three quotes, comments and
    // nested parens/brackets/braces are consumed, never interpreted. A
    // top-level `)` ends a tCount count-expression, so it is a terminator
    // here — not an error.
    function skipValue(body: string, i: number): number {
      let d1 = 0, d2 = 0, d3 = 0;
      let quote: string | null = null;
      while (i < body.length) {
        const x = body[i];
        if (quote !== null) {
          if (x === '\\') { i += 2; continue; }
          if (x === quote) quote = null;
          i++;
          continue;
        }
        if (x === "'" || x === '"' || x === '`') { quote = x; i++; continue; }
        if (x === '/' && body[i + 1] === '/') { while (i < body.length && body[i] !== '\n') i++; continue; }
        if (x === '/' && body[i + 1] === '*') {
          i += 2;
          while (i < body.length && !(body[i] === '*' && body[i + 1] === '/')) i++;
          i += 2;
          continue;
        }
        if (x === '(') d1++;
        else if (x === ')') { if (d1 === 0) return i; d1--; }
        else if (x === '[') d2++;
        else if (x === ']') { if (d2 === 0) return -1; d2--; }
        else if (x === '{') d3++;
        else if (x === '}') {
          if (d1 === 0 && d2 === 0 && d3 === 0) return i;
          d3--;
        } else if (x === ',' && d1 === 0 && d2 === 0 && d3 === 0) return i;
        i++;
      }
      return -1;
    }

    // Parse `{ k, key: <expr>, ... }` at body[i]: the passed key names.
    // `spread`/`dynamic` fail the call loudly rather than under-read it.
    function parseParamsObject(body: string, i: number): { keys: string[]; spread: boolean; dynamic: boolean } {
      i = skipTrivia(body, i);
      if (body[i] !== '{') return { keys: [], spread: false, dynamic: true };
      i++;
      const keys: string[] = [];
      let spread = false;
      for (;;) {
        i = skipTrivia(body, i);
        if (i >= body.length) return { keys, spread, dynamic: true };
        const c = body[i];
        if (c === '}') return { keys, spread, dynamic: false };
        if (c === ',') { i++; continue; }
        if (c === '.' && body[i + 1] === '.' && body[i + 2] === '.') {
          spread = true;
          const end = skipValue(body, i + 3);
          if (end < 0) return { keys, spread, dynamic: true };
          i = end;
          continue;
        }
        let key: string | null = null;
        if (/[A-Za-z_$]/.test(c)) {
          let j = i + 1;
          while (j < body.length && /[A-Za-z0-9_$]/.test(body[j])) j++;
          key = body.slice(i, j);
          i = j;
        } else if (c === "'" || c === '"') {
          let j = i + 1;
          while (j < body.length && (body[j] !== c || body[j - 1] === '\\')) j++;
          if (j >= body.length) return { keys, spread, dynamic: true };
          key = body.slice(i + 1, j);
          i = j + 1;
        } else {
          return { keys, spread, dynamic: true };
        }
        i = skipTrivia(body, i);
        if (body[i] === ':') {
          const end = skipValue(body, i + 1);
          if (end < 0) return { keys, spread, dynamic: true };
          i = end;
        }
        keys.push(key);
      }
    }

    // From just past `tCount('key',` skip the count expression: return the
    // remainder (extra params) at a top-level `,`, or '' at `)` (no extras).
    function splitCountArgs(body: string, i: number): { extra: string; ok: boolean } {
      const end = skipValue(body, i);
      if (end < 0) return { extra: '', ok: false };
      if (body[end] === ',') return { extra: body.slice(end + 1), ok: true };
      return { extra: '', ok: true };
    }

    const files: string[] = [];
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = join(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (/\.(svelte|ts)$/.test(e.name)) files.push(p);
      }
    };
    walk(src);

    const offenders: string[] = [];
    const show = (names: string[]) => `[${[...new Set(names)].sort().join(', ')}]`;
    for (const f of files) {
      const body = readFileSync(f, 'utf8');
      const rel = f.replace(root + '/', '');
      const re = /\b(t|tCount)\(\s*'([^']+)'/g;
      let m: RegExpExecArray | null;
      while ((m = re.exec(body)) !== null) {
        const fn = m[1];
        const key = m[2];
        const i = skipTrivia(body, m.index + m[0].length);
        if (fn === 't') {
          if (body[i] !== ',') {
            // No params: allowed only when the template needs none — or
            // when the key is a brace-literal hint (content, not slots).
            const value = en.get(key);
            if (value === undefined || BRACE_LITERAL[key]) continue;
            const want = placeholdersOf(value);
            if (want.length > 0) offenders.push(`${rel}: t('${key}') passes no params but en uses ${show(want)}`);
            continue;
          }
          const parsed = parseParamsObject(body, i + 1);
          if (parsed.dynamic || parsed.spread) {
            offenders.push(`${rel}: t('${key}') uses dynamic/spread params — extend the parser or pass a literal`);
            continue;
          }
          const value = en.get(key);
          if (value === undefined) continue;
          if (BRACE_LITERAL[key]) {
            offenders.push(`${rel}: t('${key}') is a brace-literal hint and must be called WITHOUT params`);
            continue;
          }
          const want = show(placeholdersOf(value));
          const got = show(parsed.keys);
          if (got !== want) offenders.push(`${rel}: t('${key}') passes ${got} but en uses ${want}`);
        } else {
          const one = en.get(`${key}_one`);
          const other = en.get(`${key}_other`);
          if (one === undefined || other === undefined) continue;
          const want = show([...placeholdersOf(one), ...placeholdersOf(other)].filter((n) => n !== 'count'));
          if (body[i] !== ',') {
            if (want !== '[]') offenders.push(`${rel}: tCount('${key}') passes no extra params but templates need ${want}`);
            continue;
          }
          const split = splitCountArgs(body, i + 1);
          if (!split.ok) {
            offenders.push(`${rel}: tCount('${key}') has unparseable args`);
            continue;
          }
          if (split.extra.trim() === '') {
            if (want !== '[]') offenders.push(`${rel}: tCount('${key}') passes no extra params but templates need ${want}`);
            continue;
          }
          const parsed = parseParamsObject(split.extra, 0);
          if (parsed.dynamic || parsed.spread) {
            offenders.push(`${rel}: tCount('${key}') uses dynamic/spread extra params — extend the parser or pass a literal`);
            continue;
          }
          const got = show(parsed.keys);
          if (got !== want) offenders.push(`${rel}: tCount('${key}') passes ${got} but templates need ${want}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it('no dictionary carries a duplicated status key (#619, #905)', () => {
    for (const source of [enSrc, deSrc, frSrc]) {
      // #619: one key for the reconnect status message.
      expect(dictKeys(source)).not.toContain('reconnect.needsReconnect');
      // #905: both rule cards post the same replacement, so they read the
      // same key — the quiet-hours copy had already drifted in French.
      expect(dictKeys(source)).not.toContain('rules.quietReplacementPlaceholder');
    }
  });
});

describe('plural and number formatting (#616)', () => {
  it('tCount picks the CLDR category, so French renders 0 in the singular', () => {
    expect(tCount('logs.count', 1)).toBe('1 entry');
    expect(tCount('logs.count', 2)).toBe('2 entries');
    expect(tCount('logs.count', 0)).toBe('0 entries');

    i18n.set('fr');
    // CLDR fr puts 0 in `one` — the hand-rolled `count === 1` got this wrong.
    expect(tCount('logs.count', 0)).toBe('0 entrée');
    expect(tCount('logs.count', 1)).toBe('1 entrée');
    expect(tCount('logs.count', 2)).toBe('2 entrées');

    i18n.set('de');
    expect(tCount('logs.count', 1)).toBe('1 Eintrag');
    expect(tCount('logs.count', 2)).toBe('2 Einträge');

    i18n.set('en');
  });

  it('routes numeric params through the locale number format', () => {
    const en = t('logs.showingOf', { shown: 100, total: 5000 });
    expect(en).toBe('Showing 100 of 5,000');

    i18n.set('fr');
    const fr = t('logs.showingOf', { shown: 100, total: 5000 });
    expect(fr).toBe(`100 sur ${new Intl.NumberFormat('fr').format(5000)} affichées`);
    expect(fr).not.toBe('100 sur 5000 affichées');
    expect(tCount('logs.count', 5000)).toBe(`${new Intl.NumberFormat('fr').format(5000)} entrées`);

    i18n.set('de');
    expect(tCount('logs.count', 5000)).toBe(`${new Intl.NumberFormat('de').format(5000)} Einträge`);

    i18n.set('en');
  });

  it('leaves string params and param-less keys alone', () => {
    expect(t('about.version', { version: '4.6.0' })).toBe('Version 4.6.0');
    expect(t('settings.formatTemplatePlaceholder')).toContain('{artist}');
  });
});

describe('<html lang> and cross-webview convergence (#620)', () => {
  it('retags the document on every locale change, ignoring unknown locales', () => {
    i18n.set('de');
    expect(document.documentElement.lang).toBe('de');
    i18n.set('fr');
    expect(document.documentElement.lang).toBe('fr');

    i18n.set('en');
    expect(document.documentElement.lang).toBe('en');
    i18n.set('zz' as never);
    expect(document.documentElement.lang).toBe('en');
    expect(i18n.locale).toBe('en');
  });

  it('converges on a locale another webview wrote to localStorage', () => {
    // A detached Logs/Settings window owns its own store instance; the main
    // window's switch reaches it only through the `storage` event.
    window.dispatchEvent(
      new StorageEvent('storage', { key: 'presencejam:locale', newValue: 'de' })
    );
    expect(i18n.locale).toBe('de');
    expect(document.documentElement.lang).toBe('de');

    // Same value, unknown value, unrelated key, and the pre-4.7 bare key —
    // which is no longer a channel at all — are all no-ops.
    window.dispatchEvent(
      new StorageEvent('storage', { key: 'presencejam:locale', newValue: 'de' })
    );
    window.dispatchEvent(
      new StorageEvent('storage', { key: 'presencejam:locale', newValue: 'zz' })
    );
    window.dispatchEvent(new StorageEvent('storage', { key: 'locale', newValue: 'fr' }));
    window.dispatchEvent(new StorageEvent('storage', { key: 'presencejam:theme', newValue: 'fr' }));
    expect(i18n.locale).toBe('de');

    i18n.set('en');
    expect(localStorage.getItem('presencejam:locale')).toBe('en');
  });

  it('seeds the pre-4.7 bare key into the namespaced one on module load (#909)', async () => {
    // The one path the other tests cannot reach: the store tags `lang` and
    // migrates the legacy mirror when it is first imported. `vi.resetModules`
    // + a dynamic import is the only way to re-run that load (module-init
    // boundary), so this test is last.
    localStorage.setItem('locale', 'de');
    localStorage.removeItem('presencejam:locale');
    document.documentElement.lang = 'en';
    vi.resetModules();

    const reloaded = await import('$lib/i18n/store.svelte');
    // A returning user keeps the language the bare key carried…
    expect(document.documentElement.lang).toBe('de');
    expect(localStorage.getItem('presencejam:locale')).toBe('de');
    // …and the bare key is gone, so nothing reads it again.
    expect(localStorage.getItem('locale')).toBeNull();

    // The reloaded instance owns the document now: #892's guard means the
    // statically imported store (already English) would not retag it.
    await reloaded.i18n.set('en');
    expect(document.documentElement.lang).toBe('en');
  });
});
