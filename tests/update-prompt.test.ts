/**
 * #590 (update-ux slice) — install-on-quit staging surface.
 *
 * The Rust half streams throttled `update-stage-progress` events while a
 * deferred stage downloads and exposes `cancel_deferred_update` to drop a
 * staged payload. This pins the frontend half:
 *   - the payload drives a live whole-percent position, and a payload with
 *     no `total` stays indeterminate instead of showing a guessed bar;
 *   - cancelling a staged update invokes the command and returns the banner
 *     to its plain offer;
 *   - a cancel issued while the download is still running cannot interrupt
 *     it, so the payload that lands afterwards is discarded rather than
 *     advertised as staged;
 *   - the progress subscription is released on unmount, including an unmount
 *     that races `listen()`'s promise (#287 teardown discipline).
 *
 * Fails pre-fix: the staged row listens to nothing (no percentage ever
 * renders), and the staged state offers no way back — the only exit was
 * applying the payload at the next quit.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { render, cleanup, fireEvent, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { get } from 'svelte/store';
import type { Mock } from 'vitest';

type Listener = { event: string; fn: (e: { payload: unknown }) => void };
const listeners: Listener[] = [];
/** Registrations released through their unlisten handle (leak probe). */
let released = 0;

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, fn: (e: unknown) => void) => {
    const entry = { event, fn: fn as Listener['fn'] };
    listeners.push(entry);
    return () => {
      const i = listeners.indexOf(entry);
      if (i >= 0) listeners.splice(i, 1);
      released += 1;
    };
  })
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(async () => '4.5.2') }));
/**
 * #982: the immediate download path drives the plugin's progress callback.
 * `downloadAndInstall` parks it so a test can emit the plugin's
 * `Started`/`Progress` events, and never settles — the component relaunches
 * once it resolves, which is not what these tests are about.
 */
type PluginProgress = {
  event: 'Started' | 'Progress';
  data: { contentLength?: number; chunkLength?: number };
};
let downloadCb: ((event: PluginProgress) => void) | null = null;

vi.mock('@tauri-apps/plugin-updater', () => ({
  check: vi.fn(async () => ({
    version: '4.6.0',
    downloadAndInstall: (cb: (event: PluginProgress) => void) => {
      downloadCb = cb;
      return new Promise<void>(() => {});
    }
  }))
}));

import { invoke } from '@tauri-apps/api/core';
import UpdatePrompt from '$lib/components/UpdatePrompt.svelte';
import { i18n, t } from '$lib/i18n';
import { configHydrated, configStore, defaultConfig } from '$lib/stores/config';
import type { AppConfig } from '$lib/types';

const invokeMock = invoke as unknown as Mock;

/** #678: the backend's candidate payload from `check_for_update`. */
const CANDIDATE = {
  version: '4.6.0',
  notes: 'Fixes and polish',
  pub_date: '2026-09-16T21:07:35Z'
};

/** Resolvers of the in-flight `stage_deferred_update` calls, in call order. */
let stageResolvers: ((outcome: unknown) => void)[] = [];

/** Deliver a Tauri event and let Svelte flush. */
async function emit(event: string, payload: unknown) {
  for (const l of [...listeners]) {
    if (l.event === event) l.fn({ payload });
  }
  await tick();
}

/** Mount the banner and wait until the update check has rendered it. */
async function mountBanner() {
  const rendered = render(UpdatePrompt);
  await waitFor(() =>
    expect(listeners.filter((l) => l.event === 'update-stage-progress')).toHaveLength(1)
  );
  await waitFor(() => expect(rendered.container.querySelector('.update-banner')).not.toBeNull());
  return rendered;
}

/**
 * Drive the deferred path up to the point where `stage_deferred_update` is
 * in flight: plain offer -> quit-time confirmation -> confirm.
 */
async function startStage(container: HTMLElement) {
  const confirm = () => within(container).getByRole('button', { name: t('update.installOnQuit') });
  await fireEvent.click(confirm());
  await waitFor(() => expect(container.querySelector('.update-confirm')).not.toBeNull());
  await fireEvent.click(confirm());
  await waitFor(() => expect(container.querySelector('.update-progress')).not.toBeNull());
}

/**
 * Drive the immediate download path up to a download in flight: the button is
 * gated on a resolved channel (#678), so this waits for the gate to open.
 */
