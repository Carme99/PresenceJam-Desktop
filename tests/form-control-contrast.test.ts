/**
 * #740 — a form control's boundary must clear the 3:1 non-text contrast
 * minimum (WCAG 1.4.11).
 *
 * An `input` / `textarea` / `select` is identified by its edge and its fill;
 * nothing else marks where it starts. The shared rule in `src/app.css` painted
 * that edge with `--border`, which is a *decorative* divider colour: 1.32:1 on
 * `--bg-surface` and 1.10:1 on the `--bg-elevated` field fill in the dark
 * theme (1.29:1 / 1.14:1 light). A low-vision user cannot see where the field
 * begins or how wide it is.
 *
 * The guard resolves the real cascade out of the token file — which custom
 * property the `input` rule paints, and the two surfaces that border has to be
 * seen against — and computes the WCAG ratio from it. It asserts the *ratio*,
 * never a hex value: a palette revision that keeps the ratio is a pass, one
 * that quietly slides the border back toward the surface is a failure.
 *
 * It fails against the pre-fix stylesheet in two independent ways:
 * `--border-input` is not declared at all, and the `input` rule resolves to
 * `--border`, which measures 1.32:1 / 1.10:1 (dark) and 1.29:1 / 1.14:1 (light).
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import postcss from 'postcss';

const appCss = readFileSync('src/app.css', 'utf8');

type Rgb = [number, number, number];

const NON_TEXT_MINIMUM = 3;

const THEMES = ['dark', 'light'] as const;

/** WCAG 2.x relative luminance of an sRGB triple. */
function luminance([r, g, b]: Rgb): number {
  const channel = (value: number) => {
    const normalized = value / 255;
    return normalized <= 0.04045 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** The WCAG contrast ratio between two opaque sRGB triples. */
function contrast(a: Rgb, b: Rgb): number {
  const [lighter, darker] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (lighter + 0.05) / (darker + 0.05);
}

function toRgb(value: string): Rgb {
  const hex = value.trim().replace('#', '');
  if (!/^[0-9a-fA-F]{6}$/.test(hex)) {
    throw new Error(`Expected a #RRGGBB token value, got: ${value}`);
  }
  return [
    Number.parseInt(hex.slice(0, 2), 16),
    Number.parseInt(hex.slice(2, 4), 16),
    Number.parseInt(hex.slice(4, 6), 16)
  ];
}

/** Each theme's custom properties, read out of its own block in the token file. */
function themeTokens(): Record<string, Record<string, string>> {
  const themes: Record<string, Record<string, string>> = {};

  postcss.parse(appCss).walkRules((rule) => {
    const selectors = rule.selectors.map((selector) => selector.trim());
    const name = selectors.includes("[data-theme='dark']")
      ? 'dark'
      : selectors.includes("[data-theme='light']")
        ? 'light'
        : null;
    if (name === null) return;

    themes[name] ??= {};
    rule.walkDecls(/^--/, (decl) => {
      themes[name][decl.prop] = decl.value.trim();
    });
  });

  return themes;
}

/**
 * The colour half of the `border` shorthand the shared field rule declares:
 * the width and the style keyword dropped, the custom property reference left
 * to be resolved against a theme.
 */
function fieldBorderDeclaration(): string {
  let found: string | undefined;

  postcss.parse(appCss).walkRules((rule) => {
    if (found !== undefined) return;
    const selector = rule.selectors.join(',').replace(/\s+/g, '');
    if (selector !== 'input,textarea,select') return;
    rule.walkDecls(/^border(-color)?$/, (decl) => {
      found ??= decl.value;
    });
  });

  if (found === undefined) throw new Error('No `input, textarea, select` rule in src/app.css');
  return found
    .replace(/\b(solid|dashed|dotted|double|none|hidden)\b/g, '')
    .replace(/[\d.]+px/g, '')
    .trim();
}

/** Resolve `var(--token)` against one theme's token table. */
function resolve(value: string, theme: Record<string, string>): string {
  const reference = /^var\(\s*(--[\w-]+)\s*\)$/.exec(value);
  if (!reference) return value;
  const resolved = theme[reference[1]];
  if (!resolved) throw new Error(`Unresolved token in the theme block: ${reference[1]}`);
  return resolved;
}

describe('#740 form-control boundaries clear the 3:1 non-text minimum', () => {
  it('declares a dedicated boundary token in both themes', () => {
    const tokens = themeTokens();
    for (const theme of THEMES) {
      expect(tokens[theme]?.['--border-input'], `${theme} --border-input`).toMatch(/^#[0-9A-Fa-f]{6}$/);
    }
  });

  it('paints the field border with that token, not the decorative divider colour', () => {
    const declaration = fieldBorderDeclaration();
    expect(declaration).toBe('var(--border-input)');
  });

  it('measures at least 3:1 against both surfaces a field can sit on', () => {
    const tokens = themeTokens();
    const declaration = fieldBorderDeclaration();
    const measured: string[] = [];

    for (const theme of THEMES) {
      const block = tokens[theme];
      expect(block, `${theme} token block`).toBeTruthy();
      const border = toRgb(resolve(declaration, block));
      for (const surface of ['--bg-surface', '--bg-elevated'] as const) {
        const ratio = contrast(border, toRgb(resolve(block[surface], block)));
        measured.push(`${theme} vs ${surface}: ${ratio.toFixed(2)}:1`);
        expect(ratio, `${theme} ${surface} (${surface} = ${block[surface]})`).toBeGreaterThanOrEqual(
          NON_TEXT_MINIMUM
        );
      }
    }

    // The numbers are part of the deliverable — the issue asks for the measured
    // ratios — so surface them in the run output rather than leaving a
    // reviewer to recompute them from the palette.
    console.log(`#740 measured field-boundary contrast — ${measured.join('; ')}`);
  });

  it('leaves the decorative divider colour alone, and too weak to pass as a boundary', () => {
    const tokens = themeTokens();
    expect(tokens.dark?.['--border'], 'dark --border').toBe('#2A2F5A');
    expect(tokens.light?.['--border'], 'light --border').toBe('#DFE2F0');

    // If a later "simplification" folds `--border-input` back into `--border`,
    // the assertion above stops measuring a *field* boundary and starts
    // measuring this one — so pin that the divider stays under the minimum.
    for (const theme of THEMES) {
      const divider = toRgb(tokens[theme]['--border']);
      expect(contrast(divider, toRgb(tokens[theme]['--bg-surface'])), `${theme} --border`).toBeLessThan(
        NON_TEXT_MINIMUM
      );
    }
  });
});
