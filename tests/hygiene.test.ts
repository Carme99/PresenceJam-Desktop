/**
 * #904 — the design-token hygiene guard.
 *
 * `src/app.css` is the design-token source of truth (AGENTS.md §5); no raw
 * colour literal belongs in component CSS. Before this file existed the rule
 * was unenforced, and the appearance picker's theme previews were the proof:
 * `.swatch-dark` / `.swatch-light` / `.swatch-system` carried verbatim copies
 * of `--bg-base` / `--bg-elevated` in both palettes, so a palette revision
 * left the picker advertising colours the app no longer paints.
 *
 * The second rule pairs with it: the preview tokens those swatches consume
 * must exist on `:root` (not on a theme block — the light swatch is rendered
 * while the dark theme is live) and must still mirror the palettes they
 * stand in for.
 *
 * Fail before the fix: the three `.swatch-*` rules in `Settings.svelte` each
 * contain two `#RRGGBB` literals, and no `--preview-*` token exists at all.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const SRC = 'src';
/** The one file allowed to hold colour literals: it *defines* the tokens. */
const TOKEN_FILE = 'src/app.css';

/** Every `.css` / `.svelte` file under `src/`, token file included. */
function styleFiles(dir = SRC): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) return styleFiles(full);
    return /\.svelte$|\.css$/.test(entry) ? [full] : [];
  });
}

/**
 * The CSS text a browser would actually apply: `<style>` blocks only, with
 * their comments removed. Comments matter — `/* #904 … *\/` is an issue
 * reference, not a colour, and a guard that trips on it gets switched off.
 */
function applicableCss(path: string): string {
  const source = readFileSync(path, 'utf8');
  const blocks = path.endsWith('.svelte')
    ? [...source.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1])
    : [source];
  return blocks
    .join('\n')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(VAR_FALLBACK, 'var(--token)');
}

/**
 * One carve-out, deliberately narrow: `var(--token, #fallback)` is a
 * defensive default for a token that may not exist yet, not a second copy of
 * a palette value — the declaration still paints through the token. Those are
 * stripped before the scan; a literal used on its own (which is what the
 * `.swatch-*` rules did) is still an offence.
 */
