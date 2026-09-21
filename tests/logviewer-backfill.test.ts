/**
 * #595 — the LogViewer seeds its buffer from the on-disk log tail.
 *
 * The pane used to render "No log entries yet" after the app had been running
 * for hours, while `PresenceJam.log` held the whole session and the "Copy
 * snapshot" button in the same toolbar pasted that very history. Mount now
 * reads the tail through `get_recent_logs` and prepends it, while `log://log`
 * stays authoritative for everything logged afterwards.
 *
 * The interaction contract pinned here:
 *   - the seeded entries render with the file's levels canonicalised, so the
 *     existing filter tabs match them;
 *   - a tail that lands after the user has scrolled away must NOT move the
 *     scroll position (the seed goes into the buffer, not the viewport);
 *   - a pane still pinned to the bottom follows the content, as before;
 *   - Clear wins over a tail still in flight.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, cleanup, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import type { Mock } from 'vitest';

type Listener = (e: { payload: { level: number; message: string } }) => void;
const listeners: Listener[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_event: string, fn: (e: unknown) => void) => {
    listeners.push(fn as Listener);
    return () => {};
  })
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('$lib/stores/detach', () => ({
  popOut: vi.fn().mockResolvedValue(undefined),
  popIn: vi.fn().mockResolvedValue(undefined)
}));

import { invoke } from '@tauri-apps/api/core';
import LogViewer from '$lib/components/LogViewer.svelte';
import { i18n } from '$lib/i18n';

const invokeMock = invoke as unknown as Mock;

/** Lines exactly as tauri-plugin-log's LogDir target writes them (UTC). */
const FILE_LINES = [
  '[2026-09-16][04:00:01][pj_lib::polling][INFO] [POLLING] started',
  '[2026-09-16][04:00:02][pj_lib::polling][WARN] [POLLING] rate limited',
  '[2026-09-16][04:00:03][pj_lib::teams][ERROR] [TEAMS] refresh failed'
];

/** Resolve the in-flight `get_recent_logs` call — the tail lands when we say. */
let resolveBackfill: ((lines: string[]) => void) | undefined;

function emit(level: number, message: string) {
  for (const fn of listeners) fn({ payload: { level, message } });
}

const nextFrame = () => new Promise<void>((r) => requestAnimationFrame(() => r()));

// Current value of the stubbed `scrollTop` (jsdom never moves it itself).
let scrollTop = 0;

/** Pin the geometry the component reads and the scroll position it writes. */
function stubGeometry(list: HTMLElement, scrollHeight: number, clientHeight: number, initial: number) {
  scrollTop = initial;
  Object.defineProperty(list, 'scrollHeight', { value: scrollHeight, configurable: true });
  Object.defineProperty(list, 'clientHeight', { value: clientHeight, configurable: true });
  Object.defineProperty(list, 'scrollTop', {
    configurable: true,
    get: () => scrollTop,
    set: (value: number) => {
      scrollTop = value;
    }
  });
}

/** Wait until the component has registered its live listener. */
async function listenerReady() {
  await waitFor(() => expect(listeners.length).toBe(1));
}

beforeEach(() => {
  listeners.length = 0;
  resolveBackfill = undefined;
  i18n.set('en');
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'get_recent_logs') {
      return new Promise<string[]>((resolve) => {
        resolveBackfill = resolve;
      });
    }
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  cleanup();
});

