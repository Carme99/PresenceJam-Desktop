/**
 * S9 (issue #677) — the Dashboard's snooze chip.
 *
 * The chip is the only user-visible surface of the tray snooze inside the
 * window, and it has exactly two jobs the acceptance names:
 *
 *   - it mirrors the PERSISTED deadline (`configStore.snooze_until`) and counts
 *     it down, and
 *   - its **Resume now** button clears the deadline through the same
 *     whole-document config write the rest of the app uses, so the poller sees
 *     the clear on its next iteration.
 *
 * Three properties are pinned here: a live deadline renders a countdown plus the
 * button; an ALREADY PASSED deadline renders nothing (the derived state is the
 * instant, not the field — otherwise a stale store would keep a dead snooze on
 * screen); and a mount re-reads the config, which is how a snooze started from
 * the tray reaches a Dashboard that was unmounted at the time.
 *
 * Verified by reverting the chip block in `Dashboard.svelte` and re-running this
 * file: the two positive assertions fail.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import type { Mock } from 'vitest';

type Listener = { event: string; fn: (e: { payload: unknown }) => void };
const listeners: Listener[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, fn: (e: unknown) => void) => {
    const entry = { event, fn: fn as Listener['fn'] };
    listeners.push(entry);
    return () => {
      const i = listeners.indexOf(entry);
      if (i >= 0) listeners.splice(i, 1);
    };
  })
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: vi.fn(() => ({ label: 'main' })),
  WebviewWindow: vi.fn()
}));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(async () => '4.6.0') }));
vi.mock('@tauri-apps/plugin-updater', () => ({ check: vi.fn(async () => null) }));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => false),
  requestPermission: vi.fn(async () => 'denied'),
  sendNotification: vi.fn()
}));
vi.mock('$lib/stores/detach', async () => {
  // A hoisted `vi.mock` factory cannot use a static import (the module under
  // test imports this store at collection time), so `svelte/store` is loaded
  // dynamically here — the same exception `tests/dashboard.test.ts` documents.
  const { writable } = await import('svelte/store');
  return {
    detachedPanes: writable({ logs: false, settings: false }),
    focusDetached: vi.fn(async () => {}),
    popOut: vi.fn(async () => {}),
    popIn: vi.fn(async () => {}),
    reconcileDetachedPanes: vi.fn(async () => {})
  };
});

import { invoke } from '@tauri-apps/api/core';
import Dashboard from '$lib/components/Dashboard.svelte';
import { configStore, defaultConfig } from '$lib/stores/config';
import type { AppConfig } from '$lib/types';
import { i18n } from '$lib/i18n';

const invokeMock = invoke as unknown as Mock;

const CONNECTED_STATUS = {
  is_syncing: true,
  spotify_connected: true,
  teams_connected: true,
  current_track: null
};

/** A config carrying the deadline (RFC3339 UTC, as the tray writes it). */
function configWith(snoozeUntil: string | null): AppConfig {
  return { ...structuredClone(defaultConfig), snooze_until: snoozeUntil } as AppConfig;
}

/** RFC3339 UTC for `seconds` from now. */
function deadlineIn(seconds: number): string {
  return new Date(Date.now() + seconds * 1000).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

beforeEach(() => {
  listeners.length = 0;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'load_config':
        return configWith(currentStore().snooze_until);
      case 'get_sync_status':
        return CONNECTED_STATUS;
      case 'save_config':
        return configWith(null);
      default:
        return undefined;
    }
  });
  configStore.set(configWith(null));
});

afterEach(() => {
  cleanup();
});

function currentStore(): AppConfig {
  let value = configWith(null);
  const unsub = configStore.subscribe((v) => {
    value = v;
  });
  unsub();
  return value;
}

