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
    expect(invokeMock.mock.calls.some(([cmd]) => cmd.startsWith('reconnect_spotify'))).toBe(false);
  });

  it('re-authorizes from the main window without clearing stored credentials (#554)', async () => {
    const { getByRole } = await mountSettings(false);

    await fireEvent.click(getByRole('button', { name: t('settings.reconnectSpotify') }));

    await waitFor(() =>
      expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'reconnect_spotify_session')).toBe(true)
    );
    // `reconnect_spotify` wipes the tokens *and* the keychain client_secret.
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'reconnect_spotify')).toBe(false);
    const call = invokeMock.mock.calls.find(([cmd]) => cmd === 'reconnect_spotify_session');
    expect(call?.[1]).toEqual({
      clientId: 'test-client-id',
      redirectUri: 'presencejam://callback'
    });
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

/**
 * Findings #634/#635/#637 + issue #538: the 4.6 Settings controls.
 *
 * Every one of these fields existed in `config.json` (or in Rust) with no
 * editor at all, so a user could only reach them by hand-editing the file:
 * the quiet-hours replacement status, the custom profanity lexicon, the
 * paused-backoff ceiling, the manual-status policy, the out-of-office gate and
 * the per-rule presence action. They are exercised through the real component,
 * with the same clamps the backend applies (`clamp_teams` /
 * `clamp_polling` / `clamp_rules`).
 *
 * Fails pre-fix: none of these controls renders, so every `querySelector`
 * below returns null.
 */
describe('Settings presence-rules and orphaned-field controls (#538/#634/#635/#637)', () => {
  it('renders the manual-status and out-of-office policies, off/on by default', async () => {
    const { container } = await mountSettings();

    const manual = container.querySelector('#respect-manual-status') as HTMLInputElement;
    const ooo = container.querySelector('#gate-out-of-office') as HTMLInputElement;
    expect(manual).not.toBeNull();
    expect(ooo).not.toBeNull();
    // Mirrors the Rust serde defaults: manual status respected, OOO opt-in.
    expect(manual.checked).toBe(true);
    expect(ooo.checked).toBe(false);

    await fireEvent.click(ooo);
    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);
    await waitFor(() => expect(get(configStore).teams.gate_when_out_of_office).toBe(true));
  });

  it('posts the quiet-hours replacement text and its presence action', async () => {
    const { container } = await mountSettings();

    const addQuiet = [...container.querySelectorAll('.btn-secondary')].find(
      (b) => b.textContent?.trim() === t('rules.addQuietHours')
    ) as HTMLButtonElement;
    await fireEvent.click(addQuiet);
    await tick();

    const replacement = container.querySelector(
      'input[aria-label="' + t('rules.quietReplacementPlaceholder') + '"]'
    ) as HTMLInputElement;
    expect(replacement).not.toBeNull();
    await fireEvent.input(replacement, { target: { value: '🌙 Back at 09:00' } });

    const presence = container.querySelector(
      'select[aria-label="' + t('rules.presenceLabel') + '"]'
    ) as HTMLSelectElement;
    expect(presence).not.toBeNull();
    // Only the five documented setPresence combinations are offered — the
    // "don't change my presence" option plus the closed set.
    expect(presence.querySelectorAll('option').length).toBe(6);
    await fireEvent.change(presence, { target: { value: 'Away|Away' } });

    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);
    await waitFor(() => {
      const entry = get(configStore).status_rules.quiet_hours[0];
      expect(entry.replacement_status).toBe('🌙 Back at 09:00');
      expect(entry.presence_availability).toBe('Away');
      expect(entry.presence_activity).toBe('Away');
    });
  });

  it('bounds the custom lexicon to the same 64x32 the backend enforces', async () => {
    const { container } = await mountSettings();

    const area = container.querySelector('#profanity-extra-words') as HTMLTextAreaElement;
    expect(area).not.toBeNull();
    const longWord = 'x'.repeat(40);
    const many = [...Array(70).keys()].map((i) => `word${i}`);
    await fireEvent.input(area, { target: { value: [longWord, ...many].join('\n') } });
    await tick();

    // The clamp hint tells the user what will actually be matched.
    expect(container.querySelector('.clamp-hint')?.textContent).toContain('64');

    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);
    await waitFor(() => {
      const words = get(configStore).teams.profanity_extra_words;
      expect(words.length).toBe(64);
      expect(words[0].length).toBe(32);
    });
  });

  it('bounds the paused-backoff ceiling to the backend clamp range', async () => {
    const { container } = await mountSettings();

    const input = container.querySelector('#pause-backoff-max') as HTMLInputElement;
    expect(input).not.toBeNull();
    // The native bounds mirror clamp_polling's 60..=3600.
    expect(input.min).toBe('60');
    expect(input.max).toBe('3600');

    await fireEvent.input(input, { target: { value: '99999' } });
    await tick();
    // Out-of-range input surfaces the effective value instead of silently
    // storing something the backend will rewrite.
    // The hint names the effective range/value the backend will store (the
    // test harness's `save_config` echo is an identity, so the clamp itself is
    // pinned by the Rust test `test_clamp_polling_...`).
    const hint = container.querySelector('.clamp-hint')?.textContent ?? '';
    expect(hint).toContain('3');
    expect(hint).toContain('60');
  });
});
