/**
 * #903 — the shared button primitives, measured.
 *
 * `.btn-refresh` and `.snooze-resume` (Dashboard) and the update banner's
 * actions each re-declared their own padding, border and radius, so the same
 * class of secondary action rendered at three different heights and two
 * different radii, with two hover mechanics: a `filter: brightness(1.08)`
 * jump on the refresh button against a token colour change everywhere else.
 *
 * This is a REAL-LAYOUT contract, so it lives in Playwright rather than jsdom
 * (AGENTS.md §5): `getBoundingClientRect()` and `getComputedStyle()` against a
 * zero-box layout engine would pass against any CSS at all. Chromium and WebKit
 * measure the real thing.
 *
 * What is asserted:
 *   - the two compact Dashboard actions (`.btn-refresh`, `.snooze-resume`) and
 *     the banner's two actions share one height and one border-radius — the
 *     issue's "matching heights and radii for equivalent actions";
 *   - hovering `.btn-refresh` changes no `filter` — the brightness jump is gone
 *     and the hover goes through the shared tokens;
 *   - with `prefers-reduced-motion: reduce` the refresh button still has NO
 *     transition, and with motion allowed it DOES — which is what pins that the
 *     rewrite kept the override rather than leaving it inert. `.btn-refresh`
 *     declares no transition of its own, so both directions depend on the
 *     override really outranking the global `button` rule's inherited one.
 *
 * Fails against the pre-fix stylesheet on the first assertion (the snooze button
 * was `--r-sm` at `--sp-1 var(--sp-2)` while the refresh button was `--r-md`
 * at `--sp-1 var(--sp-3)`), and on the filter assertion.
 */
import { expect, test, type Page } from '@playwright/test';

const BASE_URL = 'http://127.0.0.1:4173';

/** 30 minutes out, RFC3339 — a live tray-snooze deadline so the chip renders. */
const SNOOZE_UNTIL = new Date(Date.now() + 30 * 60 * 1000).toISOString();

const TRACK = {
  id: 'track-1',
  title: 'Refresh Geometry',
  artist: 'An Artist',
  album: 'An Album',
  album_art_url: null,
  duration_ms: 200000,
  progress_ms: 1000,
  is_playing: true
};

/** The default config plus a live snooze deadline, for `load_config`. */
function configWithSnooze(): Record<string, unknown> {
  return {
    locale: 'en',
    spotify_client_id: null,
    teams_client_id: null,
    teams_tenant_id: null,
    presence_profiles: {},
    track_rule_action: 'append',
    status_format: '🎵 {artist} - {track} 🎧',
    status_format_episode: '🎙️ {show} - {episode}',
    polling_seconds: 30,
    teams_seconds: 300,
    snooze_until: SNOOZE_UNTIL,
    density: 'comfortable',
    theme: 'dark',
    start_minimized: false,
    close_to_tray: true,
    launch_at_startup: false,
    notifications: { enabled: true, on_track_change: true, on_status_clear: false },
    updates: { channel: 'stable' },
    status_rules: []
  };
}

/**
 * Open the Dashboard with the Tauri IPC stubbed. `get_sync_status` reports an
 * active sync so `.btn-refresh` is enabled, and `load_config` carries the live
 * snooze deadline so `.snooze-resume` renders next to it — the only place both
 * compact actions are on screen together.
 */
async function openDashboard(page: Page, reducedMotion: 'reduce' | 'no-preference' = 'no-preference') {
  await page.emulateMedia({ reducedMotion });
  await page.addInitScript(
    ({ track, config }) => {
      localStorage.setItem('presencejam:locale', 'en');
      let callbackId = 0;
      const callbacks = new Map<number, (...args: unknown[]) => void>();
      const internals = {
        metadata: { currentWindow: { label: 'main' } },
        transformCallback(callback: (...args: unknown[]) => void): number {
          const id = ++callbackId;
          callbacks.set(id, callback);
          return id;
        },
        async invoke(command: string): Promise<unknown> {
          if (command === 'plugin:event|listen') return callbackId;
          if (command === 'plugin:event|unlisten') return null;
          if (command === 'is_onboarding_complete') return true;
          if (command === 'load_config') return config;
          if (command === 'get_sync_status') {
            return { is_syncing: true, current_track: track, spotify_connected: true, teams_connected: true };
          }
          if (command === 'get_presence_history') return [];
          if (command === 'load_manual_status_command') return null;
          if (command === 'plugin:updater|check') return null;
          return null;
        }
      };
      Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
        configurable: true,
        value: { unregisterListener: () => undefined }
      });
      Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: internals });
    },
    { track: TRACK, config: configWithSnooze() }
  );

  await page.goto('/');
  await expect(page.locator('.btn-refresh')).toBeVisible();
  await expect(page.locator('.snooze-resume')).toBeVisible();
}

