/**
 * #745 — the build version is normal-size text, so its resolved color must
 * clear WCAG AA against the page background in both themes. Read the actual
 * component selector and palette tokens so an opacity or token regression is
 * caught without pinning the test to explanatory prose.
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const appCss = readFileSync('src/app.css', 'utf8');
const pageSource = readFileSync('src/routes/+page.svelte', 'utf8');

type Rgb = [number, number, number];
type Tokens = Map<string, string>;

function declarations(block: string): Map<string, string> {
  const values = new Map<string, string>();
  for (const declaration of block.matchAll(/([\w-]+)\s*:\s*([^;]+);/g)) {
    values.set(declaration[1], declaration[2].trim());
  }
  return values;
}

function themeTokens(theme: 'dark' | 'light'): Tokens {
  const selector = `[data-theme='${theme}']`;
  const selectorStart = appCss.indexOf(selector);
  if (selectorStart < 0) throw new Error(`${selector} not found in app.css`);

  const blockStart = appCss.indexOf('{', selectorStart);
  const blockEnd = appCss.indexOf('}', blockStart);
  if (blockStart < 0 || blockEnd < 0) throw new Error(`${selector} has no declaration block`);

  return declarations(appCss.slice(blockStart + 1, blockEnd));
}

function rgb(value: string): Rgb {
  const match = value.match(/^#([0-9a-f]{6})$/i);
  if (!match) throw new Error(`Expected a six-digit palette token, got ${value}`);
  const hex = match[1];
  return [
    Number.parseInt(hex.slice(0, 2), 16),
    Number.parseInt(hex.slice(2, 4), 16),
    Number.parseInt(hex.slice(4, 6), 16)
  ];
}

function composite(foreground: Rgb, background: Rgb, alpha: number): Rgb {
  return foreground.map((channel, index) => channel * alpha + background[index] * (1 - alpha)) as Rgb;
}

function luminance(color: Rgb): number {
  const channels = color.map((channel) => {
    const normalized = channel / 255;
    return normalized <= 0.04045
      ? normalized / 12.92
      : ((normalized + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrast(foreground: Rgb, background: Rgb): number {
  const lighter = Math.max(luminance(foreground), luminance(background));
  const darker = Math.min(luminance(foreground), luminance(background));
  return (lighter + 0.05) / (darker + 0.05);
}

describe('#745 build version contrast', () => {
  it('meets 4.5:1 against the page background in both themes', () => {
    const style = pageSource.match(/<style>([\s\S]*?)<\/style>/)?.[1];
    expect(style).toBeDefined();

    const versionRule = style?.match(/\.version\s*\{([^}]*)\}/)?.[1];
    expect(versionRule).toBeDefined();
    const version = declarations(versionRule ?? '');
    const colorReference = version.get('color');
    const opacity = Number(version.get('opacity') ?? '1');
    expect(colorReference).toMatch(/^var\(--[\w-]+\)$/);
    expect(Number.isFinite(opacity)).toBe(true);
    expect(opacity).toBeGreaterThanOrEqual(0);
    expect(opacity).toBeLessThanOrEqual(1);

    for (const theme of ['dark', 'light'] as const) {
      const tokens = themeTokens(theme);
      const foregroundToken = colorReference?.match(/var\(--([\w-]+)\)/)?.[1] ?? '';
      const background = rgb(tokens.get('--bg-base') ?? '');
      const foreground = composite(rgb(tokens.get(`--${foregroundToken}`) ?? ''), background, opacity);

      expect(contrast(foreground, background), `${theme} version contrast`).toBeGreaterThanOrEqual(4.5);
    }
  });
});