async function startDownload(container: HTMLElement) {
  const button = () =>
    within(container).getByRole('button', { name: t('update.downloadAndInstall') });
  await waitFor(() => expect(button()).toBeTruthy());
  await fireEvent.click(button());
  await waitFor(() => expect(downloadCb).not.toBeNull());
}

/**
 * The banner component's own stylesheet. jsdom has no layout engine, so the
 * layout contract this slice implements (#950, #951) is only observable in
 * this harness as the rules the strip is styled by; the geometry itself was
 * hit-tested in a real browser (see the #951 commit).
 */
function bannerCss(): string {
  // Vitest runs with the repo root as cwd (tests/theme-density.test.ts).
  const source = readFileSync('src/lib/components/UpdatePrompt.svelte', 'utf8');
  return source.slice(source.indexOf('<style>'), source.indexOf('</style>'));
}

/**
 * Point the harness's `load_config` at a config persisted on `channel`,
 * leaving the store itself untouched — the banner must read the channel from
 * the backend, not from whoever happened to hydrate the store first.
 */
function persistChannel(channel: 'stable' | 'beta') {
  const base = invokeMock.getMockImplementation()!;
  const cfg: AppConfig = { ...structuredClone(defaultConfig), updates: { channel } };
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) =>
    cmd === 'load_config' ? cfg : base(cmd, args)
  );
}

beforeEach(() => {
  listeners.length = 0;
  released = 0;
  stageResolvers = [];
  downloadCb = null;
  i18n.set('en');
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'load_config') {
      // The mirror's own default: `updates.channel === 'stable'`.
      return Promise.resolve(structuredClone(defaultConfig));
    }
    if (cmd === 'check_for_update') {
      return Promise.resolve(CANDIDATE);
    }
    if (cmd === 'stage_deferred_update') {
      const outcome = Promise.withResolvers<unknown>();
      stageResolvers.push(outcome.resolve);
      return outcome.promise;
    }
    return Promise.resolve(undefined);
  });
  // #678: the banner hydrates the store itself when nothing has yet; start
  // every test from the un-hydrated, default-valued store so the hydration
  // path is what is under test, not a value a previous test left behind.
  configStore.set(structuredClone(defaultConfig));
  configHydrated.set(false);
});

afterEach(() => {
  cleanup();
});

describe('UpdatePrompt deferred staging (#590)', () => {
  it('renders the streamed percentage, and stays indeterminate without a total', async () => {
    const { container } = await mountBanner();
    await startStage(container);

    const row = () => container.querySelector('.update-progress')?.textContent?.trim();

    await emit('update-stage-progress', { downloaded: 45_000_000, total: 100_000_000 });
    expect(row()).toBe(t('update.stagingProgress', { percent: 45 }));

    await emit('update-stage-progress', { downloaded: 60_000_000, total: 100_000_000 });
    expect(row()).toBe(t('update.stagingProgress', { percent: 60 }));

    // No Content-Length: no percentage may be invented.
    await emit('update-stage-progress', { downloaded: 1_000_000, total: null });
    expect(row()).toBe(t('update.preparing'));
    expect(row()).not.toContain('%');
  });

  it('cancels a staged update through the backend and returns to the plain offer', async () => {
    const { container } = await mountBanner();
    await startStage(container);

    stageResolvers.shift()!({ staged: '4.6.0', current: '4.5.2' });
    await waitFor(() => expect(container.querySelector('.update-staged')).not.toBeNull());

    await fireEvent.click(
      within(container).getByRole('button', { name: t('update.cancelStage') })
    );

    await waitFor(() =>
      expect(invokeMock.mock.calls.map(([cmd]) => cmd)).toContain('cancel_deferred_update')
    );
    await waitFor(() => expect(container.querySelector('.update-staged')).toBeNull());
    expect(container.querySelector('.update-confirm')).toBeNull();
    expect(within(container).getByRole('button', { name: t('update.installOnQuit') })).toBeTruthy();
  });

  it('discards a payload that lands after the user cancelled the download', async () => {
    const { container } = await mountBanner();
    await startStage(container);

    await emit('update-stage-progress', { downloaded: 1_000_000, total: 100_000_000 });
    await fireEvent.click(
      within(container).getByRole('button', { name: t('update.cancelStage') })
    );
    await waitFor(() => expect(container.querySelector('.update-progress')).toBeNull());

    // The transfer itself cannot be interrupted Rust-side: the bytes land
    // after the cancel, and must not be presented as a stage.
    stageResolvers.shift()!({ staged: '4.6.0', current: '4.5.2' });

    await waitFor(() =>
      expect(invokeMock.mock.calls.filter(([cmd]) => cmd === 'cancel_deferred_update')).toHaveLength(
        2
      )
    );
    expect(container.querySelector('.update-staged')).toBeNull();
    await waitFor(() =>
      expect(within(container).getByRole('button', { name: t('update.installOnQuit') })).toBeTruthy()
    );
  });

  it('releases the progress subscription on unmount, even before listen() resolves', async () => {
    const mounted = render(UpdatePrompt);
    await waitFor(() =>
      expect(listeners.filter((l) => l.event === 'update-stage-progress')).toHaveLength(1)
    );
    mounted.unmount();
    await waitFor(() => expect(released).toBe(1));
    expect(listeners).toHaveLength(0);

    // `listen()` resolves asynchronously: a destroy while it is pending must
    // release the subscription instead of leaking it.
    const pending = render(UpdatePrompt);
    pending.unmount();
    await waitFor(() => expect(released).toBe(2));
    expect(listeners).toHaveLength(0);
  });
});

