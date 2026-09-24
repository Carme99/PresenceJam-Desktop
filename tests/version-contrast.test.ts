/**
 * #745 — the build version is normal-size text, so its resolved color must
 * clear WCAG AA against the page background in both themes. Mount the page
 * with the same backend seams as the navigation tests and inspect the actual
 * rendered label under the painted theme.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import { readFileSync } from 'node:fs';
import postcss from 'postcss';
import { color, serializeRGB } from '@csstools/css-color-parser';
import { parseComponentValue } from '@csstools/css-parser-algorithms';
import { tokenize } from '@csstools/css-tokenizer';

const { invoke, detachedPaneStore } = vi.hoisted(() => {
  const paneValue = { logs: false, settings: false };
  return {
    invoke: vi.fn(),
    detachedPaneStore: {
      subscribe(run: (value: typeof paneValue) => void) {
        run(paneValue);
        return () => {};
      }
    }
  };
});

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {})
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ label: 'main' })
}));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => true),
  requestPermission: vi.fn(async () => 'granted')
}));
vi.mock('$lib/stores/detach', () => ({
  detachedPanes: detachedPaneStore,
  focusDetached: vi.fn(async () => {}),
  popOut: vi.fn(async () => {}),
  popIn: vi.fn(async () => {}),
  reconcileDetachedPanes: vi.fn(async () => {})
}));

import Page from '../src/routes/+page.svelte';
import { currentView, pendingMenuNav, settingsDirty } from '$lib/stores/app';
import { defaultConfig } from '$lib/stores/config';
import { resetAuthFlow } from '$lib/stores/authFlow.svelte';

const appCss = readFileSync('src/app.css', 'utf8');
const pageSource = readFileSync('src/routes/+page.svelte', 'utf8');
const pageStyle = pageSource.match(/<style(?:\s[^>]*)?>([\s\S]*?)<\/style\s*>/i)?.[1];
if (!pageStyle) throw new Error('The root page component has no style block');

type Rgb = [number, number, number];
type Color = { rgb: Rgb; alpha: number };

function resolveValue(value: string, seen = new Set<string>()): string {
  let resolved = value.trim();
  const variable = /var\(\s*(--[\w-]+)(?:\s*,([^()]*))?\s*\)/.exec(resolved);
  if (!variable) return resolved;

  const name = variable[1];
  if (seen.has(name)) throw new Error(`Circular color token: ${name}`);
  const nextSeen = new Set(seen).add(name);
  const tokenValue = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const replacement = tokenValue || variable[2]?.trim();
  if (!replacement) throw new Error(`Unresolved color token: ${name}`);
  return resolveValue(resolved.replace(variable[0], replacement), nextSeen);
}

function cssColor(value: string): Color {
  const resolved = resolveValue(value);
  if (resolved === 'transparent') return { rgb: [0, 0, 0], alpha: 0 };

  // Convert modern CSS color syntaxes (including oklch and alpha hex) with
  // the CSS Tools parser, then let the browser's CSSStyleDeclaration parser
  // normalize the sRGB result for the numeric channels.
  const componentValue = parseComponentValue(tokenize({ css: resolved }));
  if (!componentValue) throw new Error(`Unsupported CSS color: ${value}`);
  const parsed = color(componentValue);
  const normalized = parsed ? serializeRGB(parsed).toString() : resolved;
  const probe = document.createElement('span');
  probe.style.color = normalized;
  const browserColor = probe.style.color;
  const match = /^rgba?\(([^)]+)\)$/i.exec(browserColor);
  if (!match) throw new Error(`Unsupported CSS color: ${value}`);
  const parts = match[1].split(/\s*[,/]\s*/);
  const channel = (part: string, scale = 255) => {
    const number = Number.parseFloat(part);
    return part.endsWith('%') ? (number / 100) * scale : number;
  };
  return {
    rgb: [channel(parts[0]), channel(parts[1]), channel(parts[2])],
    alpha: parts[3] === undefined ? 1 : channel(parts[3], 1)
  };
}

function composite(foreground: Color, background: Color): Color {
  return {
    rgb: foreground.rgb.map(
      (value, index) => value * foreground.alpha + background.rgb[index] * (1 - foreground.alpha)
    ) as Rgb,
    alpha: 1
  };
}

