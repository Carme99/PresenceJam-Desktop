/**
 * #600 — a reader who scrolled up must not have the text slide under them.
 *
 * The DOM renders only the tail window (#399), so every appended entry evicts
 * a row from the top of the rendered list and the whole block would move up
 * one row per event (continuous at Trace level). The fix anchors on a row
 * that survives the eviction and re-applies its content-space delta.
 *
 * jsdom has no layout engine, so heights/`scrollTop`/`offsetTop` are pinned
 * here: the assertions are about the arithmetic the component applies to that
 * geometry, and about the pinned path staying on its existing snap.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { tick } from 'svelte';
import { render, fireEvent, cleanup } from '@testing-library/svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const listeners: Array<(e: { payload: { level: number; message: string } }) => void> = [];

vi.mock('@tauri-apps/api/event', () => ({
  // Mirror the real signature: listen(eventName, handler).
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

function emit(level: number, message: string) {
  for (const fn of listeners) fn({ payload: { level, message } });
}

// Await the frame the component itself scheduled, rather than a guessed delay.
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

function setOffsetTop(row: HTMLElement, value: number) {
  Object.defineProperty(row, 'offsetTop', { value, configurable: true });
}

beforeEach(() => {
  listeners.length = 0;
});

afterEach(() => {
  cleanup();
});

describe('LogViewer scroll anchoring (#600)', () => {
  it('holds the reader position when an entry arrives while scrolled up', async () => {
    const { container } = render(LogViewer, { detached: false });
    emit(3, 'msg-a');
    emit(3, 'msg-b');
    emit(3, 'msg-c');
    await tick();

    const list = container.querySelector('.log-list') as HTMLElement;
    // 1000 - 300 - 200 = 500px from the bottom -> unpinned.
    stubGeometry(list, 1000, 200, 300);
    await fireEvent.scroll(list);
    expect(container.querySelector('.jump-latest')).not.toBeNull();

    const rows = container.querySelectorAll<HTMLElement>('.log-entry');
    expect(rows.length).toBe(3);
    // The anchor is the row behind the head (the head is what the next
    // entry evicts); it survives the update, so it can be measured after.
    const anchor = rows[1];
    setOffsetTop(anchor, 120);

    emit(3, 'msg-d');
    // The rendered window moved up by 40px: without the anchor, the text
    // under the reader would shift by exactly that much.
    setOffsetTop(anchor, 80);
    await nextFrame();

    // scrollTop was compensated by the anchor delta (300 + 80 - 120).
    expect(scrollTop).toBe(260);
  });

  it('still snaps to the bottom while pinned', async () => {
    const { container } = render(LogViewer, { detached: false });
    emit(3, 'msg-a');
    await tick();

    const list = container.querySelector('.log-list') as HTMLElement;
    // 1000 - 800 - 200 = 0px from the bottom -> pinned.
    stubGeometry(list, 1000, 200, 800);
    emit(3, 'msg-b');
    await nextFrame();

    expect(scrollTop).toBe(1000);
  });
});
