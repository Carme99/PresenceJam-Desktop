/**
 * #492 — LogViewer behavior, exercised through the real component.
 *
 * The slice does not change LogViewer logic (it already carries the
 * #399 tail window, #400 stickiness, #401 Trace fixes); these tests pin
 * the behaviors #492 lists so a regression — unkeyed full re-render,
 * forced scroll-to-bottom, dropped Trace tab — fails CI. Fail pre-fix
 * (if any of the three behaviors regress), pass post-fix.
 *
 * #692 adds the listener-teardown cases at the bottom: the pane must
 * release a `log://log` registration that settles *after* it unmounts.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const listeners: Array<(e: { payload: { level: number; message: string } }) => void> = [];

// #692 regression harness. `listen()` resolves asynchronously and the pane
// can unmount inside that window, so the mock can hold every registration in
// flight (`holdRegistrations`) until the test releases it — the leak only
// shows up in exactly that ordering. `registrationsCreated` /
// `unlistenCalls` are the two halves of "was this subscription released?".
let holdRegistrations = false;
let registrationsCreated = 0;
let unlistenCalls = 0;
let heldResolvers: Array<() => void> = [];

vi.mock('@tauri-apps/api/event', () => ({
  // Mirror the real signature: listen(eventName, handler).
  listen: vi.fn((_event: string, fn: (e: unknown) => void) => {
    listeners.push(fn as never);
    registrationsCreated++;
    if (!holdRegistrations) {
      return Promise.resolve(() => {
        unlistenCalls++;
      });
    }
    return new Promise<() => void>((resolve) => {
      heldResolvers.push(() =>
        resolve(() => {
          unlistenCalls++;
        })
      );
    });
  })
}));

/** Settle every registration the harness is holding. */
async function releaseHeld(): Promise<void> {
  const held = heldResolvers.splice(0);
  for (const resolve of held) resolve();
  await Promise.resolve();
  await Promise.resolve();
}

vi.mock('$lib/stores/detach', () => ({
  popOut: vi.fn().mockResolvedValue(undefined),
  popIn: vi.fn().mockResolvedValue(undefined)
}));
// Static import: component path is author-time known (vi.mock calls hoist
// above it, so Tauri mocks still apply at load time).
import LogViewer from '$lib/components/LogViewer.svelte';
import { i18n } from '$lib/i18n';

function emit(level: number, message: string) {
  for (const fn of listeners) fn({ payload: { level, message } });
}

beforeEach(() => {
  listeners.length = 0;
  holdRegistrations = false;
  registrationsCreated = 0;
  unlistenCalls = 0;
  heldResolvers = [];
});

afterEach(() => {
  // Unmount each render: the jsdom document is shared per file, so
  // getByRole would otherwise match tabs from earlier tests.
  cleanup();
});

describe('LogViewer behavior (#492)', () => {
  it('renders a capped keyed tail with showing-X-of-Y on a 500 burst', async () => {
    const { container } = render(LogViewer, { detached: false });
    for (let i = 0; i < 500; i++) emit(3, `msg-${i}`);
    await Promise.resolve();
    const rows = container.querySelectorAll('.log-entry');
    expect(rows.length).toBeLessThanOrEqual(100);
    expect(rows.length).toBeGreaterThan(0);
    expect(container.querySelector('.count')?.textContent).toMatch(/Showing \d+ of 500/);
    // Tail window: newest message visible, oldest burst entry evicted.
    expect(container.textContent).toContain('msg-499');
    expect(container.textContent).not.toContain('msg-0');
  });

  it('Trace tab isolates level-1 logs', async () => {
    const { container, getByRole } = render(LogViewer, { detached: false });
    emit(1, 'trace-one');
    emit(3, 'info-one');
    await Promise.resolve();
    await fireEvent.click(getByRole('tab', { name: 'Trace' }));
    expect(container.textContent).toContain('trace-one');
    expect(container.textContent).not.toContain('info-one');
  });

  it('offers Jump to latest only when scrolled up', async () => {
    const { container } = render(LogViewer, { detached: false });
    emit(3, 'hello');
    await Promise.resolve();
    const list = container.querySelector('.log-list') as HTMLElement;
    // At bottom: no jump button.
    expect(container.querySelector('.jump-latest')).toBeNull();
    // Scroll up: button appears.
    Object.defineProperty(list, 'scrollHeight', { value: 1000, configurable: true });
    Object.defineProperty(list, 'clientHeight', { value: 200, configurable: true });
    list.scrollTop = 100;
    await fireEvent.scroll(list);
    expect(container.querySelector('.jump-latest')).not.toBeNull();
  });

  it('labels the count through CLDR plural rules, not `count === 1` (#616)', async () => {
    const { container, getByRole } = render(LogViewer, { detached: false });
    const count = () => container.querySelector('.count')?.textContent?.trim();

    emit(3, 'first');
    await Promise.resolve();
    expect(count()).toBe('1 entry');

    emit(3, 'second');
    await Promise.resolve();
    expect(count()).toBe('2 entries');

    await fireEvent.click(getByRole('button', { name: 'Clear' }));
    await tick();
    expect(count()).toBe('0 entries');

    // CLDR fr puts 0 in the `one` category, and the label follows the locale
    // live — the hand-rolled `count === 1` rendered "0 entrées" here.
    i18n.set('fr');
    await tick();
    expect(count()).toBe('0 entrée');
    i18n.set('en');
  });
});

/**
 * #692 — the pane used to `await listen()` inside `onMount` and release the
 * result from an array in `onDestroy`. Unmounting while the registration was
 * in flight left the unlisten landing in an array nobody swept again, so
 * every visit leaked one subscription. These cases hold the registration
 * open across the unmount and assert it is still released.
 */
describe('LogViewer listener teardown (#692)', () => {
  it('releases a registration that settles after the pane unmounts', async () => {
    holdRegistrations = true;
    const { unmount } = render(LogViewer, { detached: false });
    await waitFor(() => expect(registrationsCreated).toBe(1));
    expect(unlistenCalls).toBe(0);

    unmount();
    // The subscription only appears now — the pane is already gone.
    await releaseHeld();

    expect(unlistenCalls).toBe(1);
  });

  it('leaves no live subscription behind after three visits', async () => {
    for (let visit = 0; visit < 3; visit++) {
      holdRegistrations = true;
      const { unmount } = render(LogViewer, { detached: false });
      await waitFor(() => expect(registrationsCreated).toBe(visit + 1));
      unmount();
      await releaseHeld();
    }

    expect(registrationsCreated).toBe(3);
    expect(unlistenCalls).toBe(3);
  });
});
