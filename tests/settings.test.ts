/**
 * #548 / #549 / #550 / #552 — Settings navigation, notifications and the
 * theme picker, exercised through the real component.
 *
 *   - unsaved edits gate Back instead of being silently discarded (#548);
 *   - a denied OS permission must not leave the notification toggle on (#549);
 *   - a detached pane forwards reconnect navigation to the main window and
 *     never drives `reconnect_*` itself (#550);
 *   - the theme cards are a real radiogroup with arrow-key navigation (#552).
 *
 * Fails pre-fix: Back navigates away with the edits dropped, the toggle stays
 * checked after a denial, the detached pane invokes `reconnect_teams`, and the
 * cards expose `aria-pressed` inside a `radiogroup`.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, waitFor } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { tick } from 'svelte';
import type { Mock } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
  emitTo: vi.fn(async () => {})
}));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => true),
  requestPermission: vi.fn(async () => 'granted')
}));
vi.mock('$lib/stores/detach', () => ({
  popOut: vi.fn(async () => {}),
  popIn: vi.fn(async () => {}),
  focusDetached: vi.fn(async () => {})
}));

import { invoke } from '@tauri-apps/api/core';
import { emitTo } from '@tauri-apps/api/event';
import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification';
import { popIn } from '$lib/stores/detach';
import Settings from '$lib/components/Settings.svelte';
import { currentView } from '$lib/stores/app';
import { configStore, defaultConfig } from '$lib/stores/config';
import { notificationsEnabled } from '$lib/stores/notifications';
import { resetSpotifyAuthFlow, resetTeamsAuthFlow } from '$lib/stores/authFlow.svelte';
import { theme } from '$lib/stores/theme';
import { t } from '$lib/i18n';

const invokeMock = invoke as unknown as Mock;
const emitToMock = emitTo as unknown as Mock;
const popInMock = popIn as unknown as Mock;
const isPermissionGrantedMock = isPermissionGranted as unknown as Mock;
const requestPermissionMock = requestPermission as unknown as Mock;

/** A configured install: the Spotify reconnect button needs a stored Client ID. */
function configuredConfig() {
  const cfg = structuredClone(defaultConfig);
  cfg.spotify.client_id = 'test-client-id';
  return cfg;
}

/**
 * Mount Settings and wait until `onMount` has adopted the loaded config: the
 * connected badge only renders once `get_sync_status` has answered, so the
 * form cannot be overwritten underneath the test.
 */
async function mountSettings(detached = false) {
  const result = render(Settings, { detached });
  await waitFor(() => {
    expect(result.container.querySelector('.badge.success')).not.toBeNull();
  });
  return result;
}

const backButton = (container: HTMLElement) =>
  container.querySelector('.back-btn') as HTMLButtonElement;

const formatInput = (container: HTMLElement) =>
  container.querySelector('#status-format') as HTMLInputElement;

beforeEach(() => {
  currentView.set('dashboard');
  configStore.set(configuredConfig());
  theme.set('dark');
  resetSpotifyAuthFlow();
  resetTeamsAuthFlow();
  isPermissionGrantedMock.mockResolvedValue(true);
  requestPermissionMock.mockResolvedValue('granted');
  emitToMock.mockClear();
  popInMock.mockClear();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    switch (cmd) {
      case 'load_config':
        return configuredConfig();
      case 'save_config':
        // The backend echoes the copy it persisted (#297).
        return args != null && typeof args === 'object' && 'config' in args ? args.config : undefined;
      case 'get_sync_status':
        return {
          is_syncing: false,
          current_track: null,
          spotify_connected: true,
          teams_connected: true
        };
      case 'get_spotify_granted_scopes':
        return ['user-modify-playback-state'];
      case 'get_teams_granted_scopes':
        return ['Presence.Read', 'profile'];
      default:
        return [];
    }
  });
});

afterEach(() => {
  cleanup();
});

