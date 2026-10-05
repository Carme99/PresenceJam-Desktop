import { expect, test, type Page } from '@playwright/test';

/**
 * #743 — the skip link the shared layout renders in every webview must work
 * in a popped-out pane too.
 *
 * Before the fix, `/detached/<pane>` mounted `LogViewer` / `Settings` with no
 * `#main-content` anywhere in the document, so the link was the first tab
 * stop and activating it left focus exactly where it was — a bypass that
 * silently failed in the window the keyboard-heavy "watch the log beside
 * Teams" workflow actually uses.
 *
 * Fails pre-fix: `#main-content` does not exist in the pane at all, so the
 * fragment navigation has nothing to move focus to.
 */
async function openDetachedPane(page: Page, pane: 'logs' | 'settings'): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem('presencejam:locale', 'en');

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
        if (command === 'get_recent_logs') return [];
        if (command === 'plugin:event|listen') return callbackId;
        if (command === 'plugin:event|unlisten') return null;
        if (command === 'load_config') {
          return {
            polling_interval_secs: 30,
            teams_poll_interval_secs: 60,
            track_rule_action: 'append',
            presence_profiles: {},
            notifications: { track_change: false, sync_error: true },
            theme: 'system',
            density: 'comfortable',
            client_id: '',
            client_secret: '',
            profanity_words: [],
            profanity_placeholder: '****'
          };
        }
        if (command === 'get_sync_status') {
          return {
            is_syncing: false,
            current_track: null,
            spotify_connected: true,
            teams_connected: true
          };
        }
        if (command === 'get_spotify_granted_scopes') return [];
        if (command === 'get_teams_granted_scopes') return [];
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

  await page.goto(`/detached/${pane}`);
}

test('the Logs pane skip link moves focus to the pane body', async ({ page }) => {
  await openDetachedPane(page, 'logs');
  await expect(page.locator('.log-viewer')).toBeVisible();

  await page.locator('.skip-link').focus();
  await page.keyboard.press('Enter');

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const active = document.activeElement;
        return {
          id: active?.id ?? '',
          isBody: active === document.body,
          wrapsPaneBody: Boolean(active?.querySelector('.log-viewer'))
        };
      })
    )
    .toEqual({ id: 'main-content', isBody: false, wrapsPaneBody: true });
});

test('the Settings pane skip link moves focus to the pane body', async ({ page }) => {
  await openDetachedPane(page, 'settings');
  await expect(page.locator('.settings')).toBeVisible();

  await page.locator('.skip-link').focus();
  await page.keyboard.press('Enter');

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const active = document.activeElement;
        return {
          id: active?.id ?? '',
          isBody: active === document.body,
          wrapsPaneBody: Boolean(active?.querySelector('.settings'))
        };
      })
    )
    .toEqual({ id: 'main-content', isBody: false, wrapsPaneBody: true });
});
