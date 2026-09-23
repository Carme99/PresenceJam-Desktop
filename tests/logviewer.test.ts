/**
 * #492 — LogViewer behavior, exercised through the real component.
 *
 * The slice does not change LogViewer logic (it already carries the
 * #399 tail window, #400 stickiness, #401 Trace fixes); these tests pin
 * the behaviors #492 lists so a regression — unkeyed full re-render,
 * forced scroll-to-bottom, dropped Trace tab — fails CI. Fail pre-fix
 * (if any of the three behaviors regress), pass post-fix.
 *
 * #692 adds the listener-teardown cases at the bottom: the pane must
 * release a `log://log` registration that settles *after* it unmounts.
 */
import '../src/app.css';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const listeners: Array<(e: { payload: { level: number; message: string } }) => void> = [];

// #692 regression harness. `listen()` resolves asynchronously and the pane
// can unmount inside that window, so the mock can hold every registration in
// flight (`holdRegistrations`) until the test releases it — the leak only
// shows up in exactly that ordering. `registrationsCreated` /
// `unlistenCalls` are the two halves of "was this subscription released?".
let holdRegistrations = false;
let registrationsCreated = 0;
let unlistenCalls = 0;
let heldResolvers: Array<() => void> = [];

vi.mock('@tauri-apps/api/event', () => ({
  // Mirror the real signature: listen(eventName, handler).
  listen: vi.fn((_event: string, fn: (e: unknown) => void) => {
    listeners.push(fn as never);
    registrationsCreated++;
    if (!holdRegistrations) {
      return Promise.resolve(() => {
        unlistenCalls++;
      });
    }
    return new Promise<() => void>((resolve) => {
      heldResolvers.push(() =>
        resolve(() => {
          unlistenCalls++;
        })
      );
    });
  })
}));

/** Settle every registration the harness is holding. */
async function releaseHeld(): Promise<void> {
  const held = heldResolvers.splice(0);
  for (const resolve of held) resolve();
  await Promise.resolve();
  await Promise.resolve();
}

vi.mock('$lib/stores/detach', () => ({
  popOut: vi.fn().mockResolvedValue(undefined),
  popIn: vi.fn().mockResolvedValue(undefined)
}));
// Static import: component path is author-time known (vi.mock calls hoist
// above it, so Tauri mocks still apply at load time).
import LogViewer from '$lib/components/LogViewer.svelte';
import { i18n } from '$lib/i18n';
import type { Mock } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

function emit(level: number, message: string) {
  for (const fn of listeners) fn({ payload: { level, message } });
}

beforeEach(() => {
  listeners.length = 0;
  holdRegistrations = false;
  registrationsCreated = 0;
  unlistenCalls = 0;
  heldResolvers = [];
});

afterEach(() => {
  // Unmount each render: the jsdom document is shared per file, so
  // getByRole would otherwise match tabs from earlier tests.
  cleanup();
  i18n.set('en');
});

describe('LogViewer behavior (#492)', () => {
  it('renders a capped keyed tail with showing-X-of-Y on a 500 burst', async () => {
    const { container } = render(LogViewer, { detached: false });
    for (let i = 0; i < 500; i++) emit(3, `msg-${i}`);
    await Promise.resolve();
    const rows = container.querySelectorAll('.log-entry');
    expect(rows.length).toBeLessThanOrEqual(100);
    expect(rows.length).toBeGreaterThan(0);
    expect(container.querySelector('.count')?.textContent).toMatch(/Showing \d+ of 500/);
    // Tail window: newest message visible, oldest burst entry evicted.
    expect(container.textContent).toContain('msg-499');
    expect(container.textContent).not.toContain('msg-0');
  });

  it('Trace filter button isolates level-1 logs (#734)', async () => {
    const { container, getByRole } = render(LogViewer, { detached: false });
    emit(1, 'trace-one');
    emit(3, 'info-one');
    await Promise.resolve();
    await fireEvent.click(getByRole('button', { name: 'Trace' }));
    expect(container.textContent).toContain('trace-one');
    expect(container.textContent).not.toContain('info-one');
  });

  it('offers Jump to latest only when scrolled up', async () => {
    const { container } = render(LogViewer, { detached: false });
    emit(3, 'hello');
    await Promise.resolve();
    const list = container.querySelector('.log-list') as HTMLElement;
    // At bottom: no jump button.
    expect(container.querySelector('.jump-latest')).toBeNull();
    // Scroll up: button appears.
    Object.defineProperty(list, 'scrollHeight', { value: 1000, configurable: true });
    Object.defineProperty(list, 'clientHeight', { value: 200, configurable: true });
    list.scrollTop = 100;
    await fireEvent.scroll(list);
    expect(container.querySelector('.jump-latest')).not.toBeNull();
  });

  it('labels the count through CLDR plural rules, not `count === 1` (#616)', async () => {
    const { container, getByRole } = render(LogViewer, { detached: false });
    const count = () => container.querySelector('.count')?.textContent?.trim();

    emit(3, 'first');
    await Promise.resolve();
    expect(count()).toBe('1 entry');

    emit(3, 'second');
    await Promise.resolve();
    expect(count()).toBe('2 entries');

    await fireEvent.click(getByRole('button', { name: 'Clear' }));
    await tick();
    expect(count()).toBe('0 entries');

    // CLDR fr puts 0 in the `one` category, and the label follows the locale
    // live — the hand-rolled `count === 1` rendered "0 entrées" here.
    i18n.set('fr');
    await tick();
    expect(count()).toBe('0 entrée');
    i18n.set('en');
  });
});

