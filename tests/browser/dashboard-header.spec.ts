import { expect, test, type Page } from '@playwright/test';

/**
 * #954 — the Dashboard header at the sizes the window actually supports.
 *
 * `tauri.conf.json` declares `minWidth: 400`, and the header had no
 * small-window strategy: the title column set `min-width: 0` but the `h1` had
 * no ellipsis, the badges were `nowrap`, and `.header-right` was a fixed 256px
 * of icon buttons. At 400px the title and badge text painted across the icon
 * row and the header grew to several badge rows.
 *
 * This is browser geometry — `scrollWidth` versus `clientWidth` is meaningless
 * in JSDOM, which reports 0 for both — so it lives here rather than in Vitest
 * (AGENTS.md §5). It fails pre-fix: the header's scroll width exceeds its
 * client width at 400×500, the badges wrap to a second row, and the title
 * column reaches into the icon row.
 */
const LOCALES = ['en', 'de', 'fr'] as const;
const SIZES = [
  { label: '400x500 (declared minimum)', width: 400, height: 500 },
  { label: '600x750 (shipped default)', width: 600, height: 750 }
] as const;

const TRACK = {
  id: 'track-1',
  title: 'A Track With A Long Enough Title To Matter',
  artist: 'An Artist',
  album: 'An Album',
  album_art_url: null,
  duration_ms: 200000,
  progress_ms: 1000,
  is_playing: true
};

async function openDashboard(page: Page, locale: (typeof LOCALES)[number]): Promise<void> {
  await page.addInitScript(
    ({ selectedLocale, track }) => {
      localStorage.setItem('presencejam:locale', selectedLocale);

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
              current_track: track,
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
    },
    { selectedLocale: locale, track: TRACK }
  );

  await page.goto('/');
  await expect(page.locator('.dashboard header')).toBeVisible();
  await expect(page.locator('.track-card')).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('lang', locale);
}

interface HeaderGeometry {
  scrollWidth: number;
  clientWidth: number;
  titleRight: number;
  badgesRight: number;
  iconRowLeft: number;
  badgeRows: number;
  inlineNavVisible: boolean;
  overflowButtonVisible: boolean;
  titleClipped: boolean;
  syncLabel: string;
  syncLabelClipped: boolean;
}

async function measureHeader(page: Page): Promise<HeaderGeometry> {
  return page.locator('.dashboard header').evaluate((header) => {
    const title = header.querySelector<HTMLElement>('.title h1');
    const badges = [...header.querySelectorAll<HTMLElement>('.badges .badge')];
    const iconRow = header.querySelector<HTMLElement>('.header-right');
    if (!title || !iconRow) throw new Error('the Dashboard header is missing its title or actions');
    const iconRowLeft = iconRow.getBoundingClientRect().left;
    const badgeTops = new Set(badges.map((badge) => Math.round(badge.getBoundingClientRect().top)));
    return {
      scrollWidth: header.scrollWidth,
      clientWidth: header.clientWidth,
      titleRight: title.getBoundingClientRect().right,
      badgesRight: badges.length
        ? Math.max(...badges.map((badge) => badge.getBoundingClientRect().right))
        : 0,
      iconRowLeft,
      badgeRows: badgeTops.size,
      inlineNavVisible: (header.querySelector<HTMLElement>('.header-extra')?.offsetParent ?? null)
        !== null,
      overflowButtonVisible: (header.querySelector<HTMLElement>('.more-btn')?.offsetParent ?? null)
        !== null,
      titleClipped: title.scrollWidth > title.clientWidth,
      syncLabel: header.querySelector<HTMLElement>('.badge.accent .badge-label')?.textContent?.trim() ?? '',
      syncLabelClipped:
        (header.querySelector<HTMLElement>('.badge.accent .badge-label')?.scrollWidth ?? 0) >
        (header.querySelector<HTMLElement>('.badge.accent .badge-label')?.clientWidth ?? 0)
    };
  });
}

