/**
 * #492 — LogViewer behavior, exercised through the real component.
 *
 * The slice does not change LogViewer logic (it already carries the
 * #399 tail window, #400 stickiness, #401 Trace fixes); these tests pin
 * the behaviors #492 lists so a regression — unkeyed full re-render,
 * forced scroll-to-bottom, dropped Trace tab — fails CI. Fail pre-fix
 * (if any of the three behaviors regress), pass post-fix.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
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
// Static import: component path is author-time known (vi.mock calls hoist
// above it, so Tauri mocks still apply at load time).
import LogViewer from '$lib/components/LogViewer.svelte';

function emit(level: number, message: string) {
  for (const fn of listeners) fn({ payload: { level, message } });
}

beforeEach(() => {
  listeners.length = 0;
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
});