/**
 * #692 — the pane used to `await listen()` inside `onMount` and release the
 * result from an array in `onDestroy`. Unmounting while the registration was
 * in flight left the unlisten landing in an array nobody swept again, so
 * every visit leaked one subscription. These cases hold the registration
 * open across the unmount and assert it is still released.
 */
describe('LogViewer listener teardown (#692)', () => {
  it('releases a registration that settles after the pane unmounts', async () => {
    holdRegistrations = true;
    const { unmount } = render(LogViewer, { detached: false });
    await waitFor(() => expect(registrationsCreated).toBe(1));
    expect(unlistenCalls).toBe(0);

    unmount();
    // The subscription only appears now — the pane is already gone.
    await releaseHeld();

    expect(unlistenCalls).toBe(1);
  });

  it('leaves no live subscription behind after three visits', async () => {
    for (let visit = 0; visit < 3; visit++) {
      holdRegistrations = true;
      const { unmount } = render(LogViewer, { detached: false });
      await waitFor(() => expect(registrationsCreated).toBe(visit + 1));
      unmount();
      await releaseHeld();
    }

    expect(registrationsCreated).toBe(3);
    expect(unlistenCalls).toBe(3);
  });
});

/**
 * #949 — a fixed badge column let French "Avertissement" overflow into the
 * message column. This case resolves the winning grid declaration from the
 * mounted row, measures the rendered label with a deterministic jsdom seam,
 * and checks the message's resulting position. It therefore catches a later
 * CSS rule that recreates the overlap without pinning CSS prose.
 */
