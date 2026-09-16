/**
 * #488 (fe slice) — real t() behavior + call-site key coverage.
 *
 * Fail pre-fix: a runtime-missing key threw TypeError instead of
 * degrading. Pass post-fix: unknown keys degrade to the key string
 * (with a dev warn), params still substitute, and every static
 * t('...') call-site resolves against the en key set.
 * (Dict copy — PageHeader default, Onboarding placeholders,
 * reconnect.reconnectSpotify delete — is owned by the ux slice.)
 */
import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
// Static: the i18n barrel is a pure TS module (only .svelte.ts leaf
// imports need the svelte plugin, configured in vitest.config.js).
import { t, i18n } from '$lib/i18n';

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
      const re = /\bt\(\s*'([^']+)'/g;
      let m: RegExpExecArray | null;
      while ((m = re.exec(body)) !== null) {
        if (!enKeys.includes(m[1]))
          missing.push(`${f.replace(root + '/', '')}: ${m[1]}`);
      }
    }
    expect(missing).toEqual([]);
  });
});
