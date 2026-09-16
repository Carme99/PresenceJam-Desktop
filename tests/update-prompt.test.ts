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

const invokeMock = invoke as unknown as Mock;

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

beforeEach(() => {
  listeners.length = 0;
  released = 0;
  stageResolvers = [];
  i18n.set('en');
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'stage_deferred_update') {
      const outcome = Promise.withResolvers<unknown>();
      stageResolvers.push(outcome.resolve);
      return outcome.promise;
    }
    return Promise.resolve(undefined);
  });
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