describe('LogViewer level badge column (#949)', () => {
  it('keeps localized badges clear of messages at every density', async () => {
    const labelsByLocale = {
      en: ['Trace', 'Debug', 'Info', 'Warning', 'Error'],
      de: ['Trace', 'Debug', 'Info', 'Warnung', 'Fehler'],
      fr: ['Trace', 'Debug', 'Info', 'Avertissement', 'Erreur']
    } as const;
    // jsdom does not load app.css layout rules; mirror only its density token
    // contract so the component's var() declarations resolve in both states.
    const densityCss = `
      :root { --fs-xs: 1em; --sp-3: 1em; }
      [data-density='compact'] { --fs-xs: 0.9em; --sp-3: 0.75em; }
    `;
    const appStyle = document.createElement('style');
    appStyle.dataset.test = 'logviewer-app-css';
    appStyle.textContent = densityCss;
    document.head.append(appStyle);

    const densityValues: Record<string, { fontSize: number; gap: number }> = {};
    try {
      for (const density of ['comfortable', 'compact'] as const) {
        document.documentElement.dataset.density = density;
        densityValues[density] = {
          fontSize: parseCssLength('var(--fs-xs)', document.documentElement),
          gap: parseCssLength('var(--sp-3)', document.documentElement)
        };
        for (const locale of ['en', 'de', 'fr'] as const) {
          i18n.set(locale);
          const view = render(LogViewer, { detached: false });
          for (let level = 1; level <= 5; level++) emit(level, `${locale}-message-${level}`);
          await tick();

          const rows = Array.from(view.container.querySelectorAll<HTMLElement>('.log-entry'));
          expect(rows).toHaveLength(5);

          for (const [index, row] of rows.entries()) {
            const badge = row.querySelector<HTMLElement>('.level-badge');
            const message = row.querySelector<HTMLElement>('.message');
            expect(badge).not.toBeNull();
            expect(message).not.toBeNull();
            expect(badge?.textContent).toBe(labelsByLocale[locale][index]);
            expect(message?.textContent).toBe(`${locale}-message-${index + 1}`);

            const rowStyle = getComputedStyle(row);
            expect(rowStyle.gridTemplateColumns).not.toBe('');
            const tracks = splitGridTracks(rowStyle.gridTemplateColumns);
            expect(tracks).toHaveLength(3);
            const intrinsicWidth = measureBadge(badge as HTMLElement);
            const badgeTrack = resolveBadgeTrack(tracks[1], intrinsicWidth);
            const timestampWidth = parseCssLength(tracks[0], row);
            const gapValue = rowStyle.columnGap || matchingProperty(row, 'column-gap') || matchingProperty(row, 'gap');
            expect(gapValue).not.toBe('');
            const gap = parseCssLength(gapValue, row);
            const badgeLeft = timestampWidth + gap;
            const messageLeft = badgeLeft + badgeTrack + gap;
            patchRect(badge as HTMLElement, rect(badgeLeft, intrinsicWidth));
            patchRect(message as HTMLElement, rect(messageLeft, 1));

            expect(badgeTrack).toBeGreaterThanOrEqual(intrinsicWidth);
            expect((badge as HTMLElement).getBoundingClientRect().right).toBeLessThanOrEqual(
              (message as HTMLElement).getBoundingClientRect().left
            );
          }

          view.unmount();
          listeners.length = 0;
        }
      }
      expect(densityValues.compact.fontSize).toBeLessThan(densityValues.comfortable.fontSize);
      expect(densityValues.compact.gap).toBeLessThan(densityValues.comfortable.gap);
    } finally {
      appStyle.remove();
      delete document.documentElement.dataset.density;
    }
  });
});

function splitGridTracks(value: string): string[] {
  const tracks: string[] = [];
  let current = '';
  let depth = 0;
  for (const character of value.trim()) {
    if (character === '(') depth++;
    if (character === ')') depth--;
    if (/\s/.test(character) && depth === 0) {
      if (current) tracks.push(current);
      current = '';
    } else {
      current += character;
    }
  }
  if (current) tracks.push(current);
  return tracks;
}

function parseCssLength(value: string, owner: Element, fontSize = 16): number {
  const normalized = value.trim();
  if (normalized.endsWith('px')) return requireFinite(Number.parseFloat(normalized), value);
  if (normalized.endsWith('em')) {
    return requireFinite(Number.parseFloat(normalized) * fontSize, value);
  }
  const variable = normalized.match(/^var\((--[\w-]+)\)$/);
  if (variable) return parseCssLength(customProperty(variable[1]), owner, fontSize);
  return requireFinite(Number.parseFloat(normalized), value);
}

function matchingProperty(element: Element, property: string): string {
  let matched = '';
  for (const sheet of Array.from(document.styleSheets)) {
    for (const rule of Array.from(sheet.cssRules)) {
      const selector = (rule as CSSStyleRule).selectorText;
      if (selector && element.matches(selector)) {
        const value = (rule as CSSStyleRule).style?.getPropertyValue(property).trim();
        if (value) matched = value;
      }
    }
  }
  return matched;
}

function customProperty(token: string): string {
  const root = document.documentElement;
  const computed = getComputedStyle(root).getPropertyValue(token).trim();
  if (computed) return computed;
  let matched: string | undefined;
  for (const sheet of Array.from(document.styleSheets)) {
    for (const rule of Array.from(sheet.cssRules)) {
      const style = (rule as CSSStyleRule).style;
      const selector = (rule as CSSStyleRule).selectorText;
      const value = style?.getPropertyValue(token).trim();
      if (selector && value && root.matches(selector)) matched = value;
    }
  }
  if (!matched) throw new Error(`No mounted stylesheet value for ${token}`);
  return matched;
}

function requireFinite(value: number, source: string): number {
  if (!Number.isFinite(value) || value <= 0) throw new Error(`Expected a positive CSS length, got: ${source}`);
  return value;
}

function resolveBadgeTrack(track: string, intrinsicWidth: number): number {
  if (track === 'max-content' || /^minmax\(\s*max-content\s*,\s*max-content\s*\)$/.test(track)) {
    return intrinsicWidth;
  }
  return parseCssLength(track, document.documentElement);
}

