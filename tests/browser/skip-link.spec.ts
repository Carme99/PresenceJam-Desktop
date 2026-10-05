import { expect, test, type Page } from '@playwright/test';

/**
 * #739 — focus really does land on the view heading at load, and the skip
 * link's target is reachable by keyboard.
 *
 * #742 (moving that target down onto each view's body) is split into a
 * follow-up that lands after the Settings / LogViewer / Diagnostics slices own
 * a `#main-content` each. This spec pins the two halves of the CURRENT contract
 * — the link is the first focusable, and activating it moves focus rather than
 * leaving it on the wrapper — so the follow-up has a baseline to move from, and
 * so a regression that made the link inert is caught here in the meantime.
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

test('activating the skip link moves focus to its target and keeps it keyboard-reachable', async ({ page }) => {
  await openDashboard(page);

  // #739 puts focus on the view's heading at load, so the skip link — the
  // first focusable in the document — is one Shift+Tab back from there.
  await expect.poll(async () => (await activeElement(page)).tagName).toBe('H1');
  await page.keyboard.press('Shift+Tab');
  const onSkipLink = await page.evaluate(() =>
    Boolean(document.activeElement?.classList.contains('skip-link'))
  );
  expect(onSkipLink, 'the skip link is not the first focusable in the document').toBe(true);

  // Activating it is a fragment navigation, which is what moves focus.
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await activeElement(page)).id).toBe('main-content');
  expect(await activeElement(page)).toMatchObject({ inHeader: false, isBody: false });
});
