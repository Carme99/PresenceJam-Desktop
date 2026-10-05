/**
 * #887 — a burst of streamed log records must cost ONE layout read and ONE
 * animation frame per animation frame, not one of each per line.
 *
 * Pre-fix, every `log://log` delivery ran the whole bookkeeping path:
 * a forced `isAtBottom()` read, a scroll-anchor capture, an O(n) `shift()`
 * over the ~500-row buffer, a full filter recomputation, and its own
 * `requestAnimationFrame`. A Trace-level burst therefore queued one rAF per
 * line and re-read geometry for each — jank that scaled with the log rate
 * rather than with what is on screen.
 *
 * jsdom has no layout engine, so `scrollHeight`/`clientHeight`/`scrollTop`
 * are pinned here and each read is COUNTED — the counts are the observable
 * contract. `requestAnimationFrame` is replaced with a manual queue so the
 * test decides exactly when a frame runs, which is what makes "per frame"
 * a testable statement rather than a timing race.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { tick } from 'svelte';
import { render, cleanup, fireEvent } from '@testing-library/svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

type Listener = (e: { payload: { level: number; message: string } }) => void;
const listeners: Listener[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_event: string, fn: (e: unknown) => void) => {
    listeners.push(fn as never);
    return () => {};
  })
}));

vi.mock('$lib/stores/detach', () => ({
  popOut: vi.fn().mockResolvedValue(undefined),
  popIn: vi.fn().mockResolvedValue(undefined)
}));

import LogViewer from '$lib/components/LogViewer.svelte';

/** Deliver `count` records back-to-back, with no frame in between. */
function burst(count: number) {
  for (let i = 0; i < count; i++) {
    for (const fn of listeners) fn({ payload: { level: 3, message: `line-${i}` } });
  }
}

function emit(level: number, message: string) {
  for (const fn of listeners) fn({ payload: { level, message } });
}

// The frame queue the component schedules into. Held rather than run so the
// test controls frame boundaries and can assert what a frame COST.
let frames: FrameRequestCallback[] = [];
let rafCalls = 0;

/** Run every queued frame callback once. */
function runFrame() {
  const queued = frames;
  frames = [];
  for (const cb of queued) cb(0);
}

// Current value of the stubbed `scrollTop` (jsdom never moves it itself).
let scrollTop = 0;
// How many times the component has read each geometry property. These are
// the forced-layout reads the issue is about.
let heightReads = 0;

/**
 * Pin the geometry the component reads and count every read of the two
 * properties whose access forces a synchronous layout in a real browser.
 */
function stubGeometry(list: HTMLElement, scrollHeight: number, clientHeight: number, initial: number) {
  scrollTop = initial;
  heightReads = 0;
  Object.defineProperty(list, 'scrollHeight', {
    configurable: true,
    get: () => {
      heightReads += 1;
      return scrollHeight;
    }
  });
  Object.defineProperty(list, 'clientHeight', { value: clientHeight, configurable: true });
  Object.defineProperty(list, 'scrollTop', {
    configurable: true,
    get: () => scrollTop,
    set: (value: number) => {
      scrollTop = value;
    }
  });
}

// Counts `offsetTop` reads made by the scroll-anchor capture; see
// `instrumentRows`. Reset per test in `beforeEach`.
let anchorReadCounter = 0;

