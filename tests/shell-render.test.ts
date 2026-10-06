/**
 * #779 — the app shell renders the route's content through the `children`
 * snippet instead of the legacy `<slot />`.
 *
 * SvelteKit hands a layout its page as the `children` snippet. While the
 * shell still declared `<slot />`, that prop was ignored: mounting the
 * layout with page content rendered the shell's chrome and no body at all —
 * exactly the blank page body a Svelte 6 bump would have shipped to every
 * user.
 *
 * Fails pre-fix: the assertion that the page body lands inside the shell
 * fails, because `<slot />` renders nothing when the layout is handed a
 * `children` snippet.
 *
 * The layout reads `__TAURI_INTERNALS__` and `getCurrentWindow()` at module
 * init, so the runtime flag and the window module are installed through
 * `vi.hoisted` — after hoisting the layout would take the plain-browser
 * branch, which deliberately renders the boot notice instead of page
 * content. `detached-logs` keeps the main-window-only IPC work off, so the
 * mount is the cheapest real shell a test can drive.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';

vi.hoisted(() => {
  (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'detached-logs' } }
  };
});

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn(async () => null) }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
  emitTo: vi.fn(async () => {})
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ label: 'detached-logs' })
}));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => true),
  requestPermission: vi.fn(async () => 'granted'),
  sendNotification: vi.fn(async () => {})
}));

import Shell from '../src/routes/+layout.svelte';

/** The `children` snippet SvelteKit passes a layout for its page content. */
function pageContent(marker: string) {
  return createRawSnippet(() => ({
    render: () => `<p class="route-body">${marker}</p>`
  }));
}

beforeEach(() => invoke.mockClear());
afterEach(() => cleanup());

describe('#779 the shell renders route content through children', () => {
  it('renders the page body inside the shell chrome', () => {
    const { container } = render(Shell, { props: { children: pageContent('dashboard-view') } });

    expect(container.querySelector('.route-body')?.textContent).toBe('dashboard-view');
    // Shell chrome that must survive the swap: the skip link sits here, and
    // it only makes sense when page content is actually rendered under it.
    expect(container.querySelector('a.skip-link')).not.toBeNull();
  });

  it('swaps one route body for another on navigation', () => {
    const first = render(Shell, { props: { children: pageContent('dashboard-view') } });
    expect(first.container.querySelector('.route-body')?.textContent).toBe('dashboard-view');
    cleanup();

    // SvelteKit re-renders the same layout instance per navigation and hands
    // it the new page as a fresh snippet; the shell must show only the new
    // body, never both and never a stale one.
    const second = render(Shell, { props: { children: pageContent('settings-view') } });
    const bodies = [...second.container.querySelectorAll('.route-body')].map((n) => n.textContent);
    expect(bodies).toEqual(['settings-view']);
  });
});