function paddingSide(value: string, side: 'left' | 'right'): string {
  const parts = value.trim().split(/\s+/);
  expect(parts.length).toBeGreaterThan(0);
  if (parts.length === 1) return parts[0];
  if (side === 'left') return parts[1];
  return parts[3] ?? parts[1];
}

function measureBadge(badge: HTMLElement): number {
  const computed = getComputedStyle(badge);
  const fontSizeValue = computed.fontSize || matchingProperty(badge, 'font-size');
  const padding = matchingProperty(badge, 'padding');
  const paddingLeftValue = computed.paddingLeft || paddingSide(padding, 'left');
  const paddingRightValue = computed.paddingRight || paddingSide(padding, 'right');
  const letterSpacingValue = computed.letterSpacing || matchingProperty(badge, 'letter-spacing');
  expect(fontSizeValue).not.toBe('');
  expect(padding).not.toBe('');
  const fontSize = parseCssLength(fontSizeValue, badge);
  const horizontalPadding = parseCssLength(paddingLeftValue, badge, fontSize)
    + parseCssLength(paddingRightValue, badge, fontSize);
  const letterSpacing = letterSpacingValue === ''
    ? 0
    : parseCssLength(letterSpacingValue, badge, fontSize);
  const label = computed.textTransform === 'uppercase'
    ? (badge.textContent ?? '').toUpperCase()
    : (badge.textContent ?? '');
  const glyphWidth = [...label].reduce(
    (width, character) => width + (/\s/.test(character) ? 0.35 : 0.62),
    0
  ) * fontSize;
  return glyphWidth + horizontalPadding + label.length * letterSpacing;
}

function rect(left: number, width: number): DOMRect {
  return {
    x: left,
    y: 0,
    left,
    top: 0,
    right: left + width,
    bottom: 20,
    width,
    height: 20,
    toJSON: () => ({})
  } as DOMRect;
}

function patchRect(element: HTMLElement, value: DOMRect): void {
  Object.defineProperty(element, 'getBoundingClientRect', {
    configurable: true,
    value: () => value
  });
}

/**
 * #969 — backfilled rows from different days are distinguishable in the log
 * view, and timestamps follow the configured app locale instead of the OS
 * default. The pane seeds its buffer from `get_recent_logs` (#595), so the
 * fix lands in `parseLogLine` and the live-row formatter.
 */
describe('LogViewer date + locale (#969)', () => {
  // Two on-disk lines from different days, far enough in the past that the
  // system timezone can never flip either of them onto "today" during a
  // test run. Same time-of-day on purpose — the criterion is that two rows
  // sharing only their HH:MM:SS must remain distinguishable.
  const TWO_DAYS_BACK = [
    '[2024-09-19][09:14:02][pj_lib::demo][INFO] from day one',
    '[2024-09-20][09:14:02][pj_lib::demo][INFO] from day two'
  ];

  it('renders a date span on backfilled rows whose day is not today', async () => {
    const invokeMock = invoke as unknown as Mock;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'get_recent_logs') return TWO_DAYS_BACK;
      return undefined;
    });

    const { container } = render(LogViewer, { detached: false });
    await waitFor(() => expect(container.querySelectorAll('.log-entry').length).toBe(2));

    const rows = Array.from(container.querySelectorAll('.log-entry'));
    const dateSpans = rows.map((row) => row.querySelector('.date'));
    // Both rows are from 2024 — never today — so both must carry their date.
    expect(dateSpans[0]).not.toBeNull();
    expect(dateSpans[1]).not.toBeNull();
    // Two distinct dates produce two distinct renderings (locale formatting
    // may reorder fields, but the day/key is unique per row).
    expect(dateSpans[0]?.textContent).not.toBe(dateSpans[1]?.textContent);
    // The shared HH:MM:SS alone would have been indistinguishable; with the
    // date span the two rows now read differently.
    expect(rows[0].textContent).not.toBe(rows[1].textContent);
  });

  it('localizes the seeded row timestamp to the active locale', async () => {
    const invokeMock = invoke as unknown as Mock;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_recent_logs') return TWO_DAYS_BACK;
      return undefined;
    });

    i18n.set('en');
    const en = render(LogViewer, { detached: false });
    await waitFor(() =>
      expect(en.container.querySelectorAll('.log-entry').length).toBe(2)
    );
    const enTimestamp = en.container.querySelector('.log-entry .timestamp')?.textContent ?? '';

    en.unmount();
    await tick();

    i18n.set('fr');
    const fr = render(LogViewer, { detached: false });
    await waitFor(() =>
      expect(fr.container.querySelectorAll('.log-entry').length).toBe(2)
    );
    const frTimestamp = fr.container.querySelector('.log-entry .timestamp')?.textContent ?? '';

    // The two locales render the same wall-clock time differently — `en`
    // appends "AM"/"PM" by default, `fr` uses 24-hour notation. Asserting
    // NOT identical keeps the test stable across ICU revisions where the
    // exact separator (`:`, `\u202F:`, …) may change.
    expect(frTimestamp).not.toBe('');
    expect(enTimestamp).not.toBe('');
    expect(frTimestamp).not.toBe(enTimestamp);

    // Restore the test-suite default so subsequent tests aren't pinned to fr.
    i18n.set('en');
    await tick();
  });
});

