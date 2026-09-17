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
vi.mock('@tauri-apps/plugin-updater', () => ({
  check: vi.fn(async () => ({ version: '4.6.0' }))
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
});