beforeEach(() => {
  listeners.length = 0;
  anchorReadCounter = 0;
  frames = [];
  rafCalls = 0;
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
    rafCalls += 1;
    frames.push(cb);
    return frames.length;
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/**
 * Count the `offsetTop` reads `captureScrollAnchor` makes on rendered rows.
 *
 * The anchor capture is the only thing in the component that reads a row's
 * `offsetTop`, and only while the pane is unpinned — so this counter is a
 * direct observable for "did this frame re-anchor?". The values themselves
 * are 0 in jsdom and cannot distinguish anything; the COUNT is the signal.
 *
 * The getters must be installed on the CURRENTLY rendered rows before the
 * burst, because the rows are replaced on every render.
 */
function instrumentRows(list: HTMLElement): void {
  for (const row of list.querySelectorAll<HTMLElement>('.log-entry')) {
    Object.defineProperty(row, 'offsetTop', {
      configurable: true,
      get: () => {
        anchorReadCounter += 1;
        return 0;
      }
    });
  }
}

describe('LogViewer burst coalescing (#887)', () => {
  it('runs one frame and one layout read for a burst, whatever the log rate', async () => {
    const { container } = render(LogViewer, { detached: false });
    const list = container.querySelector('.log-list') as HTMLElement;
    // Pinned: the pane is scrolled to the bottom, the state a streaming log
    // spends essentially all of its time in.
    stubGeometry(list, 1000, 200, 800);
    await tick();

    rafCalls = 0;
    burst(1000);
    await tick();

    // The whole burst landed inside one frame.
    expect(rafCalls, 'a 1000-line burst must queue one frame, not 1000').toBe(1);
    // The pre-push sample is taken once for the frame, and `runFrameWork`
    // reads the geometry once more. Pre-fix this was >= 2000.
    expect(heightReads, 'a 1000-line burst must not re-read layout per line').toBeLessThanOrEqual(2);

    runFrame();

    // Still one layout read inside the frame itself, and the snap landed.
    expect(heightReads).toBeLessThanOrEqual(2);
    expect(scrollTop, 'the pane must still auto-scroll to the bottom').toBe(1000);
  });

  it('does not scale layout work with the log rate', async () => {
    const { container } = render(LogViewer, { detached: false });
    const list = container.querySelector('.log-list') as HTMLElement;
    stubGeometry(list, 1000, 200, 800);
    await tick();

    // Measure each burst's cost separately: the read counter is cumulative,
    // so a delta is what says whether the cost is per-frame or per-record.
    heightReads = 0;
    rafCalls = 0;
    burst(100);
    await tick();
    runFrame();
    const readsFor100 = heightReads;
    const framesFor100 = rafCalls;

    heightReads = 0;
    rafCalls = 0;
    burst(1000);
    await tick();
    runFrame();
    const readsFor1000 = heightReads;

    // Ten times the records must not cost ten times the frames...
    expect(rafCalls).toBe(framesFor100);
    // ...nor any more layout reads. Pre-fix the 1000-record burst read the
    // geometry ~3000 times against ~300 for the 100-record one.
    expect(readsFor1000).toBe(readsFor100);
  });

  it('keeps the newest entries and bounds the buffer when evicting in batches', async () => {
    const { container } = render(LogViewer, { detached: false });
    const list = container.querySelector('.log-list') as HTMLElement;
    stubGeometry(list, 100_000, 200, 99_800);
    await tick();

    burst(1000);
    await tick();
    runFrame();

    const rows = container.querySelectorAll('.log-entry');
    // #399 render window: the DOM holds the tail only.
    expect(rows.length).toBe(100);
    // The rendered tail is the NEWEST entries — eviction drops the oldest.
    expect(rows[rows.length - 1].textContent).toContain('line-999');
    expect(rows[0].textContent).toContain('line-900');
    // The oldest records are gone from the buffer, so they are not counted.
    expect(container.textContent).not.toContain('line-0 ');
  });

  it('holds the reader position for an unpinned pane during a burst', async () => {
    const { container } = render(LogViewer, { detached: false });
    const list = container.querySelector('.log-list') as HTMLElement;
    // Two entries so the pane has rows to anchor against and the Jump button
    // (which only renders with content) is available as an assertion.
    emit(3, 'before-a');
    emit(3, 'before-b');
    await tick();
    // 1000 - 300 - 200 = 500px from the bottom -> unpinned.
    stubGeometry(list, 1000, 200, 300);
    await fireEvent.scroll(list);
    await tick();
    expect(container.querySelector('.jump-latest')).not.toBeNull();
    burst(50);
    await tick();
    runFrame();

    // Unpinned means unpinned: the burst did not yank the reader back down.
    expect(scrollTop, 'an unpinned pane must not be snapped to the bottom').toBe(300);
    expect(container.querySelector('.jump-latest')).not.toBeNull();
  });

  it('re-anchors on every frame of a sustained unpinned burst', async () => {
    // The latch that coalesces geometry sampling must be cleared by the
    // frame itself, not left to the `scroll` event a restore would fire:
    // once a burst evicts the anchor row, `restoreScrollAnchor` writes
    // nothing and no scroll event fires, so a latch left set would stop the
    // anchor ever being captured again and the reader's place would slide one
    // row per frame (#600). jsdom never fires scroll on its own, so this
    // harness is exactly the environment in which that regression is
    // invisible unless the component clears the latch itself.
    const { container } = render(LogViewer, { detached: false });
    const list = container.querySelector('.log-list') as HTMLElement;
    emit(3, 'anchor-a');
    emit(3, 'anchor-b');
    await tick();
    stubGeometry(list, 4000, 200, 300);
    await fireEvent.scroll(list);
    await tick();

    // Two successive frames, each with a burst big enough to evict the row
    // the previous frame anchored on. Instrument the rows that are rendered
    // NOW, then burst: the capture reads their `offsetTop` synchronously
    // inside the listener, before the frame runs.
    instrumentRows(list);
    burst(120);
    await tick();
    expect(
      anchorReadCounter,
      'the first burst frame must capture a scroll anchor'
    ).toBeGreaterThan(0);
    runFrame();

    // The rows were re-rendered by that burst, so re-instrument before the
    // second one and check the capture happened again. Without the frame
    // clearing the latch, the second frame would skip the capture entirely
    // and the reader's place would slide one row per frame (#600).
    instrumentRows(list);
    anchorReadCounter = 0;
    burst(120);
    await tick();
    expect(
      anchorReadCounter,
      'a later burst frame must capture a fresh scroll anchor, not reuse a spent sample'
    ).toBeGreaterThan(0);
    const beforeSecondFrame = scrollTop;
    runFrame();
    expect(scrollTop).toBe(beforeSecondFrame);
  });
});