/**
 * #734 — the filter strip used to declare a tab widget (`role="tablist"` /
 * `role="tab"` / `aria-selected`) without a `tabpanel`, `aria-controls`, a
 * roving tabindex, or any arrow-key handling. It is six plain toggle buttons
 * with `aria-pressed` inside a labelled group: every filter is a separate
 * Tab stop and activates with the documented keys (Tab to reach, Space/Enter
 * to press — the native button keyboard model).
 */
describe('LogViewer filter strip semantics (#734)', () => {
  const FILTERS = ['All', 'Trace', 'Debug', 'Info', 'Warning', 'Error'];

  it('exposes six toggle buttons with aria-pressed, never a partial tab pattern', async () => {
    const { container } = render(LogViewer, { detached: false });
    await Promise.resolve();
    const group = container.querySelector('.seg');
    expect(group).not.toBeNull();
    // Exactly one pattern: no leftover tablist/tab/tabpanel roles.
    expect(group?.getAttribute('role')).not.toBe('tablist');
    expect(container.querySelectorAll('[role="tablist"]').length).toBe(0);
    expect(container.querySelectorAll('[role="tab"]').length).toBe(0);
    expect(container.querySelectorAll('[role="tabpanel"]').length).toBe(0);
    expect(container.querySelectorAll('[aria-selected]').length).toBe(0);
    expect(container.querySelectorAll('[aria-controls]').length).toBe(0);
    // The toggle pattern: six buttons, one pressed at a time.
    const buttons = Array.from(group?.querySelectorAll('button') ?? []);
    expect(buttons.map((b) => b.textContent?.trim())).toEqual(FILTERS);
    expect(buttons.length).toBe(6);
    for (const b of buttons) {
      expect(b.getAttribute('aria-pressed')).not.toBeNull();
    }
    const pressed = buttons.filter((b) => b.getAttribute('aria-pressed') === 'true');
    expect(pressed.length).toBe(1);
    expect(pressed[0].textContent?.trim()).toBe('All');
  });

  it('changes the filter from the keyboard (focus + Enter/Space activation)', async () => {
    const { container } = render(LogViewer, { detached: false });
    emit(1, 'trace-one');
    emit(3, 'info-one');
    await Promise.resolve();
    const buttons = Array.from(
      container.querySelectorAll('.seg button')
    ) as HTMLElement[];
    expect(buttons.length).toBe(6);
    // Every filter is keyboard-reachable: a plain Tab-stop button, no
    // roving-tabindex scheme to maintain.
    for (const b of buttons) {
      expect(b.tabIndex).toBeGreaterThanOrEqual(0);
    }
    // Native buttons turn Enter/Space into a click, so focus + click is the
    // keyboard path — no arrow-key handling to document.
    buttons[1].focus();
    expect(document.activeElement).toBe(buttons[1]);
    await fireEvent.click(document.activeElement as HTMLElement);
    await tick();
    expect(container.textContent).toContain('trace-one');
    expect(container.textContent).not.toContain('info-one');
    expect(buttons[1].getAttribute('aria-pressed')).toBe('true');
    // A second keyboard activation moves the pressed state along.
    buttons[5].focus();
    await fireEvent.click(document.activeElement as HTMLElement);
    await tick();
    expect(buttons[5].getAttribute('aria-pressed')).toBe('true');
    expect(buttons[1].getAttribute('aria-pressed')).toBe('false');
  });
});