/**
 * #678 (update-channel slice) — the banner's candidate, its actions and the
 * config it reads them from.
 *
 * Fails pre-fix: the banner discovered its candidate through the plugin's JS
 * `check()`, which cannot take an endpoint list, so a Beta check could only
 * ever have read the stable manifest; the immediate download-and-relaunch
 * action was offered unconditionally, which a beta endpoint list cannot
 * honour; and the store it gated that on was never hydrated by the banner, so
 * a relaunch with a persisted `beta` still offered the stable-only JS path.
 */
describe('UpdatePrompt release channel (#678)', () => {
  it('surfaces the manifest notes and publish date as the banner tooltip', async () => {
    const { container } = await mountBanner();
    const tooltip = container.querySelector('.update-banner')?.getAttribute('title') ?? '';
    expect(tooltip).toContain(CANDIDATE.notes);
    expect(tooltip).toContain(CANDIDATE.pub_date);
  });

  it('discovers its candidate through the backend channel-aware check', async () => {
    await mountBanner();
    expect(invokeMock.mock.calls.map(([cmd]) => cmd)).toContain('check_for_update');
  });

  it('keeps the download path for a persisted stable channel', async () => {
    persistChannel('stable');
    const { container } = await mountBanner();

    await waitFor(() =>
      expect(
        within(container).getByRole('button', { name: t('update.downloadAndInstall') })
      ).toBeTruthy()
    );
    expect(container.querySelector('.update-beta')).toBeNull();
  });

  it('opens the channel gate on the first paint when the store is already hydrated', async () => {
    // The other order: boot hydrated `configStore` before this banner mounted
    // (`+page.svelte` reads through the store and flags `configHydrated`). The
    // channel is authoritative then, so the gate must already be open once the
    // banner has a candidate — not held behind a re-read of a value the app
    // already has.
    configStore.set({ ...structuredClone(defaultConfig), updates: { channel: 'stable' } });
    configHydrated.set(true);
    persistChannel('stable');
    const { container } = await mountBanner();

    // Deliberately synchronous: waiting would hide exactly the flicker this
    // pins.
    expect(
      within(container).getByRole('button', { name: t('update.downloadAndInstall') })
    ).toBeTruthy();
    expect(container.querySelector('.update-beta')).toBeNull();
    expect(
      invokeMock.mock.calls.map(([cmd]) => cmd).filter((cmd) => cmd === 'load_config')
    ).toHaveLength(0);
  });

  it('hydrates the persisted beta channel itself and offers only install-on-quit', async () => {
    persistChannel('beta');
    // The store is deliberately left at the mirror's defaults (stable): the
    // banner must hydrate the persisted channel at its own point of use, not
    // assume that boot already did.
    const { container } = await mountBanner();

    await waitFor(() =>
      expect(container.querySelector('.update-beta')?.textContent?.trim()).toBe(
        t('update.betaOnQuitOnly')
      )
    );
    expect(invokeMock.mock.calls.map(([cmd]) => cmd)).toContain('load_config');
    expect(get(configStore).updates.channel).toBe('beta');
    expect(
      within(container).queryByRole('button', { name: t('update.downloadAndInstall') })
    ).toBeNull();

    // The deferred (Rust) path is still offered, and still stages.
    await startStage(container);
    stageResolvers.shift()!({ staged: '4.6.0', current: '4.5.2' });
    await waitFor(() => expect(container.querySelector('.update-staged')).not.toBeNull());
  });

  it('re-checks when the stored channel changes and replaces the old candidate', async () => {
    const { container } = await mountBanner();
    const checks = () =>
      invokeMock.mock.calls.filter(([cmd]) => cmd === 'check_for_update').length;

    await waitFor(() => expect(checks()).toBe(1));
    expect(container.querySelector('.update-title')?.textContent?.trim()).toBe(
      t('update.available', { version: CANDIDATE.version })
    );

    // Hydration settles the baseline: the banner already holds the persisted
    // channel, so it does not re-check just for having read it (#678's own
    // hydration case) — the switch below is what must start a second check.
    await waitFor(() => expect(get(configHydrated)).toBe(true));
    await tick();
    expect(checks()).toBe(1);

    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) =>
      cmd === 'check_for_update'
        ? { version: '4.7.0', notes: null, pub_date: null }
        : base(cmd, args)
    );

    // Settings writes the channel through the store (#977).
    configStore.set({ ...get(configStore), updates: { channel: 'beta' } });

    await waitFor(() => expect(checks()).toBe(2));
    await waitFor(() =>
      expect(container.querySelector('.update-title')?.textContent?.trim()).toBe(
        t('update.available', { version: '4.7.0' })
      )
    );
  });
});

