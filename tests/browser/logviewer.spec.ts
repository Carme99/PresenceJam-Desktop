import { expect, test, type Page } from '@playwright/test';

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

test('focuses the log viewport and navigates it with PageUp/PageDown in Chromium', async ({ browser }) => {
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
    expect(geometry.scrollHeight).toBeGreaterThan(geometry.clientHeight);
    await page.keyboard.press('PageDown');
    await expect.poll(() => viewport.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
    const afterPageDown = await viewport.evaluate((element) => element.scrollTop);

    await page.keyboard.press('PageUp');
    await expect.poll(() => viewport.evaluate((element) => element.scrollTop)).toBeLessThan(afterPageDown);
  } finally {
    await context.close();
  }
});
