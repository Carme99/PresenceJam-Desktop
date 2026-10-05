import { expect, test, type Locator, type Page } from '@playwright/test';

const LOG_LINES = [
  '[2026-01-01][12:00:00][browser][TRACE] trace message',
  '[2026-01-01][12:00:01][browser][DEBUG] debug message',
  '[2026-01-01][12:00:02][browser][INFO] info message',
  '[2026-01-01][12:00:03][browser][WARN] warning message',
  '[2026-01-01][12:00:04][browser][ERROR] error message'
] as const;

const SCROLL_LINES = Array.from(
  { length: 80 },
  (_, index) => `[2026-01-01][12:${String(index).padStart(2, '0')}:00][browser][INFO] line ${index}`
);

const LOCALES = ['en', 'de', 'fr'] as const;
const DENSITIES = ['comfortable', 'compact'] as const;
/**
 * Reads `scrollTop` after the browser's smooth-scroll animation has
 * settled. The browser performs animated keyboard scrolling on a focusable
 * scroll container, so a naive `expect.poll(...).toBeGreaterThan(0)` returns
 * as soon as the first frame of the animation moves the value off zero —
 * that is a mid-animation sample, not the final position. If the next
 * keyboard event fires while the previous animation is still running, the
 * two animations overlap and the observed position settles far from where
 * either keypress alone would have placed it. The fix is to sample only
 * after the scroll has settled.
 *
 * Contract: the caller MUST guarantee that an animation is in progress
 * before invoking this helper — otherwise three equal reads can mean "the
 * animation finished" OR "we sampled before anything started", and the
 * helper cannot tell those apart. A pre-animation zero reads exactly the
 * same as a settled zero. In this spec the contract is enforced upstream
 * by `expect.poll(...).toBeGreaterThan(0)` (and symmetrically `.toBeLessThan`
 * for PageUp) — phase 1 of a two-phase split. After phase 1 returns, an
 * animation is demonstrably running, and three equal reads can only mean
 * it finished. The helper itself does not re-check that contract; a
 * caller that skips phase 1 invites the pre-animation false-settle bug.
 *
 * Polls `scrollTop` until three consecutive reads separated by a short
 * wait are equal (the animation has stopped). Returns the settled value,
 * or throws if the cap is reached without stability. Local to this spec:
 * one caller, deliberately not promoted to a shared utility.
 *
 * Edge case: an animation shorter than STABLE_INTERVAL_MS settles in a
 * single step, so the first two reads are equal and the helper returns
 * that value — which is correct, because a single-step animation has no
 * in-flight states to mistake for a settled one. Chromium and WebKit
 * keyboard scrolls run 200-400ms, so this path is theoretical here.
 */
async function settledScrollTop(
  viewport: Locator,
  label: string,
  timeoutMs = 2000
): Promise<number> {
  // 150ms of apparent stability is not proof of settling: a smooth scroll can
  // pause mid-animation long enough for three equal reads and then continue. On a
  // loaded webkit runner that produced `afterPageDown = 19` followed by the same
  // PageUp leaving scrollTop at 439 — the "settled" baseline was simply read
  // before the animation finished. Widening the window to 300ms and requiring 4
  // consecutive equal reads makes a mid-animation pause far less likely to pass
  // as settled, while still returning well inside the 2s budget.
  const STABLE_INTERVAL_MS = 75;
  const REQUIRED_STABLE_READS = 4;
  const deadline = Date.now() + timeoutMs;
  let previous = await viewport.evaluate((element) => element.scrollTop);
  let stableReads = 0;
  while (Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, STABLE_INTERVAL_MS));
    const current = await viewport.evaluate((element) => element.scrollTop);
    if (current === previous) {
      stableReads += 1;
      if (stableReads >= REQUIRED_STABLE_READS) return current;
    } else {
      stableReads = 0;
      previous = current;
    }
  }
  throw new Error(
    `${label}: scrollTop did not settle within ${timeoutMs}ms (last observed value: ${previous})`
  );
}

