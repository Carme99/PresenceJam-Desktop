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
// contains an apostrophe; prettier wraps the long ones onto the next line.
function dictEntries(source: string): { key: string; value: string }[] {
  const out: { key: string; value: string }[] = [];
  const re = /^  '([^']+)':\s*(?:'([^']*)'|"([^"]*)"),$/gm;
  let m: RegExpExecArray | null;
  while ((m = re.exec(source)) !== null) {
    out.push({ key: m[1], value: m[2] ?? m[3] });
  }
  return out;
}

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
    for (const src of [enSrc, deSrc, frSrc]) {
      expect(src).toContain("'settings.episodeFormatHint'");
    }
  });

  it('settings.reconnectSpotify stays live (Reconnect view uses it)', () => {
    // #426 delete itself is ux-owned; this slice only guards the live
    // sibling the Reconnect view renders.
    expect(enSrc).toContain("'settings.reconnectSpotify'");
    for (const s of [enSrc, deSrc, frSrc]) {
      expect(s).toContain("'common.back'");
      expect(s).toContain("'settings.formatTemplatePlaceholder'");
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
    window.dispatchEvent(new StorageEvent('storage', { key: 'locale', newValue: 'de' }));
    expect(i18n.locale).toBe('de');
    expect(document.documentElement.lang).toBe('de');

    // Same value, unknown value, unrelated key: all no-ops.
    window.dispatchEvent(new StorageEvent('storage', { key: 'locale', newValue: 'de' }));
    window.dispatchEvent(new StorageEvent('storage', { key: 'locale', newValue: 'zz' }));
    window.dispatchEvent(new StorageEvent('storage', { key: 'presencejam:theme', newValue: 'fr' }));
    expect(i18n.locale).toBe('de');

    i18n.set('en');
    expect(localStorage.getItem('locale')).toBe('en');
  });

  it('tags the document with the stored locale on module load', async () => {
    // The one path the other tests cannot reach: the store tags `lang` when
    // it is first imported. `vi.resetModules` + a dynamic import is the only
    // way to re-run that load (module-init boundary), so this test is last.
    localStorage.setItem('locale', 'de');
    document.documentElement.lang = 'en';
    vi.resetModules();

    await import('$lib/i18n/store.svelte');
    expect(document.documentElement.lang).toBe('de');

    i18n.set('en');
    expect(document.documentElement.lang).toBe('en');
  });
});