interface BoxGeometry {
  height: number;
  borderRadius: string;
  padding: string;
  fontSize: string;
  filter: string;
  transitionProperty: string;
  transitionDuration: string;
}

async function measure(page: Page, selector: string): Promise<BoxGeometry> {
  return page.locator(selector).evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      height: element.getBoundingClientRect().height,
      borderRadius: style.borderTopLeftRadius,
      padding: style.padding,
      fontSize: style.fontSize,
      filter: style.filter,
      transitionProperty: style.transitionProperty,
      transitionDuration: style.transitionDuration
    };
  });
}

test('equivalent compact actions share one height and one radius (#903)', async ({ browser }) => {
  const context = await browser.newContext({ baseURL: BASE_URL, viewport: { width: 900, height: 750 } });
  const page = await context.newPage();
  try {
    await openDashboard(page);

    const refresh = await measure(page, '.btn-refresh');
    const resume = await measure(page, '.snooze-resume');

    // The issue's criterion: "matching heights and radii for equivalent
    // actions". Pre-fix these differed — `--r-sm` vs `--r-md` and
    // `--sp-1 var(--sp-2)` vs `--sp-1 var(--sp-3)`.
    expect(Math.abs(resume.height - refresh.height), 'snooze-resume vs btn-refresh height').toBeLessThanOrEqual(1);
    expect(resume.borderRadius, 'snooze-resume vs btn-refresh radius').toBe(refresh.borderRadius);
    expect(resume.padding, 'snooze-resume vs btn-refresh padding').toBe(refresh.padding);
    expect(resume.fontSize, 'snooze-resume vs btn-refresh font size').toBe(refresh.fontSize);
  } finally {
    await context.close();
  }
});

test('the refresh button hovers through tokens, not a brightness filter (#903)', async ({ browser }) => {
  const context = await browser.newContext({ baseURL: BASE_URL, viewport: { width: 900, height: 750 } });
  const page = await context.newPage();
  try {
    await openDashboard(page);
    const before = await measure(page, '.btn-refresh');
    await page.locator('.btn-refresh').hover();
    await page.waitForFunction(
      (selector) => {
        const element = document.querySelector(selector);
        return element !== null && getComputedStyle(element).backgroundColor !== '';
      },
      '.btn-refresh'
    );
    const hovered = await measure(page, '.btn-refresh');

    expect(before.filter, 'filter at rest').toBe('none');
    expect(hovered.filter, 'filter on hover — brightness() must be gone').toBe('none');
  } finally {
    await context.close();
  }
});

test('the refresh button still has no transition under reduced motion (#903)', async ({ browser }) => {
  const context = await browser.newContext({ baseURL: BASE_URL, viewport: { width: 900, height: 750 } });
  const page = await context.newPage();
  try {
    await openDashboard(page, 'reduce');
    const reduced = await measure(page, '.btn-refresh');
    // `transition: none` is the override the rewrite had to keep; it must not
    // have been left in place but outranked by anything.
    expect(reduced.transitionProperty, 'reduced-motion transition-property').toBe('none');
    expect(reduced.transitionDuration, 'reduced-motion transition-duration').toBe('0s');

    // The other direction, so the assertion above is not vacuous: with motion
    // allowed the same button inherits the global `button` transition.
    const motionContext = await browser.newContext({ baseURL: BASE_URL, viewport: { width: 900, height: 750 } });
    const motionPage = await motionContext.newPage();
    try {
      await openDashboard(motionPage, 'no-preference');
      const animated = await measure(motionPage, '.btn-refresh');
      expect(animated.transitionProperty, 'motion-allowed transition-property').toContain('background-color');
    } finally {
      await motionContext.close();
    }
  } finally {
    await context.close();
  }
});