const LABELS_BY_LOCALE: Record<(typeof LOCALES)[number], readonly string[]> = {
  en: ['Trace', 'Debug', 'Info', 'Warning', 'Error'],
  de: ['Trace', 'Debug', 'Info', 'Warnung', 'Fehler'],
  fr: ['Trace', 'Debug', 'Info', 'Avertissement', 'Erreur']
};


async function openLogViewer(
  page: Page,
  locale: (typeof LOCALES)[number],
  density: (typeof DENSITIES)[number],
  lines: readonly string[] = LOG_LINES
): Promise<void> {
  await page.addInitScript(
    ({ locale: selectedLocale, density: selectedDensity, lines }) => {
      localStorage.setItem('presencejam:locale', selectedLocale);
      localStorage.setItem('presencejam:density', selectedDensity);

      let callbackId = 0;
      const callbacks = new Map<number, (...args: unknown[]) => void>();
      const internals = {
        metadata: { currentWindow: { label: 'detached' } },
        transformCallback(callback: (...args: unknown[]) => void): number {
          const id = ++callbackId;
          callbacks.set(id, callback);
          return id;
        },
        async invoke(command: string): Promise<unknown> {
          if (command === 'get_recent_logs') return lines;
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
    },
    { locale, density, lines }
  );

  await page.goto('/detached/logs');
  await expect(page.locator('.log-entry')).toHaveCount(lines.length);
  await expect(page.locator('html')).toHaveAttribute('data-density', density);
  await expect(page.locator('html')).toHaveAttribute('lang', locale);
}

test('keeps every localized level badge clear of its message in Chromium', async ({ browser }) => {
  for (const density of DENSITIES) {
    for (const locale of LOCALES) {
      const context = await browser.newContext({ baseURL: 'http://127.0.0.1:4173' });
      const page = await context.newPage();
      try {
        await openLogViewer(page, locale, density);

        const measurements = await page.locator('.log-entry').evaluateAll((rows) =>
          rows.map((row) => {
            const badge = row.querySelector<HTMLElement>('.level-badge');
            const message = row.querySelector<HTMLElement>('.message');
            if (!badge || !message) throw new Error('LogViewer row is missing a badge or message');

            const rowRect = row.getBoundingClientRect();
            const badgeRect = badge.getBoundingClientRect();
            const messageRect = message.getBoundingClientRect();
            const badgeStyle = getComputedStyle(badge);

            return {
              level: badge.textContent?.trim() ?? '',
              rowLeft: rowRect.left,
              rowRight: rowRect.right,
              badgeLeft: badgeRect.left,
              badgeRight: badgeRect.right,
              messageLeft: messageRect.left,
              badgeWidth: badgeRect.width,
              badgeClientWidth: badge.clientWidth,
              badgeScrollWidth: badge.scrollWidth,
              marginLeft: Number.parseFloat(badgeStyle.marginLeft),
              marginRight: Number.parseFloat(badgeStyle.marginRight),
              position: badgeStyle.position,
              transform: badgeStyle.transform,
              left: badgeStyle.left,
              top: badgeStyle.top
            };
          })
        );

        expect(measurements).toHaveLength(5);
        expect(measurements.map(({ level }) => level)).toEqual(LABELS_BY_LOCALE[locale]);
        for (const measurement of measurements) {
          const context = `${locale}/${density} ${measurement.level}`;
          expect(measurement.badgeWidth, context).toBeGreaterThan(0);
          expect(measurement.badgeScrollWidth, `${context} clips its label`).toBeLessThanOrEqual(
            measurement.badgeClientWidth
          );
          expect(measurement.marginLeft, `${context} has a negative left margin`).toBeGreaterThanOrEqual(0);
          expect(measurement.marginRight, `${context} has a negative right margin`).toBeGreaterThanOrEqual(0);
          expect(measurement.position, `${context} must not use positioned offsets`).toBe('static');
          expect(measurement.transform, `${context} must not use a transform offset`).toBe('none');
          expect(measurement.left, `${context} must not use a left offset`).toBe('auto');
          expect(measurement.top, `${context} must not use a top offset`).toBe('auto');
          expect(measurement.badgeLeft, `${context} starts before its row`).toBeGreaterThanOrEqual(measurement.rowLeft);
          expect(measurement.badgeRight, `${context} ends after its row`).toBeLessThanOrEqual(measurement.rowRight);
          expect(
            measurement.badgeRight,
            `${context} badge overlaps its message`
          ).toBeLessThanOrEqual(measurement.messageLeft);
        }
      } finally {
        await context.close();
      }
    }
  }
});

test('focuses the log viewport and navigates it with PageUp/PageDown in Chromium and WebKit', async ({ browser }) => {
  const context = await browser.newContext({ baseURL: 'http://127.0.0.1:4173' });
  const page = await context.newPage();
  try {
    await openLogViewer(page, 'en', 'comfortable', SCROLL_LINES);
    const viewport = page.locator('.log-list');

    await viewport.focus();
    await expect(viewport).toBeFocused();
    await expect(viewport).toHaveAttribute('role', 'region');
    await expect(viewport).toHaveAttribute('aria-label', 'Logs');
    await expect(viewport).toHaveAttribute('tabindex', '0');

    await viewport.evaluate((element) => {
      element.scrollTop = 0;
    });
    const geometry = await viewport.evaluate((element) => ({
      scrollHeight: element.scrollHeight,
      clientHeight: element.clientHeight
    }));
    const maxScrollTop = geometry.scrollHeight - geometry.clientHeight;
    expect(geometry.scrollHeight).toBeGreaterThan(geometry.clientHeight);
    // Guards against a CSS/layout regression shrinking the scroll range to
    // nothing, which would make the PageUp/PageDown bounds below vacuous.
    expect(maxScrollTop, 'viewport must be scrollable').toBeGreaterThan(0);

    await page.keyboard.press('PageDown');
    // Phase 1 — retrying, so a pre-animation zero cannot fool it.
    // `expect.poll` retries on failure with its own backoff until the timeout
    // (5s in this project). After this returns, scrollTop is demonstrably > 0,
    // which means an animation is running.
    await expect
      .poll(() => viewport.evaluate((element) => element.scrollTop))
      .toBeGreaterThan(0);
    // Phase 2 — the scroll is running, so three equal reads can only mean it
    // settled, not that the keypress never registered. The previous fix
    // conflated these two cases (it returned 0 on a cold runner whose
    // keypress→animation-start latency exceeded the 150ms stability window).
    let afterPageDown = await settledScrollTop(viewport, 'after PageDown');
    expect(
      afterPageDown,
      'PageDown must move past the start of the scroll range'
    ).toBeGreaterThan(0);
    expect(
      afterPageDown,
      'PageDown must not scroll past the end of the scroll range'
    ).toBeLessThan(maxScrollTop);

    await page.keyboard.press('PageUp');
    // Phase 1 — symmetric retry: prove the keypress took effect before
    // trusting the helper. `afterPageDown` is the settled baseline, so a
    // value strictly below it means PageUp has moved the viewport.
    // If the PageDown baseline was read mid-animation, PageUp's effect is masked
    // by the still-running scroll and this times out. Retry the pair once from a
    // freshly settled PageDown rather than failing the run on that race.
    let afterPageUp = afterPageDown;
    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        await expect
          .poll(() => viewport.evaluate((element) => element.scrollTop))
          .toBeLessThan(afterPageDown);
        afterPageUp = await settledScrollTop(viewport, 'after PageUp');
        break;
      } catch (err) {
        if (attempt === 1) throw err;
        // Re-settle the baseline: press PageDown again and wait it out properly.
        await page.keyboard.press('PageDown');
        await settledScrollTop(viewport, 'after PageDown (retry)');
        await page.keyboard.press('PageUp');
        afterPageDown = await settledScrollTop(viewport, 'after PageDown (retry)');
      }
    }
    expect(
      afterPageUp,
      'PageUp must land above the settled PageDown position'
    ).toBeLessThan(afterPageDown);
  } finally {
    await context.close();
  }
});