/**
 * #977 (review) — a check that lands after a newer one started must not
 * overwrite it.
 *
 * The channel switch starts a second check while the first is still in
 * flight, and the first can land last (a slow endpoint, a re-cut manifest).
 * The banner keeps what the newest check resolved.
 *
 * Fails pre-fix: the late first response replaced the newer candidate.
 */
describe('UpdatePrompt check race (#977)', () => {
  it('keeps the newest candidate when the first check resolves last', async () => {
    const pending: ((u: unknown) => void)[] = [];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'load_config') return Promise.resolve(structuredClone(defaultConfig));
      if (cmd === 'check_for_update') {
        const first = Promise.withResolvers<unknown>();
        pending.push(first.resolve);
        return first.promise;
      }
      return Promise.resolve(undefined);
    });

    const rendered = render(UpdatePrompt);
    await waitFor(() => expect(pending).toHaveLength(1));
    // Hydration settles the channel baseline, so the flip below is a switch.
    await waitFor(() => expect(get(configHydrated)).toBe(true));
    await tick();

    configStore.set({ ...structuredClone(defaultConfig), updates: { channel: 'beta' } });
    await waitFor(() => expect(pending).toHaveLength(2));

    // The newer check answers first…
    pending[1]({ version: '4.7.0', notes: null, pub_date: null });
    await waitFor(() =>
      expect(rendered.container.querySelector('.update-title')).not.toBeNull()
    );

    // …then the slow first one lands, carrying the channel it left behind.
    pending[0](CANDIDATE);
    await tick();
    expect(rendered.container.querySelector('.update-title')?.textContent?.trim()).toBe(
      t('update.available', { version: '4.7.0' })
    );
  });
});

/**
 * #737 (update-a11y slice) — the staging position is a progressbar, not live
 * text.
 *
 * The backend emits `update-stage-progress` on a 250 ms floor, and the row
 * used to sit inside the banner's `role="status"` container, so a screen
 * reader had the percentage rewritten for the whole download. The bar now
 * carries the position outside the live region, which reports the discrete
 * stage transitions only.
 *
 * Fails pre-fix: the row was live text with no `progressbar` role, so both the
 * role lookup and the "live region is unchanged across ticks" assertion fail.
 */
