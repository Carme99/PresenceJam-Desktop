/**
 * #953 — base link style for classless anchors.
 *
 * Classless anchors (inline `<a>` in Onboarding step 1, Reconnect's Teams
 * verification URL) fell back to UA blue / visited purple on `--bg-surface`,
 * which measures ~1.8:1 / ~1.5:1 in dark. The fix is a base `a` primitive in
 * src/app.css so the palette owns the link state in every painted theme.
 * This test guards the primitive so it cannot be dropped again.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';

// Vitest runs with the repo root as cwd (`npm test`), so `src/app.css` is
// resolvable directly. `hygiene.test.ts` already establishes this pattern.
const appCss = readFileSync('src/app.css', 'utf8');

/**
 * Extract every top-level rule body whose selector list contains a bare `a`.
 * Walks the source manually so that `}` characters inside comments or nested
 * at-rules (e.g. `@media`, `@keyframes`) do not split real declarations.
 */
function linkRuleBodies(): string[] {
  const bodies: string[] = [];
  const selectorStarts: number[] = [];
  // Find each `a {` selector declaration; we only care about the body.
  const selectorRe = /(^|[\s,{}])(a)\s*\{/g;
  let m: RegExpExecArray | null;
  while ((m = selectorRe.exec(appCss)) !== null) {
    const bodyStart = m.index + m[0].length;
    // Find the matching `}` at this brace depth, respecting nested braces
    // inside `@media`, `@keyframes`, etc.
    let depth = 1;
    let i = bodyStart;
    while (i < appCss.length && depth > 0) {
      const ch = appCss[i];
      if (ch === '{') depth += 1;
      else if (ch === '}') depth -= 1;
      i += 1;
    }
    if (depth !== 0) continue;
    bodies.push(appCss.slice(bodyStart, i - 1));
  }
  return bodies;
}

describe('base link style (#953)', () => {
  it('declares a base `a` rule in src/app.css', () => {
    const bodies = linkRuleBodies();
    expect(bodies.length, 'expected at least one `a { ... }` rule').toBeGreaterThan(0);
  });

  it('base `a` rule colours links with the app palette token, not UA blue', () => {
    const bodies = linkRuleBodies();
    const base = bodies
      .map((b) => b.toLowerCase())
      .find((b) => /^\s*color\s*:/.test(b) || /\n\s*color\s*:/.test(b));
    expect(base, 'base `a` rule must set `color`').toBeDefined();
    expect(base).toContain('var(--accent-text)');
    // Reject explicit hex/keyword colours that would freeze the palette.
    expect(base).not.toMatch(/#[0-9a-f]{3,8}\b/i);
  });

  it('overrides UA visited so visited links stay on the app palette', () => {
    // The visited block must use the accent-text token, not the UA purple.
    expect(appCss).toMatch(/a:visited\s*\{[^}]*color:\s*var\(--accent-text\)/);
  });

  it('both painted themes declare `color-scheme` so native popups match', () => {
    expect(appCss).toMatch(/\[data-theme=['"]dark['"]\][\s\S]*?color-scheme:\s*dark/);
    expect(appCss).toMatch(/\[data-theme=['"]light['"]\][\s\S]*?color-scheme:\s*light/);
  });
});