describe('LogViewer history backfill (#595)', () => {
  it('seeds the buffer from the on-disk tail, oldest first', async () => {
    const { container } = render(LogViewer, { detached: false });
    await listenerReady();
    expect(invokeMock).toHaveBeenCalledWith('get_recent_logs', { limit: 500 });

    resolveBackfill?.(FILE_LINES);
    await waitFor(() => expect(container.querySelectorAll('.log-entry').length).toBe(3));

    const rows = container.querySelectorAll('.log-entry');
    expect(rows[0].textContent).toContain('[POLLING] started');
    expect(rows[2].textContent).toContain('[TEAMS] refresh failed');
    // The file's own level word is canonicalised to the name the filter tabs
    // compare against, so a seeded WARN is filterable.
    expect(rows[1].querySelector('.level-badge')?.className).toContain('level-warning');
    expect(container.querySelector('.count')?.textContent).toMatch(/3 entries/i);
  });

  it('filters the seeded history through the existing level tabs', async () => {
    const { container, getByRole } = render(LogViewer, { detached: false });
    await listenerReady();
    resolveBackfill?.(FILE_LINES);
    await waitFor(() => expect(container.querySelectorAll('.log-entry').length).toBe(3));

    await fireEvent.click(getByRole('tab', { name: 'Error' }));
    expect(container.textContent).toContain('[TEAMS] refresh failed');
    expect(container.textContent).not.toContain('[POLLING] started');
  });

  it('keeps the live stream authoritative after the seed', async () => {
    const { container } = render(LogViewer, { detached: false });
    await listenerReady();
    // A live entry races the disk read: it must survive, and land after the
    // history (it happened later).
    emit(3, 'live-before-seed');
    await tick();
    resolveBackfill?.(FILE_LINES);
    await waitFor(() => expect(container.querySelectorAll('.log-entry').length).toBe(4));

    emit(3, 'live-after-seed');
    await tick();

    const rows = container.querySelectorAll('.log-entry');
    expect(rows[rows.length - 1].textContent).toContain('live-after-seed');
    expect(rows[rows.length - 2].textContent).toContain('live-before-seed');
    expect(rows[0].textContent).toContain('[POLLING] started');
  });

  it('does not move a pane the user has scrolled away from', async () => {
    const { container } = render(LogViewer, { detached: false });
    await listenerReady();

    emit(3, 'live-1');
    await tick();
    const list = container.querySelector('.log-list') as HTMLElement;
    // 1000 - 300 - 200 = 500px from the bottom -> unpinned.
    stubGeometry(list, 1000, 200, 300);
    await fireEvent.scroll(list);
    expect(container.querySelector('.jump-latest')).not.toBeNull();

    resolveBackfill?.(FILE_LINES);
    await waitFor(() => expect(container.querySelectorAll('.log-entry').length).toBe(4));
    await nextFrame();

    // The history is in the buffer, but the reader's place is untouched.
    expect(scrollTop).toBe(300);
  });

  it('still follows the seed to the bottom while pinned', async () => {
    const { container } = render(LogViewer, { detached: false });
    await listenerReady();

    emit(3, 'live-1');
    await tick();
    const list = container.querySelector('.log-list') as HTMLElement;
    // Pinned, and the list grows underneath by the seeded rows.
    stubGeometry(list, 2000, 200, 1780);
    expect(container.querySelector('.jump-latest')).toBeNull();

    resolveBackfill?.(FILE_LINES);
    await waitFor(() => expect(container.querySelectorAll('.log-entry').length).toBe(4));
    await nextFrame();

    expect(scrollTop).toBe(2000);
  });

  it('drops a tail still in flight when the user clears the pane', async () => {
    const { container, getByRole } = render(LogViewer, { detached: false });
    await listenerReady();

    await fireEvent.click(getByRole('button', { name: 'Clear' }));
    resolveBackfill?.(FILE_LINES);
    await tick();
    await nextFrame();

    expect(container.querySelectorAll('.log-entry').length).toBe(0);
    expect(container.textContent).toContain('No log entries yet');
  });

  it('ignores an unexpected payload instead of failing the mount', async () => {
    // A rejected/torn backend read must not take the live pane down with it.
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === 'get_recent_logs' ? Promise.reject(new Error('no log dir')) : undefined
    );
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { container } = render(LogViewer, { detached: false });
    await listenerReady();

    emit(3, 'live-only');
    await waitFor(() => expect(container.querySelectorAll('.log-entry').length).toBe(1));
    expect(container.textContent).toContain('live-only');
    warn.mockRestore();
  });
});

/**
 * #958 — dedupe the LogViewer backfill against the live `log://log` stream.
 *
 * A record logged in the gap between the file read and the seed landing
 * arrives in two places: as the last line of the file tail AND as a
 * `log://log` delivery. The pre-#958 merge `[...seeded, ...logs].slice(-MAX_BUFFER)`
 * put both copies in the buffer, so the row rendered (and was counted) twice.
 * The fix routes live events into a `pending` buffer while the read is in
 * flight, then drops the pending copies whose dedupe key already appears in
 * the seeded tail. Both cases pinned here:
 *   - colliding key: the live event collapses to the seeded row, count is 3;
 *   - distinct key: the live event passes through, count is 4.
 * The dedupe key is `(timestamp, level, message)`; the harness freezes the
 * wall clock to a moment whose local representation matches the parsed UTC
 * timestamp of the targeted seeded line, so the test exercises the collision
 * path deterministically rather than relying on real-time coincidence.
 */
describe('LogViewer history backfill dedup (#958)', () => {
  afterEach(() => {
    // Fake timers set inside a test must be released even if the assertion
    // threw, otherwise the next test inherits a frozen clock and any
    // subsequent `Date.now()` / `new Date()` use goes sideways.
    vi.useRealTimers();
  });

  it('drops a live event whose dedupe key matches the seeded tail', async () => {
    // Freeze the wall clock so the live event's `toLocaleTimeString()`
    // matches the parsed UTC timestamp of the last seeded line — that is
    // the only way the (timestamp, level, message) dedupe key collides in
    // the harness without depending on real-time coincidence.
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-16T04:00:03Z'));

    const { container } = render(LogViewer, { detached: false });
    await listenerReady();

    // The last seeded line is `[2026-09-16][04:00:03][pj_lib::teams][ERROR] [TEAMS] refresh failed`.
    // Parsed: timestamp = local(2026-09-16T04:00:03Z), level = 'Error',
    // message = `[pj_lib::teams] [TEAMS] refresh failed`. A live delivery of
    // exactly that record — same (timestamp, level, message) — before the
    // read resolves is the scenario the bug calls out: both surfaces cover
    // the same row.
    emit(5, '[pj_lib::teams] [TEAMS] refresh failed');
    await tick();
    resolveBackfill?.(FILE_LINES);

    // 3 entries — not 4. The duplicate collapses to a single row, and the
    // count label reflects that.
    await waitFor(() => expect(container.querySelectorAll('.log-entry').length).toBe(3));
    expect(container.querySelector('.count')?.textContent).toMatch(/3 entries/i);
  });

  it('keeps a live event whose dedupe key does NOT match any seeded entry', async () => {
    // Distinct message => distinct dedupe key => the live event survives
    // the merge unchanged and lands after the seeded tail. This is the
    // explicit non-duplicate case for the gate: false positives in the
    // dedupe would clip legitimate events here.
    const { container } = render(LogViewer, { detached: false });
    await listenerReady();

    emit(5, 'unique-live-message-not-in-seed');
    await tick();
    resolveBackfill?.(FILE_LINES);

    await waitFor(() => expect(container.querySelectorAll('.log-entry').length).toBe(4));
    expect(container.querySelector('.count')?.textContent).toMatch(/4 entries/i);
  });
});