describe('Dashboard snooze chip (S9 / #677)', () => {
  it('shows a live countdown and a Resume button for a stored deadline', async () => {
    configStore.set(configWith(deadlineIn(30 * 60)));
    const { container } = render(Dashboard as never);

    await waitFor(() => {
      expect(container.querySelector('.snooze-chip')).not.toBeNull();
    });
    const chip = container.querySelector('.snooze-chip') as HTMLElement;
    // 30 minutes minus the (sub-second) render delay, so the minute/second pair
    // is asserted with a one-tick tolerance.
    expect(chip.textContent).toMatch(/(30:00|29:59)/);
    expect(chip.textContent).toContain('left');
    const button = chip.querySelector('.snooze-resume') as HTMLButtonElement;
    expect(button).not.toBeNull();
    expect(button.textContent?.trim()).toBe('Resume now');
  });

  it('renders no chip for a deadline that has already passed', async () => {
    configStore.set(configWith(deadlineIn(-60)));
    const { container } = render(Dashboard as never);

    // Wait for the mount's own work to settle, so the assertion cannot pass
    // merely because the component has not rendered yet.
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('get_sync_status');
    });
    expect(container.querySelector('.snooze-chip')).toBeNull();
  });

  it('Resume clears the deadline through the shared config write', async () => {
    configStore.set(configWith(deadlineIn(30 * 60)));
    const { container } = render(Dashboard as never);
    await waitFor(() => {
      expect(container.querySelector('.snooze-resume')).not.toBeNull();
    });

    (container.querySelector('.snooze-resume') as HTMLButtonElement).click();

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'save_config',
        expect.objectContaining({
          config: expect.objectContaining({ snooze_until: null })
        })
      );
    });
    // The store follows the persisted answer, so the chip goes away without a
    // reload and the poller's next iteration reads the cleared field.
    await waitFor(() => {
      expect(currentStore().snooze_until).toBeNull();
      expect(container.querySelector('.snooze-chip')).toBeNull();
    });
  });

  it('re-reads the config on mount, so a tray snooze reaches a fresh mount', async () => {
    // Nothing in the store, but the tray has written a snooze to disk.
    configStore.set(configWith(null));
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'load_config') return configWith(deadlineIn(60 * 60));
      if (cmd === 'get_sync_status') return CONNECTED_STATUS;
      return undefined;
    });

    const { container } = render(Dashboard as never);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('load_config');
    });
    await waitFor(() => {
      expect(container.querySelector('.snooze-chip')).not.toBeNull();
    });
  });

  it('counts a deadline past an hour down as h:mm:ss', async () => {
    configStore.set(configWith(deadlineIn(2 * 60 * 60 + 5)));
    const { container } = render(Dashboard as never);
    await waitFor(() => {
      expect(container.querySelector('.snooze-chip')).not.toBeNull();
    });
    // The `hours > 0` arm of the formatter: `h:mm:ss`, not `mm:ss`. The stored
    // deadline is second-precision, so a couple of seconds of render slack is
    // expected — hence the tolerant seconds pair.
    expect(container.querySelector('.snooze-chip')?.textContent).toMatch(/2:00:0[0-5]/);
  });

  it('ignores an unparsable stored deadline instead of guessing one', async () => {
    // A hand-edited config can carry anything; `Date.parse` failing must read as
    // "not snoozed", never as a zero or NaN countdown.
    configStore.set(configWith('not a timestamp'));
    const { container } = render(Dashboard as never);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('get_sync_status');
    });
    expect(container.querySelector('.snooze-chip')).toBeNull();
  });

  it('keeps the chip and reports the failure when Resume cannot be saved', async () => {
    configStore.set(configWith(deadlineIn(30 * 60)));
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'load_config') return configWith(deadlineIn(30 * 60));
      if (cmd === 'get_sync_status') return CONNECTED_STATUS;
      if (cmd === 'save_config') throw new Error('disk full');
      return undefined;
    });

    const { container } = render(Dashboard as never);
    await waitFor(() => {
      expect(container.querySelector('.snooze-resume')).not.toBeNull();
    });
    (container.querySelector('.snooze-resume') as HTMLButtonElement).click();

    // The write did not happen, so the snooze stands and the user is told —
    // claiming success would leave the poller asleep with no way back.
    await waitFor(() => {
      expect(container.querySelector('.error-banner')?.textContent).toContain(
        'Could not resume syncing'
      );
    });
    expect(container.querySelector('.snooze-chip')).not.toBeNull();
    expect(currentStore().snooze_until).not.toBeNull();
  });

  /**
   * #969 — the chip's clock must follow `config.locale`. The pre-fix call
   * passed `undefined` as the locale, so a French or German user saw the
   * OS clock format; this case asserts the chip re-renders when the locale
   * flips. Two different locales produce two different renderings of the
   * same wall-clock instant.
   */
  it('renders the chip clock in the active locale, not the OS default (#969)', async () => {
    i18n.set('en');
    configStore.set(configWith(deadlineIn(30 * 60)));
    const { container } = render(Dashboard as never);
    await waitFor(() => {
      expect(container.querySelector('.snooze-chip')).not.toBeNull();
    });
    const enText = (container.querySelector('.snooze-chip') as HTMLElement).textContent ?? '';

    i18n.set('fr');
    await tick();
    // Reactive: `snoozeLabel` is a `$derived` that reads `i18n.locale`, so a
    // locale flip re-renders the chip without remounting.
    await waitFor(() => {
      const next = (container.querySelector('.snooze-chip') as HTMLElement).textContent ?? '';
      expect(next).not.toBe(enText);
    });
    i18n.set('en');
  });
});