for (const size of SIZES) {
  test(`keeps the Dashboard header on one row at ${size.label}`, async ({ browser }) => {
    for (const locale of LOCALES) {
      const context = await browser.newContext({
        baseURL: 'http://127.0.0.1:4173',
        viewport: { width: size.width, height: size.height }
      });
      const page = await context.newPage();
      try {
        await openDashboard(page, locale);
        const header = await measureHeader(page);
        const context_ = `${locale} @ ${size.label}`;

        // The row itself must not overflow.
        expect(header.scrollWidth, `${context_}: header content overflows its box`).toBeLessThanOrEqual(
          header.clientWidth
        );
        // The badges stay on one row — extra badge rows push the playback card
        // down, which is the other half of the defect.
        expect(header.badgeRows, `${context_}: badges wrapped onto ${header.badgeRows} rows`).toBe(1);
        // Neither the title nor the badges may reach into the icon row.
        expect(header.titleRight, `${context_}: title overlaps the icon row`).toBeLessThanOrEqual(
          header.iconRowLeft
        );
        expect(header.badgesRight, `${context_}: badges overlap the icon row`).toBeLessThanOrEqual(
          header.iconRowLeft
        );
        // P7: the compact badge uses the short localized label rather than an
        // ellipsised fragment of the full one — in every locale.
        expect(header.syncLabelClipped, `${context_}: the sync badge is clipped`).toBe(false);
        expect(header.syncLabel, `${context_}: the sync badge lost its label`).toBeTruthy();
      } finally {
        await context.close();
      }
    }
  });
}

test('collapses the five nav buttons into one overflow menu only when compact', async ({ browser }) => {
  const compact = await browser.newContext({
    baseURL: 'http://127.0.0.1:4173',
    viewport: { width: 400, height: 500 }
  });
  // Above the ~640px cut-over the inline row is the layout that has to work.
  const wide = await browser.newContext({
    baseURL: 'http://127.0.0.1:4173',
    viewport: { width: 900, height: 750 }
  });
  try {
    const compactPage = await compact.newPage();
    await openDashboard(compactPage, 'de');
    const compactHeader = await measureHeader(compactPage);
    expect(compactHeader.inlineNavVisible, '400px still shows all five nav buttons').toBe(false);
    expect(compactHeader.overflowButtonVisible, '400px has no overflow menu').toBe(true);

    // The menu holds the same five actions, and its entries are reachable.
    await compactPage.locator('.more-btn').click();
    await expect(compactPage.locator('.header-menu [role="menuitem"]')).toHaveCount(5);

    // P5: the menu must be openable AND navigable by keyboard. A toggle that
    // only reveals the items leaves every one of them skipped by Tab — the
    // menu would work for pointer users only, and it does so at the shipped
    // 600px default, not just at the 400px minimum.
    // The menu is open from the click above; Escape returns focus to the toggle.
    await compactPage.keyboard.press('Escape');
    await expect(compactPage.locator('.more-btn')).toBeFocused();
    await compactPage.keyboard.press('Enter');
    await expect(compactPage.locator('.header-menu [role="menuitem"]').first()).toBeFocused();
    await compactPage.keyboard.press('ArrowDown');
    await expect(compactPage.locator('.header-menu [role="menuitem"]').nth(1)).toBeFocused();
    await compactPage.keyboard.press('End');
    await expect(compactPage.locator('.header-menu [role="menuitem"]').last()).toBeFocused();
    await compactPage.keyboard.press('Escape');
    await expect(compactPage.locator('.header-menu')).toHaveCount(0);
    await expect(compactPage.locator('.more-btn')).toBeFocused();
    await compactPage.keyboard.press('Enter');

    const widePage = await wide.newPage();
    await openDashboard(widePage, 'de');
    const wideHeader = await measureHeader(widePage);
    expect(wideHeader.inlineNavVisible, '900px lost the inline nav buttons').toBe(true);
    expect(wideHeader.overflowButtonVisible, '900px gained a redundant overflow menu').toBe(false);
  } finally {
    await compact.close();
    await wide.close();
  }
});