describe('UpdatePrompt staging progress (#737)', () => {
  it('exposes the staged position as a progressbar outside the live region', async () => {
    const { container } = await mountBanner();
    await startStage(container);

    // `getByRole` throws on a second match: exactly one bar, named for the
    // update it belongs to.
    const bar = within(container).getByRole('progressbar', {
      name: t('update.available', { version: CANDIDATE.version })
    });
    expect(within(container).getByRole('status').contains(bar)).toBe(false);
    expect(bar.getAttribute('aria-valuemin')).toBe('0');
    expect(bar.getAttribute('aria-valuemax')).toBe('100');

    await emit('update-stage-progress', { downloaded: 45_000_000, total: 100_000_000 });
    expect(bar.getAttribute('aria-valuenow')).toBe('45');
    expect(bar.textContent?.trim()).toBe(t('update.stagingProgress', { percent: 45 }));

    // No Content-Length: the size is unknown, so there is no value to report.
    await emit('update-stage-progress', { downloaded: 1_000_000, total: null });
    expect(bar.hasAttribute('aria-valuenow')).toBe(false);
    expect(bar.textContent?.trim()).toBe(t('update.preparing'));
  });

  it('changes the live region only on a stage transition, whatever the tick count', async () => {
    const { container } = await mountBanner();
    await startStage(container);

    const live = within(container).getByRole('status');
    const announced = live.textContent?.trim();
    expect(announced).toBe(t('update.available', { version: CANDIDATE.version }));

    for (let i = 1; i <= 6; i++) {
      await emit('update-stage-progress', { downloaded: i * 10_000_000, total: 100_000_000 });
    }

    // Six ticks, one unchanged announcement — and the bar did move.
    expect(live.textContent?.trim()).toBe(announced);
    expect(within(container).getByRole('progressbar').getAttribute('aria-valuenow')).toBe('60');

    // The transition the region exists for is announced once it happens.
    stageResolvers.shift()!({ staged: '4.6.0', current: '4.5.2' });
    await waitFor(() => expect(container.querySelector('.update-staged')).not.toBeNull());
    expect(live.textContent).toContain(
      t('update.stagedVsCurrent', { staged: '4.6.0', current: '4.5.2' })
    );
    expect(within(container).queryByRole('progressbar')).toBeNull();
  });
});

/**
 * #982 (update-ux slice) — the immediate download reports an honest position.
 *
 * `progress` is pinned to 0 when the server sends no Content-Length, and the
 * row printed the percentage unconditionally, so the banner read a frozen
 * "0%" for the whole download while the sibling deferred path already said the
 * size was unknown.
 *
 * Fails pre-fix: the unsized branch rendered "0%" instead of the
 * indeterminate copy.
 */
describe('UpdatePrompt download progress (#982)', () => {
  it('says the size is unknown instead of holding a frozen 0%', async () => {
    const { container } = await mountBanner();
    await startDownload(container);

    // Only chunk lengths: no `Started` with a length, so nothing to measure.
    downloadCb!({ event: 'Progress', data: { chunkLength: 1024 } });
    await tick();

    const bar = within(container).getByRole('progressbar');
    expect(bar.textContent?.trim()).toBe(t('update.preparing'));
    expect(bar.textContent ?? '').not.toContain('0%');
    expect(bar.hasAttribute('aria-valuenow')).toBe(false);
  });

  it('keeps the percentage and the MB pair when the size is known', async () => {
    const { container } = await mountBanner();
    await startDownload(container);

    downloadCb!({ event: 'Started', data: { contentLength: 8 * 1024 * 1024 } });
    downloadCb!({ event: 'Progress', data: { chunkLength: 4 * 1024 * 1024 } });
    await tick();

    const bar = within(container).getByRole('progressbar');
    expect(bar.textContent?.trim()).toBe('50% (4/8 MB)');
    expect(bar.getAttribute('aria-valuenow')).toBe('50');
  });
});

/**
 * #957 (updater shell slice) — an already-current stage is not a stale decline.
 *
 * The backend reports why nothing was staged; only the stale reasons may
 * render the stale-skip copy with its "Install anyway" button, which for the
 * already-current case would repeat the same no-op.
 *
 * Fails pre-fix: any falsy `staged` rendered the stale-skip state.
 */
describe('UpdatePrompt deferred stage outcome (#957)', () => {
  it('drops the candidate when there was nothing to stage because it is current', async () => {
    const { container } = await mountBanner();
    await startStage(container);

    stageResolvers.shift()!({ staged: null, current: '4.5.2', skipped: 'current' });

    // Nothing to install: the banner goes away with the candidate instead of
    // offering a version the manifest no longer has.
    await waitFor(() => expect(container.querySelector('.update-banner')).toBeNull());
    expect(container.querySelector('.update-stale')).toBeNull();
    expect(
      within(container).queryByRole('button', { name: t('update.installAnyway') })
    ).toBeNull();
  });

  it('still reports a stale decline with its install-anyway choice', async () => {
    const { container } = await mountBanner();
    await startStage(container);

    stageResolvers.shift()!({ staged: null, current: '4.5.2', skipped: 'stale' });

    await waitFor(() => expect(container.querySelector('.update-stale')).not.toBeNull());
    expect(within(container).getByRole('button', { name: t('update.installAnyway') })).toBeTruthy();
  });
});