function luminance({ rgb }: Color): number {
  const channels = rgb.map((value) => {
    const normalized = value / 255;
    return normalized <= 0.04045
      ? normalized / 12.92
      : ((normalized + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrast(foreground: Color, background: Color): number {
  const lighter = Math.max(luminance(foreground), luminance(background));
  const darker = Math.min(luminance(foreground), luminance(background));
  return (lighter + 0.05) / (darker + 0.05);
}

function effectiveOpacity(element: Element): number {
  let opacity = 1;
  for (let current: Element | null = element; current; current = current.parentElement) {
    const value = Number.parseFloat(getComputedStyle(current).opacity);
    opacity *= Number.isFinite(value) ? value : 1;
  }
  return opacity;
}

function effectiveBackground(element: Element): Color {
  const layers: Color[] = [];
  for (let current: Element | null = element; current; current = current.parentElement) {
    const styles = getComputedStyle(current);
    const backgroundColor = styles.backgroundColor.trim();
    if (backgroundColor) {
      const background = cssColor(backgroundColor);
      if (background.alpha > 0) {
        layers.push(background);
        continue;
      }
    }

    const backgroundShorthand = styles.background.trim();
    if (
      !backgroundShorthand ||
      backgroundShorthand === 'none' ||
      /^(?:linear-gradient|radial-gradient|conic-gradient|url\()/i.test(backgroundShorthand)
    ) {
      continue;
    }
    const background = cssColor(backgroundShorthand);
    if (background.alpha > 0) layers.push(background);
  }
  const canvas: Color = { rgb: [255, 255, 255], alpha: 1 };
  return layers.reduceRight((under, layer) => composite(layer, under), canvas);
}

function hasMatchingColorRule(element: Element): boolean {
  for (const sheet of document.styleSheets) {
    for (const rule of sheet.cssRules) {
      if (!('selectorText' in rule) || !('style' in rule)) continue;
      const styleRule = rule as CSSStyleRule;
      if (
        element.matches(styleRule.selectorText) &&
        styleRule.style.getPropertyValue('color').trim()
      ) {
        return true;
      }
    }
  }
  return false;
}

function applyProductionStyles() {
  const style = document.createElement('style');
  style.dataset.test = 'app-css';
  style.textContent = postcss.parse(`${appCss}\n${pageStyle}`).toString();
  document.head.appendChild(style);
  return style;
}

function mockBackend() {
  invoke.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'is_onboarding_complete':
        return true;
      case 'load_config':
        return structuredClone(defaultConfig);
      case 'get_sync_status':
        return { is_syncing: false, current_track: null, spotify_connected: true, teams_connected: true };
      default:
        return undefined;
    }
  });
}

async function mountPage(theme: 'dark' | 'light') {
  document.documentElement.setAttribute('data-theme', theme);
  mockBackend();
  const rendered = render(Page);
  for (let i = 0; i < 32; i++) await Promise.resolve();
  return rendered;
}

beforeEach(() => {
  invoke.mockReset();
  resetAuthFlow();
  currentView.set('dashboard');
  settingsDirty.set(false);
  pendingMenuNav.set(null);
  document.documentElement.removeAttribute('data-theme');
  document.head.querySelectorAll('style[data-test="app-css"]').forEach((element) => element.remove());
  applyProductionStyles();
});

afterEach(() => {
  cleanup();
  document.head.querySelectorAll('style[data-test="app-css"]').forEach((element) => element.remove());
});

describe('#745 build version contrast', () => {
  it('renders a readable build version in both themes', async () => {
    for (const theme of ['dark', 'light'] as const) {
      const { container } = await mountPage(theme);
      const version = container.querySelector('.version');
      expect(version, `${theme} build version element`).not.toBeNull();
      expect(version?.textContent?.trim(), `${theme} build version text`).toBeTruthy();

      // The DOM and CSSOM are the contract: a renamed class or an orphaned
      // rule cannot leave a passing source-only test behind.
      expect(hasMatchingColorRule(version!), `${theme} build version color rule`).toBe(true);
      const foreground = cssColor(getComputedStyle(version!).color);
      const opacity = effectiveOpacity(version!);
      const background = effectiveBackground(version!);
      const paintedForeground = composite(
        { ...foreground, alpha: foreground.alpha * opacity },
        background
      );
      expect(contrast(paintedForeground, background), `${theme} version contrast`).toBeGreaterThanOrEqual(4.5);
      cleanup();
    }
  });
});
