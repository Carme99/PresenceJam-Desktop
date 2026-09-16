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

  it('no dictionary carries the duplicated reconnect status key (#619)', () => {
    for (const source of [enSrc, deSrc, frSrc]) {
      expect(dictKeys(source)).not.toContain('reconnect.needsReconnect');
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
