import { expect, test, type Page } from '@playwright/test';

/**
 * #742 — what the skip link actually does to the tab order.
 *
 * The link's target used to sit on `.app-container` in `+page.svelte`, the
 * wrapper around the whole mounted view. Activating it therefore put focus
 * above the header, so the next Tab still reached the repeated chrome the link
 * exists to bypass — a bypass that looks like it worked and did not.
 *
 * The contract is a real keyboard sequence in a real layout engine: activate
 * the link, then press Tab once and land on a control inside the view body.
 * JSDOM has no tab order and no fragment-navigation focus, so this lives in
 * Playwright (AGENTS.md §5). It fails pre-fix, where the target is the wrapper
 * above the header and the next Tab lands on the theme button instead.
 */
async function openDashboard(page: Page): Promise<void> {
  await page.addInitScript(() => {
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
        if (command === 'get_sync_status') {
          return {
            is_syncing: true,
            current_track: {
              id: 'track-1',
              title: 'A Track',
              artist: 'An Artist',
              album: 'An Album',
              album_art_url: null,
              duration_ms: 200000,
              progress_ms: 1000,
              is_playing: true
            },
            spotify_connected: true,
            teams_connected: true
          };
        }
        if (command === 'get_presence_history') return [];
        if (command === 'load_manual_status_command') return null;
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
  });

  await page.goto('/');
  await expect(page.locator('.dashboard header')).toBeVisible();
  await expect(page.locator('.track-card')).toBeVisible();
}

interface FocusReport {
  id: string;
  tagName: string;
  inHeader: boolean;
  inMainContent: boolean;
  isBody: boolean;
}

async function activeElement(page: Page): Promise<FocusReport> {
  return page.evaluate(() => {
    const active = document.activeElement;
    return {
      id: active?.id ?? '',
      tagName: active?.tagName ?? '',
      inHeader: Boolean(active?.closest('header')),
      inMainContent: Boolean(active?.closest('#main-content')),
      isBody: active === document.body
    };
  });
}

test('activating the skip link lands the next Tab inside the view body', async ({ page }) => {
  await openDashboard(page);

  // #739 puts focus on the view's heading at load, so the skip link — the
  // first focusable in the document — is one Shift+Tab back from there.
  await expect
    .poll(async () => (await activeElement(page)).tagName)
    .toBe('H1');
  await page.keyboard.press('Shift+Tab');
  const onSkipLink = await page.evaluate(() =>
    Boolean(document.activeElement?.classList.contains('skip-link'))
  );
  expect(onSkipLink, 'the skip link is not the first focusable in the document').toBe(true);

  // Activating it is a fragment navigation, which is what moves focus.
  await page.keyboard.press('Enter');

  // Focus is on the target itself — a body region, not a header control.
  await expect.poll(async () => (await activeElement(page)).id).toBe('main-content');
  expect(await activeElement(page)).toMatchObject({ inHeader: false, isBody: false });

  // One more Tab reaches the first control of the body, not a header button.
  await page.keyboard.press('Tab');
  const next = await activeElement(page);
  expect(next.inHeader, 'the next Tab is still inside the header').toBe(false);
  expect(next.inMainContent, 'the next Tab did not reach the view body').toBe(true);
});

test('the document has exactly one skip-link target, below the heading bar', async ({ page }) => {
  await openDashboard(page);

  const report = await page.evaluate(() => {
    const targets = [...document.querySelectorAll('#main-content')];
    const target = targets[0];
    return {
      count: targets.length,
      href: document.querySelector('.skip-link')?.getAttribute('href') ?? '',
      tagName: target?.tagName ?? '',
      inHeader: Boolean(target?.closest('header')),
      previousTag: target?.previousElementSibling?.tagName ?? '',
      tabIndex: target instanceof HTMLElement ? target.tabIndex : -99
    };
  });

  expect(report.href).toBe('#main-content');
  expect(report.count).toBe(1);
  expect(report.tagName).toBe('MAIN');
  expect(report.inHeader).toBe(false);
  expect(report.previousTag).toBe('HEADER');
  expect(report.tabIndex).toBe(-1);
});