describe('Settings dirty-navigation gate (#548)', () => {
  it('keeps the user on the form when Back is pressed with unsaved edits', async () => {
    const { container, getByRole } = await mountSettings();
    currentView.set('settings');
    expect(container.querySelector('.dirty-banner')).toBeNull();

    await fireEvent.input(formatInput(container), { target: { value: '🎧 {track}' } });
    await tick();
    expect(container.querySelector('.dirty-banner')).not.toBeNull();

    await fireEvent.click(backButton(container));
    // No navigation until the banner's Save / Discard / Stay choice is made.
    expect(get(currentView)).toBe('settings');
    expect(getByRole('button', { name: t('settings.stayHere') })).toBeTruthy();

    await fireEvent.click(getByRole('button', { name: t('settings.stayHere') }));
    await tick();
    expect(get(currentView)).toBe('settings');
    expect(container.querySelector('.dirty-banner')).not.toBeNull();
  });

  it('discards the edits and navigates once the user confirms', async () => {
    const { container, getByRole } = await mountSettings();
    currentView.set('settings');

    await fireEvent.input(formatInput(container), { target: { value: '🎧 {track}' } });
    await fireEvent.click(backButton(container));
    await fireEvent.click(getByRole('button', { name: t('settings.discardChanges') }));

    await waitFor(() => expect(get(currentView)).toBe('dashboard'));
  });

  it('saves first and then navigates', async () => {
    const { container, getByRole } = await mountSettings();
    currentView.set('settings');

    await fireEvent.input(formatInput(container), { target: { value: '🎧 {track}' } });
    await fireEvent.click(backButton(container));
    await fireEvent.click(getByRole('button', { name: t('settings.saveAndLeave') }));

    await waitFor(() => expect(get(currentView)).toBe('dashboard'));
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'save_config')).toBe(true);
  });

  it('navigates straight through when nothing is dirty', async () => {
    const { container } = await mountSettings();
    currentView.set('settings');

    await fireEvent.click(backButton(container));
    expect(get(currentView)).toBe('dashboard');
    expect(container.querySelector('.dirty-banner')).toBeNull();
  });
});

describe('Settings detached reconnect guard (#550)', () => {
  it('forwards a detached Teams reconnect to the main window instead of invoking it', async () => {
    const { getByRole } = await mountSettings(true);

    await fireEvent.click(getByRole('button', { name: t('reconnect.reconnectTeams') }));

    await waitFor(() => expect(emitToMock).toHaveBeenCalledWith('main', 'navigate', 'settings'));
    expect(popInMock).toHaveBeenCalledWith('settings');
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'reconnect_teams')).toBe(false);
  });

  it('forwards a detached Spotify reconnect the same way', async () => {
    const { getByRole } = await mountSettings(true);

    await fireEvent.click(getByRole('button', { name: t('settings.reconnectSpotify') }));

    await waitFor(() => expect(emitToMock).toHaveBeenCalledWith('main', 'navigate', 'settings'));
    expect(popInMock).toHaveBeenCalledWith('settings');
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'reconnect_spotify')).toBe(false);
  });

  it('still drives the reconnect command from the main window', async () => {
    const { getByRole } = await mountSettings(false);

    await fireEvent.click(getByRole('button', { name: t('reconnect.reconnectTeams') }));

    await waitFor(() =>
      expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'reconnect_teams')).toBe(true)
    );
    expect(emitToMock).not.toHaveBeenCalled();
  });
});

describe('Settings notification opt-in (#549)', () => {
  it('leaves the toggle off and explains when permission is denied', async () => {
    isPermissionGrantedMock.mockResolvedValue(false);
    requestPermissionMock.mockResolvedValue('denied');
    const { container } = await mountSettings();
    const box = container.querySelector('#notifications-enabled') as HTMLInputElement;

    await fireEvent.click(box);
    await waitFor(() =>
      expect(container.querySelector('.error-message')?.textContent).toBe(
        t('settings.notificationsDenied')
      )
    );

    expect(box.checked).toBe(false);
    expect(get(notificationsEnabled)).toBe(false);
    expect(localStorage.getItem('notificationsEnabled')).toBe('false');
  });

  it('records the opt-in when permission is granted', async () => {
    const { container } = await mountSettings();
    const box = container.querySelector('#notifications-enabled') as HTMLInputElement;

    await fireEvent.click(box);
    await waitFor(() => expect(get(notificationsEnabled)).toBe(true));
    expect(localStorage.getItem('notificationsEnabled')).toBe('true');
  });
});

describe('Settings theme picker semantics (#552)', () => {
  it('exposes two radios with aria-checked, a roving tabindex and arrow keys', async () => {
    const { container } = await mountSettings();
    await waitFor(() => expect(container.querySelectorAll('[role="radio"]').length).toBe(2));

    const radios = [...container.querySelectorAll<HTMLElement>('[role="radio"]')];
    const [dark, light] = radios;
    expect(dark.getAttribute('aria-checked')).toBe('true');
    expect(light.getAttribute('aria-checked')).toBe('false');
    expect(dark.getAttribute('aria-pressed')).toBeNull();
    expect(radios.filter((r) => r.getAttribute('tabindex') === '0').length).toBe(1);

    await fireEvent.keyDown(dark, { key: 'ArrowRight' });
    await tick();
    expect(get(theme)).toBe('light');
    expect(light.getAttribute('aria-checked')).toBe('true');
    expect(dark.getAttribute('aria-checked')).toBe('false');
    expect(document.activeElement).toBe(light);
  });
});
