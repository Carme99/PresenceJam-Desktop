/**
 * #948 — the LogViewer and Diagnostics toolbars must not lay their actions
 * out past the pane edge at the shipped default window (600x750).
 *
 * This is a REAL-LAYOUT contract, so it lives here and not in jsdom: jsdom
 * has no box model, so `scrollWidth`, `clientWidth` and `getBoundingClientRect`
 * are all zero there and an assertion built on them would pass against
 * overflowing CSS. Chromium and WebKit measure the real thing.
 *
 * What is asserted:
 *   - `toolbar.scrollWidth <= toolbar.clientWidth` at 600px in en, de and fr
 *     (the issue's own criterion — the row must not overflow its own box);
 *   - every toolbar action button's right edge sits inside the pane's visible
 *     area, so Clear / Open folder / Copy snapshot are actually clickable
 *     without widening the window;
 *   - the page itself never needs horizontal scrolling to reach them;
 *   - and the same for the Diagnostics toolbar.
 *
 * The German and French labels are longer than the English ones
 * ('Ordner öffnen', 'Avertissement'), so the row overflowed in every locale.
 */
import { expect, test, type Page } from '@playwright/test';

const LOCALES = ['en', 'de', 'fr'] as const;
type Locale = (typeof LOCALES)[number];

// The shipped default window in src-tauri/tauri.conf.json, and its minimum.
const DEFAULT_WIDTH = 600;
const MIN_WIDTH = 400;

/**
 * The Dashboard's Diagnostics nav button label per locale — the
 * `dashboard.openDiagnosticsAria` i18n key. Used to navigate to the pane
 * through the real UI rather than through an internal store.
 */
const DIAGNOSTICS_ARIA_LABEL: Record<Locale, string> = {
  en: 'Open diagnostics',
  de: 'Diagnose öffnen',
  fr: 'Ouvrir les diagnostics'
};

/** One toolbar action's measured geometry, in viewport pixels. */
interface ToolbarAction {
  label: string;
  left: number;
  right: number;
  width: number;
  /**
   * True when the action sits inside a horizontal scroll container within the
   * toolbar (the six-tab filter strip). Such an action is reachable BY
   * SCROLLING that container rather than by sitting inside the pane's edge,
   * so it is held to the strip's contract, not the pane's — see
   * `expectToolbarFits`.
   */
  inScrollableStrip: boolean;
}

/** Everything the overflow assertions need, measured in the page. */
interface ToolbarMeasurement {
  scrollWidth: number;
  clientWidth: number;
  paneLeft: number;
  paneRight: number;
  documentScrollWidth: number;
  documentClientWidth: number;
  actions: ToolbarAction[];
}

/**
 * Boot the app in the browser with a mocked Tauri bridge, mirroring
 * tests/browser/logviewer.spec.ts: the static adapter serves the SPA and the
 * views need `__TAURI_INTERNALS__` to mount at all.
 */
async function openApp(page: Page, locale: Locale): Promise<void> {
  await page.addInitScript((selectedLocale: string) => {
    localStorage.setItem('presencejam:locale', selectedLocale);
    localStorage.setItem('presencejam:density', 'comfortable');

    let callbackId = 0;
    const callbacks = new Map<number, (...args: unknown[]) => void>();
    const snapshot = {
      app_version: '4.7.0',
      tauri_version: '2.0.0',
      os: { platform: 'linux', arch: 'x86_64', family: 'unix' },
      config: { locale: selectedLocale },
      tokens: {},
      keychain: {},
      sync_state: {},
      recent_logs: [],
      log_source_status: 'ok',
      failed_update_install: null
    };
    const internals = {
      metadata: { currentWindow: { label: 'main' } },
      transformCallback(callback: (...args: unknown[]) => void): number {
        const id = ++callbackId;
        callbacks.set(id, callback);
        return id;
      },
      async invoke(command: string): Promise<unknown> {
        if (command === 'is_onboarding_complete') return true;
        if (command === 'load_config') return { spotify: {}, locale: selectedLocale };
        if (command === 'get_recent_logs') return [];
        if (command === 'get_diagnostics_snapshot') return snapshot;
        if (command === 'get_sync_status') return null;
        if (command === 'is_spotify_client_secret_set') return 'Present';
        if (command === 'plugin:event|listen') return callbackId;
        if (command === 'plugin:event|unlisten') return null;
        return null;
      }
    };
    Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
      configurable: true,
      value: { unregisterListener: () => undefined }
    });
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: internals
    });
  }, locale);
}

/**
 * Measure a toolbar and everything clickable in it. The numbers are computed
 * inside the page in one pass so no value crosses the wire twice.
 */