/**
 * #950, #951 (UI components slice) — the strip stays on one line inside its
 * border, and out of the Dashboard header's way.
 *
 * Fails pre-fix: the banner floated at `top: var(--sp-3)` with no placement
 * variant, its info column was the only shrinkable item, and its action group
 * could not wrap.
 */
describe('UpdatePrompt banner layout (#950, #951)', () => {
  it('docks the strip, keeps its info column shrinkable and its actions wrapping', async () => {
    const { container } = await mountBanner();
    const banner = container.querySelector('.update-banner');
    expect(banner?.classList.contains('update-banner--docked')).toBe(true);

    const css = bannerCss();
    // #951: anchored to the bottom edge, never to the top. The inset clears
    // the app's own bottom-centre controls; the geometry was hit-tested in a
    // browser at 600x750 and 400x500 (see the #951 commit).
    expect(css).toMatch(
      /\.update-banner--docked\s*\{[^}]*bottom:\s*calc\(var\(--sp-10\) \+ var\(--sp-1\)\)/
    );
    expect(css).not.toMatch(/\.update-banner--docked\s*\{[^}]*\btop:/);
    // #950: the info column owns the free space and may shrink; the action
    // group wraps instead of pushing the flex line past the border.
    expect(css).toMatch(/\.update-info\s*\{[^}]*flex:\s*1 1 auto/);
    expect(css).toMatch(/\.update-info\s*\{[^}]*min-width:\s*0/);
    expect(css).toMatch(/\.update-actions\s*\{[^}]*flex-wrap:\s*wrap/);
    // #950: the one-line rows are ellipsised, so a long title or the beta note
    // cannot wrap the strip into a tall block.
    expect(css).toMatch(/\.update-title,[\s\S]{0,260}?overflow:\s*hidden/);
    expect(css).toMatch(/\.update-title,[\s\S]{0,260}?text-overflow:\s*ellipsis/);
    expect(css).toMatch(/\.update-title,[\s\S]{0,260}?white-space:\s*nowrap/);
  });
});

/**
 * #977 (review) — a staged payload outlives the candidate it came from.
 *
 * The channel switch defers while a stage is in flight (the strip owns the
 * only "Cancel stage" affordance), and the deferred check can then find that
 * the new channel offers nothing at all. The staged bytes still install at
 * quit, so the banner — and its cancel action — must survive both halves of
 * that sequence.
 *
 * Fails pre-fix: the switch nulled the candidate, the `{#if update}` guard
 * took the banner with it, and the payload had no way back.
 */
describe('UpdatePrompt channel switch with a staged payload (#977)', () => {
  it('keeps the staged payload cancellable through the deferred re-check', async () => {
    const { container } = await mountBanner();
    await startStage(container);

    // The channel the user switches to offers nothing at all.
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) =>
      cmd === 'check_for_update' ? null : base(cmd, args)
    );

    configStore.set({ ...get(configStore), updates: { channel: 'beta' } });
    await tick();

    const cancel = () =>
      within(container).getByRole('button', { name: t('update.cancelStage') });
    // Deferred while the stage is in flight: banner and cancel action intact.
    expect(container.querySelector('.update-banner')).not.toBeNull();
    expect(cancel()).toBeTruthy();

    // The stage lands, so the deferred switch re-checks and finds no
    // candidate — the staged payload must still be cancellable.
    stageResolvers.shift()!({ staged: '4.6.0', current: '4.5.2' });
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter(([cmd]) => cmd === 'check_for_update')).toHaveLength(2)
    );
    await waitFor(() => expect(container.querySelector('.update-staged')).not.toBeNull());
    expect(container.querySelector('.update-banner')).not.toBeNull();
    expect(cancel()).toBeTruthy();

    // And the action still reaches the backend, dropping the payload.
    await fireEvent.click(cancel());
    await waitFor(() => expect(container.querySelector('.update-staged')).toBeNull());
    expect(
      invokeMock.mock.calls.filter(([cmd]) => cmd === 'cancel_deferred_update')
    ).toHaveLength(1);
  });
});