const VAR_FALLBACK = /var\(\s*--[\w-]+\s*,\s*#[0-9a-fA-F]{3,8}\s*\)/g;

/** `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` as a standalone value. */
const HEX_LITERAL = /#[0-9a-fA-F]{3,8}\b/g;

const appCss = readFileSync(TOKEN_FILE, 'utf8');

/** The value a custom property resolves to in `src/app.css`. */
function declaredToken(name: string, css = appCss): string {
  const match = new RegExp(`^\\s*${name}:\\s*([^;]+);`, 'm').exec(css);
  return match?.[1]?.trim() ?? '';
}

/** The block of `src/app.css` a selector opens, up to its closing brace. */
function themeBlock(selector: string): string {
  const start = appCss.indexOf(`\n${selector} {`);
  if (start < 0) return '';
  return appCss.slice(start, appCss.indexOf('}', start));
}

describe('colour literals outside src/app.css (#904)', () => {
  const offenders: string[] = [];

  for (const file of styleFiles()) {
    if (file === TOKEN_FILE) continue;
    const css = applicableCss(file);
    for (const match of css.matchAll(HEX_LITERAL)) {
      const line = css.slice(0, match.index).split('\n').length;
      offenders.push(`${file}:${line}: ${match[0]}`);
    }
  }

  it('finds none', () => {
    expect(offenders).toEqual([]);
  });
});

describe('theme preview tokens (#904)', () => {
  it('declares both palettes on :root, not inside a theme block', () => {
    // `--preview-dark-1` on a `[data-theme]` block would repaint with the live
    // theme, which is the bug: the picker shows the *other* palette too.
    expect(themeBlock("[data-theme='dark']")).not.toContain('--preview-');
    expect(themeBlock("[data-theme='light']")).not.toContain('--preview-');

    expect(declaredToken('--preview-dark-1')).toMatch(/^#[0-9A-Fa-f]{6}$/);
    expect(declaredToken('--preview-dark-2')).toMatch(/^#[0-9A-Fa-f]{6}$/);
    expect(declaredToken('--preview-light-1')).toMatch(/^#[0-9A-Fa-f]{6}$/);
    expect(declaredToken('--preview-light-2')).toMatch(/^#[0-9A-Fa-f]{6}$/);
  });

  it('mirrors the surfaces it stands in for, so a palette revision moves both', () => {
    const dark = themeBlock("[data-theme='dark']");
    const light = themeBlock("[data-theme='light']");

    expect(declaredToken('--preview-dark-1')).toBe(declaredToken('--bg-base', dark));
    expect(declaredToken('--preview-dark-2')).toBe(declaredToken('--bg-elevated', dark));
    expect(declaredToken('--preview-light-1')).toBe(declaredToken('--bg-base', light));
    expect(declaredToken('--preview-light-2')).toBe(declaredToken('--bg-surface', light));
  });

  it('gives the swatch a density-scaled height', () => {
    expect(declaredToken('--swatch-h')).toBe('64px');
    const compact = appCss.slice(appCss.indexOf("[data-density='compact'] {"));
    expect(new RegExp(`--swatch-h:\\s*([^;]+);`).exec(compact)?.[1]?.trim()).toBe('48px');
  });
});

describe('token files are the only place a colour literal lives (#904)', () => {
  it('actually scans the whole of src/', () => {
    // A guard that silently scans one file is not a guard.
    const scanned = styleFiles().map((f) => relative(SRC, f));
    expect(scanned).toContain('app.css');
    expect(scanned.some((f) => f.endsWith('Settings.svelte'))).toBe(true);
  });
});

/**
 * #903 — the shared button primitives.
 *
 * `.btn-refresh` and `.snooze-resume` (Dashboard) and `.download-btn` /
 * `.quit-btn` (UpdatePrompt) each re-declared padding, a border and a radius
 * locally, so the same class of secondary action rendered at different heights
 * and radii — and the refresh button's hover was a `filter: brightness(1.08)`
 * that bypassed the theme tokens entirely, so a palette revision could not
 * reach it. These two shapes are the shapes that defect took.
 *
 * Fails against the pre-fix components: `Dashboard.svelte` declared
 * `filter: brightness(1.08)`, and `padding` / `border` / `border-radius` on
 * `.btn-refresh` and `.snooze-resume`; `UpdatePrompt.svelte` declared padding
 * on `.download-btn` and `.quit-btn`.
 *
 * The rendered half — matching heights and radii between equivalent actions,
 * and the `prefers-reduced-motion` override surviving the rewrite — is a layout
 * contract and lives in `tests/browser/button-primitives.spec.ts`.
 */

/** Every component's `<style>` text, with comments and var() fallbacks stripped. */
function componentStyles(): { file: string; css: string }[] {
  return styleFiles()
    .filter((file) => file.endsWith('.svelte'))
    .map((file) => ({ file, css: applicableCss(file) }));
}

/** The declaration block of every rule in `css` whose selector names `className`. */
function rulesFor(css: string, className: string): string[] {
  const bodies: string[] = [];
  for (const rule of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (new RegExp(`\\.${className}(?![\\w-])`).test(rule[1])) bodies.push(rule[2]);
  }
  return bodies;
}

describe('no component paints anything through a CSS filter (#903)', () => {
  it('finds none', () => {
    const offenders = componentStyles()
      .filter(({ css }) => /(^|[;{\s])filter\s*:/.test(css))
      .map(({ file }) => file);
    expect(offenders).toEqual([]);
  });
});

describe('no component re-declares a button box (#903)', () => {
  /** One-off classes that existed only to re-state the shared geometry. */
  const RETIRED_BUTTON_CLASSES = ['btn-refresh', 'snooze-resume', 'quit-btn', 'download-btn'];
  const BOX_PROPERTY = /(^|[;{\s])(padding|padding-[\w-]+|border|border-[\w-]+|border-radius)\s*:/;

  it('finds none', () => {
    const offenders: string[] = [];
    for (const { file, css } of componentStyles()) {
      for (const className of RETIRED_BUTTON_CLASSES) {
        for (const body of rulesFor(css, className)) {
          if (BOX_PROPERTY.test(body)) offenders.push(`${file}: .${className} { ${body.trim()} }`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });
});
/**
 * #960 — fixed control, artwork and log-column sizes are design tokens.
 *
 * Compact density (`[data-density='compact']` in `src/app.css`) overrode only
 * the `--sp-*` / `--fs-*` / `--lh-*` scales, so every component consuming
 * those tokens tightened with the toggle — but layout-bearing sizes written
 * as literals (36px icon buttons, 88px album art, the 88px log timestamp
 * column, the 28px banner dismiss button) stayed at comfortable sizes and
 * the toggle read as inconsistent rather than as a density change.
 *
 * Fails before the fix: `.album-art` (Dashboard), `.log-entry` (LogViewer),
 * `.dismiss-btn` (UpdatePrompt) and `.info-icon` (Reconnect) carry literal
 * `px` sizes, Dashboard re-states the 36px icon box locally, and no
 * `--ctl-h` / `--art-lg` / `--badge-fs` / `--log-col-ts` / `--spinner-size`
 * tokens exist at all.
 */

/** The `[data-density='compact']` override block of `src/app.css`. */
function compactDensityBlock(): string {
  const start = appCss.indexOf("[data-density='compact'] {");
  return appCss.slice(start, appCss.indexOf('}', start));
}

/** The `Npx` value `--token` carries inside `block` (`null` when absent). */
function tokenPx(name: string, block: string): number | null {
  const match = new RegExp(`${name}:\\s*(\\d+(?:\\.\\d+)?)px;`).exec(block);
  return match ? Number(match[1]) : null;
}

/**
 * `css` with its `@media` blocks removed. Responsive floors (Dashboard's
 * 640px 32px icon rule) stay literal on purpose — they are viewport answers,
 * not density sizes — so the literal scan below must not see them.
 */
function outsideMedia(css: string): string {
  let out = '';
  let i = 0;
  while (i < css.length) {
    const at = css.indexOf('@media', i);
    if (at < 0) { out += css.slice(i); break; }
    out += css.slice(i, at);
    const open = css.indexOf('{', at);
    let depth = 0;
    let j = open;
    for (; j < css.length; j++) {
      if (css[j] === '{') depth++;
      else if (css[j] === '}') { depth--; if (depth === 0) { j++; break; } }
    }
    i = j;
  }
  return out;
}

/** Bodies of rules in `css` whose selector is exactly `selector`. */
function exactRules(css: string, selector: string): string[] {
  const bodies: string[] = [];
  for (const rule of outsideMedia(css).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (rule[1].trim() === selector) bodies.push(rule[2]);
  }
  return bodies;
}

/** The value a `prop` declaration carries inside a rule `body` (`''` when absent). */
function declaredProp(body: string, prop: string): string {
  return new RegExp(`${prop}:\\s*([^;]+);`).exec(body)?.[1]?.trim() ?? '';
}

describe('fixed control and artwork sizes are tokens (#960)', () => {
  it('declares the size tokens, with control heights at 32px or more', () => {
    expect(tokenPx('--ctl-h', appCss)).toBe(36);
    expect(tokenPx('--ctl-h-sm', appCss)).toBe(32);
    expect(tokenPx('--art-lg', appCss)).toBe(88);
    expect(tokenPx('--badge-fs', appCss)).toBe(11);
    expect(tokenPx('--spinner-size', appCss)).toBe(24);
    expect(tokenPx('--log-col-ts', appCss)).toBe(88);
    // Every `--ctl-h*` value anywhere in the stylesheet — comfortable and
    // every override — stays a usable hit target (issue step 4).
    for (const m of appCss.matchAll(/(--ctl-h[\w-]*):\s*(\d+(?:\.\d+)?)px;/g)) {
      expect(Number(m[2]), `${m[1]} must stay a >=32px hit target`).toBeGreaterThanOrEqual(32);
    }
  });

  it('shrinks only artwork, badge and spinner in compact density', () => {
    const compact = compactDensityBlock();
    expect(tokenPx('--art-lg', compact)).toBe(72);
    expect(tokenPx('--badge-fs', compact)).toBe(10);
    expect(tokenPx('--spinner-size', compact)).toBe(20);
    // Interactive sizes and layout columns never shrink: no override at all.
    expect(compact).not.toMatch(/--ctl-h\s*:/);
    expect(compact).not.toMatch(/--ctl-h-sm\s*:/);
    expect(compact).not.toMatch(/--log-col-ts\s*:/);
  });

  it('routes the listed selectors through those tokens instead of literals', () => {
    /** [file, exact selector, size properties #960 tokenises]. */
    const TOKENISED: Array<[string, string, string[]]> = [
      ['src/app.css', '.icon-btn', ['width', 'height']],
      ['src/lib/components/Dashboard.svelte', '.album-art', ['width', 'height']],
      ['src/lib/components/Dashboard.svelte', '.track-card', ['grid-template-columns']],
      ['src/lib/components/Dashboard.svelte', '.not-playing-icon', ['width', 'height']],
      ['src/lib/components/LogViewer.svelte', '.log-entry', ['grid-template-columns']],
      ['src/lib/components/LogViewer.svelte', '.level-badge', ['font-size']],
      ['src/lib/components/Settings.svelte', '.swatch', ['height']],
      ['src/lib/components/UpdatePrompt.svelte', '.dismiss-btn', ['width', 'height']],
      ['src/lib/components/Reconnect.svelte', '.info-icon', ['width', 'height']],
    ];
    const PX = /\d+(?:\.\d+)?px/;
    const offenders: string[] = [];
    for (const [file, selector, props] of TOKENISED) {
      for (const body of exactRules(applicableCss(file), selector)) {
        for (const prop of props) {
          const value = declaredProp(body, prop);
          if (value && PX.test(value)) offenders.push(`${file}: ${selector} { ${prop}: ${value}; }`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it('leaves Dashboard icon buttons on the shared token rule', () => {
    // The local `.icon-btn` base re-stated the global 36px box, so the pane
    // keeps only its detached-dot and primary variants and inherits the size
    // from the tokenised `app.css` rule (the 640px media floor stays literal).
    expect(exactRules(applicableCss('src/lib/components/Dashboard.svelte'), '.icon-btn')).toEqual([]);
  });
});

/**
 * #961 — one shared empty-state primitive, one shared spinner.
 *
 * `LogViewer.svelte` reserved 240px for its `.empty-state` while
 * `Diagnostics.svelte` carried its own 120px copy plus a 60px `.small`
 * variant, so the same "nothing yet" moment rendered at three different
 * block sizes — and Diagnostics' loading branch showed the collecting label
 * with no progress affordance at all, reading as stuck rather than pending.
 *
 * Fails before the fix: two `.empty-state` definitions exist under `src/`
 * (and none in `src/app.css`), and the loading branch renders no spinner.
 */
describe('shared empty-state and spinner primitives (#961)', () => {
  /** Files holding an exact-selector rule for `selector` (media stripped: the
   * reduced-motion `.spinner` override lives in `app.css` by design). */
  function definingFiles(selector: string): string[] {
    const files: string[] = [];
    for (const file of styleFiles()) {
      if (exactRules(applicableCss(file), selector).length > 0) files.push(file);
    }
    return files;
  }

  it('defines .spinner and .empty-state exactly once, in src/app.css', () => {
    expect(definingFiles('.spinner')).toEqual(['src/app.css']);
    expect(definingFiles('.empty-state')).toEqual(['src/app.css']);
  });

  it('keeps the shared size modifiers in src/app.css', () => {
    for (const modifier of ['.empty-state.small', '.empty-state p', '.empty-state .hint']) {
      expect(definingFiles(modifier), `${modifier} must be shared`).toContain('src/app.css');
    }
  });

  it('leaves panes only their documented placement overrides', () => {
    // The full `.empty-state` copies are gone; what stays local is the shape
    // the shared rule cannot carry — LogViewer's full-height fill of its
    // scroll area, Diagnostics' left-pinned load-error hint.
    const local: string[] = [];
    for (const file of styleFiles()) {
      if (file === TOKEN_FILE) continue;
      for (const rule of outsideMedia(applicableCss(file)).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        if (rule[1].includes('.empty-state')) local.push(`${file}: ${rule[1].trim()}`);
      }
    }
    expect(local.sort()).toEqual([
      'src/lib/components/Diagnostics.svelte: .empty-state .hint',
      'src/lib/components/LogViewer.svelte: .log-list .empty-state',
    ]);
  });
});