async function measureToolbar(page: Page, selector: string): Promise<ToolbarMeasurement> {
  return page.locator(selector).evaluate((toolbar) => {
    const pane = toolbar.parentElement ?? toolbar;
    const paneRect = pane.getBoundingClientRect();
    return {
      scrollWidth: toolbar.scrollWidth,
      clientWidth: toolbar.clientWidth,
      paneLeft: paneRect.left,
      paneRight: paneRect.right,
      documentScrollWidth: document.documentElement.scrollWidth,
      documentClientWidth: document.documentElement.clientWidth,
      actions: [...toolbar.querySelectorAll('button')].map((button) => {
        const rect = button.getBoundingClientRect();
        // An action inside a horizontal scroll container is reachable BY
        // SCROLLING that container, not by sitting inside the pane edge.
        const scroller = button.closest<HTMLElement>('.seg');
        return {
          label: button.textContent?.trim() ?? '',
          left: rect.left,
          right: rect.right,
          width: rect.width,
          inScrollableStrip: scroller !== null && scroller !== toolbar
        };
      })
    };
  });
}

/** Assert the issue's own criterion plus action reachability. */
function expectToolbarFits(measurement: ToolbarMeasurement, context: string) {
  expect(
    measurement.scrollWidth,
    `${context}: toolbar.scrollWidth (${measurement.scrollWidth}) must fit clientWidth (${measurement.clientWidth})`
  ).toBeLessThanOrEqual(measurement.clientWidth);

  expect(
    measurement.actions.length,
    `${context}: toolbar must render its actions`
  ).toBeGreaterThan(0);
  // The destructive/primary actions — Clear, Open folder, Copy snapshot,
  // Pop out, Copy/Save diagnostics — must sit inside the pane's visible
  // area. A filter tab inside the horizontally scrollable `.seg` strip is
  // reachable by scrolling that strip instead, which is the contract #948
  // chose over a duplicate overflow menu.
  const paneActions = measurement.actions.filter((action) => !action.inScrollableStrip);
  expect(
    paneActions.length,
    `${context}: toolbar must render at least one action outside the filter strip`
  ).toBeGreaterThan(0);
  for (const action of measurement.actions) {
    expect(
      action.width,
      `${context}: "${action.label}" was collapsed to zero width`
    ).toBeGreaterThan(0);
  }
  for (const action of paneActions) {
    expect(
      action.right,
      `${context}: "${action.label}" ends past the pane's visible right edge`
    ).toBeLessThanOrEqual(measurement.paneRight + 0.5);
    expect(
      action.left,
      `${context}: "${action.label}" starts before the pane's visible left edge`
    ).toBeGreaterThanOrEqual(measurement.paneLeft - 0.5);
  }

  expect(
    measurement.documentScrollWidth,
    `${context}: the page must not need horizontal scrolling to reach the toolbar`
  ).toBeLessThanOrEqual(measurement.documentClientWidth);
}

for (const width of [DEFAULT_WIDTH, MIN_WIDTH]) {
  test(`LogViewer toolbar fits at ${width}px in every locale`, async ({ browser }) => {
    for (const locale of LOCALES) {
      const context = await browser.newContext({
        baseURL: 'http://127.0.0.1:4173',
        viewport: { width, height: 750 }
      });
      const page = await context.newPage();
      try {
        await openApp(page, locale);
        await page.goto('/detached/logs');
        await expect(page.locator('.toolbar')).toBeVisible();

        expectToolbarFits(
          await measureToolbar(page, '.toolbar'),
          `${locale}@${width} logs`
        );
      } finally {
        await context.close();
      }
    }
  });

  test(`Diagnostics toolbar fits at ${width}px in every locale`, async ({ browser }) => {
    for (const locale of LOCALES) {
      const context = await browser.newContext({
        baseURL: 'http://127.0.0.1:4173',
        viewport: { width, height: 750 }
      });
      const page = await context.newPage();
      try {
        // `is_onboarding_complete` resolves true, so boot lands on the
        // Dashboard; the Diagnostics pane is reachable from there.
        await openApp(page, locale);
        await page.goto('/');
        await page.getByRole('button', { name: DIAGNOSTICS_ARIA_LABEL[locale] }).click();
        const toolbar = page.locator('.diagnostics .toolbar');
        await expect(toolbar).toBeVisible();

        expectToolbarFits(
          await measureToolbar(page, '.diagnostics .toolbar'),
          `${locale}@${width} diagnostics`
        );
      } finally {
        await context.close();
      }
    }
  });
}