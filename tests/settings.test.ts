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
import { notificationPreferences } from '$lib/stores/notifications';
import { authFlow, resetSpotifyAuthFlow, resetTeamsAuthFlow, setSpotifyPhase, setTeamsPhase } from '$lib/stores/authFlow.svelte';
import { theme } from '$lib/stores/theme';
import { t, i18n, type TKey, type Locale } from '$lib/i18n';
// #955: the presence dropdown's labels are dictionary entries, so the test
// reads the three dictionaries the app ships rather than restating the copy.
import { en, type Dict } from '$lib/i18n/en';
import { de } from '$lib/i18n/de';
import { fr } from '$lib/i18n/fr';

// #693: the persist warning is captured in the always-mounted layout listener
// and rendered from the shared presence store, so the test drives the store,
// not an event this pane no longer subscribes to.
import { presence, INITIAL_PRESENCE, markAuthPersistWarning } from '$lib/stores/presence';

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

/** A configured install with every notification class off; tests opt in. */
function notificationsOffConfig() {
  const cfg = configuredConfig();
  cfg.notifications = {
    track_change: false,
    sync_stopped: false,
    auth_required: false,
    update_staged: false
  };
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
  presence.set({ ...INITIAL_PRESENCE });
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
        // The store's current value, not a fresh default: a test that seeds a
        // config before mounting must get that config back (#675).
        return get(configStore);
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

/**
 * #549's rule, now one toggle per class (#675): the OS prompt's answer
 * decides whether a class may notify, a denial leaves the toggle off and
 * says why, and a granted opt-in is persisted into the config — the
 * `localStorage` boolean the pre-4.7 card wrote is gone.
 */
describe('Settings notification classes (#549, #675)', () => {
  const classToggle = (container: HTMLElement, cls: string) =>
    container.querySelector(`#notifications-${cls}`) as HTMLInputElement;

  it('renders one toggle per class', async () => {
    const { container } = await mountSettings();

    for (const cls of ['track_change', 'sync_stopped', 'auth_required', 'update_staged']) {
      expect(classToggle(container, cls)).not.toBeNull();
    }
    expect(classToggle(container, 'track_change').checked).toBe(true);
  });

  it('leaves the toggle off and explains when permission is denied', async () => {
    configStore.set(notificationsOffConfig());
    isPermissionGrantedMock.mockResolvedValue(false);
    requestPermissionMock.mockResolvedValue('denied');
    const { container } = await mountSettings();
    const box = classToggle(container, 'sync_stopped');

    await fireEvent.click(box);
    await waitFor(() =>
      expect(container.querySelector('.error-message')?.textContent).toBe(
        t('settings.notificationsDenied')
      )
    );

    expect(box.checked).toBe(false);
    expect(get(notificationPreferences).sync_stopped).toBe(false);
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'save_config')).toBe(false);
  });

  it('records the opt-in in the config when permission is granted', async () => {
    configStore.set(notificationsOffConfig());
    const { container } = await mountSettings();
    const box = classToggle(container, 'sync_stopped');

    await fireEvent.click(box);
    await waitFor(() => expect(get(notificationPreferences).sync_stopped).toBe(true));
    expect(get(configStore).notifications.sync_stopped).toBe(true);
    // The pre-4.7 key is not written any more — config.json owns the value.
    expect(localStorage.getItem('notificationsEnabled')).toBeNull();
  });

  /**
   * #675 regression: two Settings views genuinely coexist (C7 — a popped-out
   * pane opens beside the main window's own Settings). The form's
   * `localConfig` is a snapshot, so before the notifications section started
   * following the shared store, a Save in the *other* view wrote its stale
   * section back and un-ticked the class the user had just turned on — a
   * failure mode the pre-4.7 `localStorage` flag could not have, since no Save
   * touched it.
   */
  it('keeps a toggle made in the sibling Settings view when this view saves', async () => {
    configStore.set(notificationsOffConfig());
    const main = await mountSettings();
    const sibling = await mountSettings(true);

    await fireEvent.click(classToggle(sibling.container, 'sync_stopped'));
    await waitFor(() => expect(get(notificationPreferences).sync_stopped).toBe(true));

    // The main view's form was loaded before that toggle and is not dirty for
    // the class, so its Save now has to carry the choice the sibling made.
    const savesBefore = invokeMock.mock.calls.filter(([cmd]) => cmd === 'save_config').length;
    await fireEvent.click(main.container.querySelector('.actions .btn-full') as HTMLElement);
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter(([cmd]) => cmd === 'save_config').length).toBeGreaterThan(
        savesBefore
      )
    );

    const saved = invokeMock.mock.calls.filter(([cmd]) => cmd === 'save_config').at(-1)?.[1] as {
      config: { notifications: Record<string, boolean> };
    };
    expect(saved.config.notifications.sync_stopped).toBe(true);
  });
});

describe('Settings theme picker semantics (#552, #680)', () => {
  it('exposes three radios with aria-checked, a roving tabindex and arrow keys', async () => {
    const { container } = await mountSettings();
    await waitFor(() => expect(container.querySelectorAll('[role="radio"]').length).toBe(3));

    const radios = [...container.querySelectorAll<HTMLElement>('[role="radio"]')];
    const [dark, light, system] = radios;
    expect(dark.getAttribute('aria-checked')).toBe('true');
    expect(light.getAttribute('aria-checked')).toBe('false');
    expect(system.getAttribute('aria-checked')).toBe('false');
    expect(dark.getAttribute('aria-pressed')).toBeNull();
    expect(radios.filter((r) => r.getAttribute('tabindex') === '0').length).toBe(1);

    await fireEvent.keyDown(dark, { key: 'ArrowRight' });
    await tick();
    expect(get(theme)).toBe('light');
    expect(light.getAttribute('aria-checked')).toBe('true');
    expect(dark.getAttribute('aria-checked')).toBe('false');
    expect(document.activeElement).toBe(light);

    // #680: the third card is reachable by the same walk, and wraps around.
    await fireEvent.keyDown(light, { key: 'ArrowRight' });
    await tick();
    expect(get(theme)).toBe('system');
    expect(system.getAttribute('aria-checked')).toBe('true');
    expect(document.activeElement).toBe(system);

    await fireEvent.keyDown(system, { key: 'ArrowLeft' });
    await tick();
    expect(get(theme)).toBe('light');
    await fireEvent.keyDown(light, { key: 'ArrowLeft' });
    await tick();
    expect(get(theme)).toBe('dark');
  });

  it('selects the system preference and toggles compact density (#680)', async () => {
    const { container } = await mountSettings();
    await waitFor(() => expect(container.querySelectorAll('[role="radio"]').length).toBe(3));

    const radios = [...container.querySelectorAll<HTMLElement>('[role="radio"]')];
    await fireEvent.click(radios[2]);
    await tick();
    expect(get(theme)).toBe('system');
    // jsdom has no matchMedia here, so `system` resolves to the dark default.
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');

    const compact = container.querySelector('#compact-density') as HTMLInputElement;
    expect(compact).not.toBeNull();
    expect(compact.checked).toBe(false);

    await fireEvent.click(compact);
    await tick();
    expect(document.documentElement.getAttribute('data-density')).toBe('compact');
    expect(localStorage.getItem('presencejam:density')).toBe('compact');

    await fireEvent.click(compact);
    await tick();
    expect(document.documentElement.getAttribute('data-density')).toBe('comfortable');
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
      'input[aria-label="' + t('rules.replacementPlaceholder') + '"]'
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

/**
 * Issue #538 follow-up: the custom lexicon must reach the MATCHER, not just
 * the store. The Settings preview is the one call site where that is
 * observable without a playing track, so it pins the wiring: `preview_status`
 * receives the same (clamped) list the save path persists.
 *
 * Fails pre-fix: the preview invoked `preview_status` without `extra_words`,
 * so a word the user just added showed no effect in the preview.
 */
describe('Settings custom lexicon reaches the profanity matcher (#538)', () => {
  it('passes the clamped extra words to preview_status', async () => {
    const { container } = await mountSettings();

    const area = container.querySelector('#profanity-extra-words') as HTMLTextAreaElement;
    expect(area).not.toBeNull();
    await fireEvent.input(area, { target: { value: 'flurble\nbadword' } });

    await waitFor(() => {
      const call = invokeMock.mock.calls.find(
        ([cmd, args]) =>
          cmd === 'preview_status' &&
          (args as { extra_words?: unknown })?.extra_words instanceof Array
      );
      expect(call).toBeTruthy();
      expect((call![1] as { extra_words: string[] }).extra_words).toEqual([
        'flurble',
        'badword'
      ]);
    });
  });
});

/**
 * #693 — a Teams sign-in whose tokens could not be persisted (locked
 * keychain, full disk) used to fail silently: `poll_teams_auth` kept the
 * session in memory and emitted `teams-auth-persist-warning`, but nothing
 * listened, so the user only found out when the session was gone after a
 * restart. The always-mounted layout listener captures it into the shared
 * presence store (S2's half); this pane renders it and offers the retry.
 *
 * Fails pre-fix: nothing stored or rendered the warning, so no banner
 * appeared and no retry path existed.
 */
describe('Settings Teams persistence warning (#693)', () => {
  it('renders the captured warning and clears it from the reconnect that retries the save', async () => {
    markAuthPersistWarning('tokens could not be written');
    const { container } = await mountSettings();

    const banner = container.querySelector('.persist-banner');
    expect(banner).not.toBeNull();
    expect(banner?.textContent).toContain(t('settings.teamsPersistWarning'));
    const reconnect = banner?.querySelector('button');
    expect(reconnect?.textContent).toContain(t('common.reconnect'));

    // The retry path: reconnecting re-runs the sign-in (and its persist), and
    // the warning clears so a success leaves no banner behind.
    await fireEvent.click(reconnect as HTMLButtonElement);
    await waitFor(() => expect(get(presence).authPersistWarning).toBeNull());
    expect(container.querySelector('.persist-banner')).toBeNull();
    await waitFor(() =>
      expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'reconnect_teams')).toBe(true)
    );
  });

  it('is dismissable without a reconnect, so an unfixable cause is not a permanent banner', async () => {
    markAuthPersistWarning('keychain is locked');
    const { container } = await mountSettings();
    const banner = container.querySelector('.persist-banner');
    expect(banner).not.toBeNull();

    const dismiss = banner?.querySelector('button.dismiss');
    expect(dismiss?.textContent).toContain(t('common.dismiss'));
    await fireEvent.click(dismiss as HTMLButtonElement);

    await waitFor(() => expect(get(presence).authPersistWarning).toBeNull());
    expect(container.querySelector('.persist-banner')).toBeNull();
    // Dismissing is not a retry: it must not re-run the sign-in.
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'reconnect_teams')).toBe(false);
  });

  it('shows no banner while the store holds no persistence fault', async () => {
    const { container } = await mountSettings();
    expect(container.querySelector('.persist-banner')).toBeNull();
  });
});


/**
 * S4 (issue #672): the rules card's scheduling, priority and manual-status
 * controls, exercised through the real component.
 *
 * Fails pre-fix: a track-rule row has no window inputs, no weekday picker and
 * no reorder controls, quiet hours have no pause-polling toggle, and the two
 * manual-status texts have no editor — so every `querySelector` below is null
 * and a saved rule carries no schedule.
 */
describe('Settings rule scheduling and priority controls (S4/#672)', () => {
  it('saves a track rule window and weekday set', async () => {
    const { container } = await mountSettings();

    const addRule = [...container.querySelectorAll('.btn-secondary')].find(
      (b) => b.textContent?.trim() === t('rules.addTrackRule')
    ) as HTMLButtonElement;
    await fireEvent.click(addRule);
    await tick();

    const group = container.querySelector(
      '[role="group"][aria-label="' + t('rules.trackRulesLabel') + ' 1"]'
    ) as HTMLElement;
    expect(group).not.toBeNull();
    const start = group.querySelector(
      'input[aria-label="' + t('rules.ruleStart') + '"]'
    ) as HTMLInputElement;
    const end = group.querySelector(
      'input[aria-label="' + t('rules.ruleEnd') + '"]'
    ) as HTMLInputElement;
    // The new rule starts as the config default — every day, the whole day
    // (the 1440 end renders as midnight).
    expect(start.value).toBe('00:00');
    expect(end.value).toBe('00:00');

    await fireEvent.change(start, { target: { value: '08:00' } });
    await fireEvent.change(end, { target: { value: '17:00' } });
    const days = group.querySelector(
      '[role="group"][aria-label="' + t('rules.ruleDays') + '"]'
    ) as HTMLElement;
    // Index 0 is Monday.
    await fireEvent.click(days.querySelectorAll('input[type="checkbox"]')[0] as HTMLElement);
    await tick();

    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);
    await waitFor(() => {
      const rule = get(configStore).status_rules.track_rules[0];
      expect(rule.start_minutes).toBe(480);
      expect(rule.end_minutes).toBe(1020);
      expect(rule.days).toEqual([1]);
    });
  });

  it('treats a picked 00:00 end time as the end of the day, not an empty window', async () => {
    const { container } = await mountSettings();

    const addRule = [...container.querySelectorAll('.btn-secondary')].find(
      (b) => b.textContent?.trim() === t('rules.addTrackRule')
    ) as HTMLButtonElement;
    await fireEvent.click(addRule);
    await tick();

    const end = container.querySelector(
      'input[aria-label="' + t('rules.ruleEnd') + '"]'
    ) as HTMLInputElement;
    await fireEvent.change(end, { target: { value: '23:00' } });
    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);
    await waitFor(() => {
      expect(get(configStore).status_rules.track_rules[0].end_minutes).toBe(1380);
    });

    // 00:00 is midnight — the same instant as the config's 1440 end of day, not
    // an empty 0..0 window that would silently never match.
    await fireEvent.change(end, { target: { value: '00:00' } });
    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);
    await waitFor(() => {
      expect(get(configStore).status_rules.track_rules[0].end_minutes).toBe(1440);
    });
  });

  it('reorders rules, because the first match wins', async () => {
    const { container } = await mountSettings();

    const addRule = [...container.querySelectorAll('.btn-secondary')].find(
      (b) => b.textContent?.trim() === t('rules.addTrackRule')
    ) as HTMLButtonElement;
    await fireEvent.click(addRule);
    await fireEvent.click(addRule);
    await tick();

    const artists = [
      ...container.querySelectorAll(
        'input[aria-label="' + t('rules.artistPlaceholder') + '"]'
      )
    ] as HTMLInputElement[];
    await fireEvent.input(artists[0], { target: { value: 'first' } });
    await fireEvent.input(artists[1], { target: { value: 'second' } });
    await tick();

    // The first row's "up" control is disabled; the second row moves up.
    const upFirst = container.querySelector(
      'button[aria-label="' + t('rules.moveRuleUp', { n: 1 }) + '"]'
    ) as HTMLButtonElement;
    expect(upFirst.disabled).toBe(true);
    await fireEvent.click(
      container.querySelector(
        'button[aria-label="' + t('rules.moveRuleUp', { n: 2 }) + '"]'
      ) as HTMLElement
    );
    await tick();

    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);
    await waitFor(() => {
      expect(
        get(configStore).status_rules.track_rules.map((r) => r.artist_substring)
      ).toEqual(['second', 'first']);
    });
  });

  it('resets the two manual-status editors the rules card renders', async () => {
    const { container } = await mountSettings();

    const pausedText = container.querySelector(
      'input[aria-label="' + t('rules.pausedStatusPlaceholder') + '"]'
    ) as HTMLInputElement;
    const stoppedText = container.querySelector(
      'input[aria-label="' + t('rules.stoppedStatusPlaceholder') + '"]'
    ) as HTMLInputElement;
    await fireEvent.input(pausedText, { target: { value: 'Back in 5' } });
    await fireEvent.input(stoppedText, { target: { value: 'Idle' } });
    await tick();

    // Scope to the rules card: three other cards carry a "reset to default"
    // control, and the first one in DOM order belongs to the presence card.
    const rulesCard = [...container.querySelectorAll('section.card')].find(
      (section) => section.querySelector('h2')?.textContent?.trim() === t('rules.sectionTitle')
    ) as HTMLElement;
    const reset = rulesCard.querySelector('button.btn-link') as HTMLButtonElement;
    await fireEvent.click(reset);
    await tick();

    // The visible editors are back to the defaults, not left stale.
    expect(pausedText.value).toBe(defaultConfig.teams.paused_status_format);
    expect(stoppedText.value).toBe(defaultConfig.teams.stopped_status_format);

    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);
    await waitFor(() => {
      expect(get(configStore).teams.paused_status_format).toBe(
        defaultConfig.teams.paused_status_format
      );
      expect(get(configStore).teams.stopped_status_format).toBe(
        defaultConfig.teams.stopped_status_format
      );
    });
  });

  it('treats a picked 00:00 end time as midnight on a quiet window too', async () => {
    const { container } = await mountSettings();

    const addQuiet = [...container.querySelectorAll('.btn-secondary')].find(
      (b) => b.textContent?.trim() === t('rules.addQuietHours')
    ) as HTMLButtonElement;
    await fireEvent.click(addQuiet);
    await tick();

    // The new entry defaults to 22:00–07:00; picking 00:00 for the end must save
    // midnight (= 1440, the end of the day) rather than 0, which would be an
    // empty window the user cannot tell from a broken rule.
    const start = container.querySelector(
      'input[aria-label="' + t('rules.quietStart') + '"]'
    ) as HTMLInputElement;
    const end = container.querySelector(
      'input[aria-label="' + t('rules.quietEnd') + '"]'
    ) as HTMLInputElement;
    await fireEvent.change(start, { target: { value: '00:00' } });
    await fireEvent.change(end, { target: { value: '00:00' } });
    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);

    await waitFor(() => {
      const entry = get(configStore).status_rules.quiet_hours[0];
      expect(entry.start_minutes).toBe(0);
      expect(entry.end_minutes).toBe(1440);
    });
  });

  it('saves the pause-polling toggle and the two manual-status texts', async () => {
    const { container } = await mountSettings();

    const addQuiet = [...container.querySelectorAll('.btn-secondary')].find(
      (b) => b.textContent?.trim() === t('rules.addQuietHours')
    ) as HTMLButtonElement;
    await fireEvent.click(addQuiet);
    await tick();

    const pauseLabel = [...container.querySelectorAll('label.rule-check')].find((l) =>
      l.textContent?.includes(t('rules.pausePollingLabel'))
    ) as HTMLElement;
    expect(pauseLabel).toBeTruthy();
    const pauseToggle = pauseLabel.querySelector('input[type="checkbox"]') as HTMLInputElement;
    expect(pauseToggle.checked).toBe(false);

    const pausedText = container.querySelector(
      'input[aria-label="' + t('rules.pausedStatusPlaceholder') + '"]'
    ) as HTMLInputElement;
    const stoppedText = container.querySelector(
      'input[aria-label="' + t('rules.stoppedStatusPlaceholder') + '"]'
    ) as HTMLInputElement;
    // The editors show the Rust serde defaults.
    expect(pausedText.value).toBe('Paused');
    expect(stoppedText.value).toBe('Nothing playing on Spotify');

    await fireEvent.click(pauseToggle);
    await fireEvent.input(pausedText, { target: { value: 'Back in 5' } });
    await fireEvent.input(stoppedText, { target: { value: 'Idle' } });
    await tick();

    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);
    await waitFor(() => {
      expect(get(configStore).status_rules.quiet_hours[0].pause_polling).toBe(true);
      expect(get(configStore).teams.paused_status_format).toBe('Back in 5');
      expect(get(configStore).teams.stopped_status_format).toBe('Idle');
    });
  });
});

/**
 * 4.7.0 (#673): the Logging and Backup cards.
 *
 * The Rust half is covered by `config.rs` tests (rotation clamps, the export
 * document, import validation); what only this test can pin is the seam —
 * the logging fields ride the normal `save_config` path, and the two backup
 * buttons reach the commands the Rust side registers, with the localized
 * dialog title as the only argument.
 */
describe('Settings logging and backup cards (#673)', () => {
  const buttonByText = (container: HTMLElement, label: string) =>
    [...container.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === label
    ) as HTMLButtonElement;

  /** Serve the backup commands; everything else keeps the harness default. */
  function armBackupCommands(exportPath: string | null, importPath: string | null) {
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'load_config') return configuredConfig();
      if (cmd === 'get_sync_status') {
        return {
          is_syncing: false,
          current_track: null,
          spotify_connected: true,
          teams_connected: true
        };
      }
      if (cmd === 'get_spotify_granted_scopes') return ['user-modify-playback-state'];
      if (cmd === 'get_teams_granted_scopes') return ['Presence.Read', 'profile'];
      if (cmd === 'export_config') return exportPath;
      if (cmd === 'import_config') {
        // `null` is the command's "declined or dismissed" answer (the
        // confirmation lives in Rust), so a null path here exercises the
        // decline branch rather than the success one.
        return importPath === null ? null : { path: importPath, config: configuredConfig() };
      }
      if (args != null && typeof args === 'object' && 'config' in args) return args.config;
      return [];
    });
  }

  it('saves the rotation fields through the normal save path', async () => {
    const { container } = await mountSettings();

    const keepInput = container.querySelector('#log-keep-files') as HTMLInputElement;
    expect(keepInput).not.toBeNull();
    await fireEvent.input(keepInput, { target: { value: '7' } });

    await fireEvent.click(buttonByText(container, t('settings.saveChanges')));

    await waitFor(() => {
      expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'save_config')).toBe(true);
      // The harness echoes the payload back and Settings adopts it (#297), so
      // the store holding the typed value proves the card's binding is live —
      // the u64 wire encoding is pinned in tests/stores.test.ts, where the
      // payload shape is the subject rather than a harness artefact.
      expect(get(configStore).logging.keep_files).toBe(7);
    });
  });

  it('exports through export_config and reports the resolved path', async () => {
    const { container } = await mountSettings();
    const exportPath = '/tmp/presencejam-config-4.7.0-20260917-040506.json';
    armBackupCommands(exportPath, '/tmp/imported.json');

    await fireEvent.click(buttonByText(container, t('settings.backupExport')));

    await waitFor(() => {
      const call = invokeMock.mock.calls.find(([cmd]) => cmd === 'export_config');
      expect(call).toBeTruthy();
      // The dialog title is the localized string: Rust renders the native
      // dialog and has no dictionary of its own.
      expect(call![1]).toEqual({ title: t('settings.backupExportDialogTitle') });
    });
    await waitFor(() => {
      expect(container.textContent).toContain(exportPath);
    });
  });

  // The overwrite confirmation is the Rust command's native message dialog (so
  // it behaves the same in the main window and a popped-out pane); the card's
  // only job is to hand it the localized copy and honour a decline.
  const importArgs = (container: HTMLElement) => {
    const call = invokeMock.mock.calls.find(([cmd]) => cmd === 'import_config');
    expect(call).toBeTruthy();
    return call![1];
  };

  it('passes the localized confirm copy, and a decline changes nothing', async () => {
    const { container } = await mountSettings();
    // A null import outcome is the command's "declined or dismissed" answer.
    armBackupCommands(null, null);
    const loadsBefore = invokeMock.mock.calls.filter(([cmd]) => cmd === 'load_config').length;

    await fireEvent.click(buttonByText(container, t('settings.backupImport')));

    await waitFor(() => {
      // Rust has no dictionary: every user-visible string of the confirmation
      // travels with the call, including both button labels (the plugin's own
      // defaults are English).
      expect(importArgs(container)).toEqual({
        title: t('settings.backupImportDialogTitle'),
        confirmBody: t('settings.backupConfirmOverwrite'),
        confirmOk: t('common.yes'),
        confirmCancel: t('common.no')
      });
    });
    // Drain the click's await chain before asserting on its absence: the
    // `waitFor` above returns as soon as the call is recorded, which is before
    // the card's continuation would have run. Two flushes (Svelte's update plus
    // the promise chain behind the mocked invoke) are deterministic and, unlike
    // a timer, do not tie the test to wall-clock time. Without this the
    // "no message" check could pass by winning a race — the positive half of
    // the pair is the next test, which does see `settings.backupImported`.
    await tick();
    await tick();
    // A decline is a clean no-op in the card: no reload, and no status message
    // (neither the imported path nor an error). That the file itself was left
    // alone is `declined_import_touches_nothing`'s business on the Rust side,
    // where the real files are. Scoped to the backup card: other cards now
    // carry `role="status"` lines of their own (the shortcuts rows, #676), and
    // an unscoped query would be asserting about *them*.
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === 'load_config').length).toBe(loadsBefore);
    const backupCard = buttonByText(container, t('settings.backupImport')).closest('.card');
    expect(backupCard?.querySelector('[role="status"]')).toBeNull();
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === 'import_config')).toHaveLength(1);
  });

  it('imports after confirmation and adopts the reloaded config', async () => {
    const { container } = await mountSettings();
    armBackupCommands(null, '/tmp/imported.json');
    const loadsBefore = invokeMock.mock.calls.filter(([cmd]) => cmd === 'load_config').length;

    await fireEvent.click(buttonByText(container, t('settings.backupImport')));

    await waitFor(() => {
      expect(importArgs(container)).toEqual({
        title: t('settings.backupImportDialogTitle'),
        confirmBody: t('settings.backupConfirmOverwrite'),
        confirmOk: t('common.yes'),
        confirmCancel: t('common.no')
      });
    });
    // The UI reloads from the file the import wrote rather than trusting a
    // pre-import copy — the store must come from `load_config`, not the
    // `ImportOutcome` payload alone.
    await waitFor(() => {
      expect(invokeMock.mock.calls.filter(([cmd]) => cmd === 'load_config').length).toBeGreaterThan(
        loadsBefore
      );
    });
    await waitFor(() => {
      expect(container.textContent).toContain('/tmp/imported.json');
    });
  });
});

/**
 * #678 (update-channel slice) — the release channel is a real config field:
 * the picker must both save the choice into the persisted payload and show
 * the persisted value on the next launch.
 *
 * Fails pre-fix: there is no channel field and no Updates card.
 */
describe('Settings update channel (#678)', () => {
  it('saves the chosen channel into the config payload', async () => {
    const { container } = await mountSettings();
    const select = container.querySelector('#update-channel') as HTMLSelectElement;
    expect(select.value).toBe('stable');

    await fireEvent.change(select, { target: { value: 'beta' } });
    await tick();
    // #966: the banner now carries its own Save as well — the end-of-form one
    // is what this test is about.
    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);

    // The harness's `save_config` echo returns exactly the payload it was
    // given (#297), so a store that settled on `beta` can only have received
    // it from the picker's saved payload.
    await waitFor(() => expect(get(configStore).updates.channel).toBe('beta'));
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'save_config')).toBe(true);
  });

  it('shows the persisted channel after a relaunch', async () => {
    const base = invokeMock.getMockImplementation()!;
    const beta = structuredClone(defaultConfig);
    beta.updates = { channel: 'beta' };
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) =>
      cmd === 'load_config' ? beta : base(cmd, args)
    );

    const { container } = await mountSettings();
    const select = container.querySelector('#update-channel') as HTMLSelectElement;
    expect(select.value).toBe('beta');
  });
});

/**
 * #733 — the profanity card used to render the "preview profane sample"
 * toggle twice against the same state and the same DOM id, so one `label[for]`
 * resolved to two controls and any `getElementById` lookup was ambiguous.
 *
 * Fails pre-fix: two elements carry `profanity-preview-sample` in both filter
 * states, and two labels point at it.
 */
describe('Settings profanity card a11y (#733)', () => {
  function profanityConfig(on: boolean) {
    const cfg = configuredConfig();
    cfg.teams.profanity_filter = on;
    return cfg;
  }

  /** The name a browser computes: aria-labelledby, aria-label, `label[for]`,
   * or an ancestor `<label>`. */
  function accessibleName(el: HTMLElement): string {
    const labelledBy = el.getAttribute('aria-labelledby');
    if (labelledBy) {
      return labelledBy
        .split(/\s+/)
        .map((id) => el.ownerDocument.getElementById(id)?.textContent?.trim() ?? '')
        .join(' ')
        .trim();
    }
    const ariaLabel = el.getAttribute('aria-label');
    if (ariaLabel) return ariaLabel.trim();
    if (el.id) {
      const forLabel = el.ownerDocument.querySelector(`label[for="${el.id}"]`);
      if (forLabel) return (forLabel.textContent ?? '').trim();
    }
    return (el.closest('label')?.textContent ?? '').trim();
  }

  for (const on of [true, false]) {
    it(`renders unique ids and names every checkbox (filter ${on ? 'on' : 'off'})`, async () => {
      configStore.set(profanityConfig(on));
      const { container } = await mountSettings();

      const ids = [...container.querySelectorAll<HTMLElement>('[id]')].map((el) => el.id);
      expect(ids.length).toBeGreaterThan(0);
      expect(new Set(ids).size).toBe(ids.length);

      const checkboxes = [
        ...container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')
      ];
      expect(checkboxes.length).toBeGreaterThan(0);
      for (const box of checkboxes) {
        expect(accessibleName(box)).not.toBe('');
      }
    });
  }

  it('associates the preview-sample toggle with exactly one label and one input', async () => {
    configStore.set(profanityConfig(true));
    const { container } = await mountSettings();

    const labels = [...container.querySelectorAll('label[for="profanity-preview-sample"]')];
    const boxes = [...container.querySelectorAll<HTMLInputElement>('#profanity-preview-sample')];
    expect(labels).toHaveLength(1);
    expect(boxes).toHaveLength(1);
    // The pair is the row the user sees, not two rows sharing one id.
    expect(boxes[0].closest('.toggle-row')?.contains(labels[0])).toBe(true);
  });
});

/**
 * #746 — every rule row is a `role="group"`; both rows of a list used to carry
 * the same `aria-label`, so rule 2 announced itself as rule 1 and the reorder
 * and Remove controls gave the user nothing to disambiguate them with.
 *
 * Fails pre-fix: the names come back as two bare labels with no position.
 */
describe('Settings rule group names (#746)', () => {
  function twoRulesConfig() {
    const cfg = configuredConfig();
    cfg.status_rules = {
      quiet_hours: [
        {
          enabled: true,
          start_minutes: 1320,
          end_minutes: 420,
          days: [],
          replacement_status: '',
          presence_availability: '',
          presence_activity: '',
          pause_polling: false
        },
        {
          enabled: false,
          start_minutes: 600,
          end_minutes: 660,
          days: [1],
          replacement_status: '',
          presence_availability: '',
          presence_activity: '',
          pause_polling: true
        }
      ],
      track_rules: [
        {
          enabled: true,
          artist_substring: 'first',
          track_substring: '',
          replacement_status: '',
          presence_availability: '',
          presence_activity: '',
          days: [],
          start_minutes: 0,
          end_minutes: 1440
        },
        {
          enabled: true,
          artist_substring: 'second',
          track_substring: '',
          replacement_status: '',
          presence_availability: '',
          presence_activity: '',
          days: [],
          start_minutes: 0,
          end_minutes: 1440
        }
      ]
    };
    return cfg;
  }

  it('names each row after its position, matching the Move buttons', async () => {
    configStore.set(twoRulesConfig());
    const { container } = await mountSettings();

    const groupNames = [...container.querySelectorAll('[role="group"]')].map(
      (el) => el.getAttribute('aria-label') ?? ''
    );

    expect(groupNames.filter((name) => name.startsWith(t('rules.quietHoursLabel')))).toEqual([
      `${t('rules.quietHoursLabel')} 1`,
      `${t('rules.quietHoursLabel')} 2`
    ]);
    expect(groupNames.filter((name) => name.startsWith(t('rules.trackRulesLabel')))).toEqual([
      `${t('rules.trackRulesLabel')} 1`,
      `${t('rules.trackRulesLabel')} 2`
    ]);

    // The row's ordinal is the one its reorder controls use.
    const upSecond = container.querySelector(
      'button[aria-label="' + t('rules.moveRuleUp', { n: 2 }) + '"]'
    ) as HTMLButtonElement;
    expect(upSecond.closest('[role="group"]')?.getAttribute('aria-label')).toBe(
      `${t('rules.trackRulesLabel')} 2`
    );
  });
});

/**
 * #748 — the live preview was a `polite` live region, so every typing pause
 * re-announced the whole rendered sample on top of the field's own echo.
 *
 * Fails pre-fix: `.preview-box` carries `aria-live="polite"` while the
 * template is being edited.
 */
describe('Settings status preview announcements (#748)', () => {
  it('exposes no live region while the template is being edited', async () => {
    const { container } = await mountSettings();
    const input = formatInput(container);
    input.focus();
    await fireEvent.input(input, { target: { value: '🎵 {artist} - {track}' } });
    await tick();

    const preview = container.querySelector('.preview-box') as HTMLElement;
    expect(preview).not.toBeNull();
    expect(preview.getAttribute('aria-live')).toBeNull();
    // A live region may also be implied by the role: this one has none.
    expect(preview.getAttribute('role')).toBeNull();
    // The sample itself is still in the reading order next to its label.
    expect(preview.previousElementSibling?.textContent?.trim()).toBe(t('settings.livePreview'));
  });
});

/**
 * #816 — every Teams failure path calls `setTeamsPhase('error', …)`, which is
 * exactly what clears `teamsAuthWaiting`; the block that rendered the message
 * lived inside the waiting branch, so the card reverted to a green Connected
 * badge and the same button with no reason shown.
 *
 * Fails pre-fix: with the phase at `error` nothing in the tree renders it.
 */
describe('Settings Teams reconnect failure (#816)', () => {
  it('shows the failure on the card and clears it on a fresh attempt', async () => {
    setTeamsPhase('error', 'device code rejected');
    const { container, getByRole } = await mountSettings();

    const teamsCard = [...container.querySelectorAll('section.card')].find(
      (card) => card.querySelector('h2')?.textContent?.trim() === t('settings.sectionTeams')
    ) as HTMLElement;
    // The badge is the pre-condition of the defect: still Connected.
    expect(teamsCard.querySelector('.badge')?.textContent).toContain(t('common.connected'));
    expect(teamsCard.querySelector('.error-message')?.textContent).toBe('device code rejected');

    // The retry the card offers clears the reason and starts a new attempt.
    await fireEvent.click(getByRole('button', { name: t('reconnect.reconnectTeams') }));
    await waitFor(() => expect(authFlow.teams.error).toBeNull());
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'reconnect_teams')).toBe(true);
  });
});

/**
 * #955 — the five availability/activity pairs were the one user-visible string
 * set in the rules UI that never went through `t()`: a German or French user
 * got an English dropdown inside an otherwise translated card.
 *
 * Fails pre-fix: the options render hardcoded English in every locale, and in
 * `de`/`fr` the rendered text is the English one rather than the dictionary's.
 */
describe('Settings presence option labels (#955)', () => {
  const PRESENCE_LABELS: readonly { key: TKey; wire: string }[] = [
    { key: 'rules.presenceAvailable', wire: 'Available|Available' },
    { key: 'rules.presenceBusyCall', wire: 'Busy|InACall' },
    { key: 'rules.presenceBusyConference', wire: 'Busy|InAConferenceCall' },
    { key: 'rules.presenceAway', wire: 'Away|Away' },
    { key: 'rules.presenceDndPresenting', wire: 'DoNotDisturb|Presenting' }
  ];

  it('renders the dictionary labels in en, de and fr, with the wire values unchanged', async () => {
    await i18n.set('en');
    const { container } = await mountSettings();

    // One row in each list, so both presence selects exist.
    const [addQuiet] = [...container.querySelectorAll('.btn-secondary')].filter(
      (b) => b.textContent?.trim() === t('rules.addQuietHours')
    );
    const [addTrack] = [...container.querySelectorAll('.btn-secondary')].filter(
      (b) => b.textContent?.trim() === t('rules.addTrackRule')
    );
    await fireEvent.click(addQuiet);
    await fireEvent.click(addTrack);
    await tick();

    for (const [locale, dict] of Object.entries({ en, de, fr }) as [Locale, Dict][]) {
      await i18n.set(locale);
      await tick();

      const selects = [...container.querySelectorAll('select')].filter(
        (select) => select.getAttribute('aria-label') === t('rules.presenceLabel')
      );
      expect(selects).toHaveLength(2);

      for (const select of selects) {
        // Index 0 is "don't change my presence"; the five pairs follow it.
        const pairs = [...select.querySelectorAll('option')].slice(1);
        // A key missing from any dictionary surfaces here as `undefined`.
        expect(pairs.map((o) => [o.getAttribute('value'), o.textContent?.trim()])).toEqual(
          PRESENCE_LABELS.map(({ key, wire }) => [wire, dict[key]])
        );
      }
    }

    await i18n.set('en');
  });
});

/**
 * #964 — the poller's `spotify-reconnect-required` event lands the user on
 * Settings with the flow already waiting, and the card offered only "Complete
 * authentication in the browser": `reconnectSpotify` refuses to restart a
 * waiting flow, so a lost browser tab was a dead end here (Reconnect has had
 * both escapes since #558).
 *
 * Fails pre-fix: neither control is in the waiting branch.
 */
describe('Settings Spotify waiting-state escape (#964)', () => {
  it('offers Restart sign-in, which begins a fresh flow instead of being refused', async () => {
    setSpotifyPhase('waiting');
    const { container, getByRole } = await mountSettings();

    expect(container.querySelector('#spotify-manual-url')).not.toBeNull();
    await fireEvent.click(getByRole('button', { name: t('reconnect.restartSignIn') }));

    await waitFor(() =>
      expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'reconnect_spotify_session')).toBe(true)
    );
    // The restart put the flow back in the waiting state, i.e. it ran.
    expect(authFlow.spotify.phase).toBe('waiting');
  });

  it('completes the flow from a pasted redirect URL, and refuses one without a code', async () => {
    setSpotifyPhase('waiting');
    const { container, getByRole } = await mountSettings();
    const input = container.querySelector('#spotify-manual-url') as HTMLInputElement;
    const submit = getByRole('button', { name: t('onboarding.submitCode') });

    await fireEvent.input(input, { target: { value: 'presencejam://callback?state=xyz' } });
    await fireEvent.click(submit);
    await waitFor(() =>
      expect(container.querySelector('.error-message')?.textContent).toBe(t('validation.noCodeInUrl'))
    );
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'complete_spotify_auth_manual')).toBe(false);

    await fireEvent.input(input, {
      target: { value: 'presencejam://callback?code=abc&state=xyz' }
    });
    await fireEvent.click(submit);
    await waitFor(() => {
      const call = invokeMock.mock.calls.find(([cmd]) => cmd === 'complete_spotify_auth_manual');
      expect(call?.[1]).toEqual({ code: 'abc', oauthState: 'xyz' });
    });
  });
});

/**
 * #965 — the connection row's `{#if} … {:else if waiting}` chain had no final
 * `{:else}`, so a disconnected Spotify account rendered the red "Not connected"
 * badge and no action at all, while the Teams card beside it falls through to
 * its own reconnect.
 *
 * Fails pre-fix: the disconnected row renders no control.
 */
describe('Settings disconnected Spotify card (#965)', () => {
  /** `get_sync_status` with Spotify down and Teams up. */
  function spotifyDisconnected() {
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) =>
      cmd === 'get_sync_status'
        ? {
            is_syncing: false,
            current_track: null,
            spotify_connected: false,
            teams_connected: true
          }
        : base(cmd, args)
    );
  }

  it('offers a reconnect when the account is disconnected', async () => {
    const cfg = configuredConfig();
    cfg.spotify.client_secret_state = 'present';
    configStore.set(cfg);
    spotifyDisconnected();
    const { container } = await mountSettings();

    // The first `.connection-row` is the Spotify card's.
    const row = container.querySelector('.connection-row') as HTMLElement;
    const action = row.querySelector('.btn-secondary') as HTMLButtonElement;
    expect(action?.textContent?.trim()).toBe(t('settings.reconnectSpotify'));

    await fireEvent.click(action);
    await waitFor(() =>
      expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'reconnect_spotify_session')).toBe(true)
    );
  });

  it('points at onboarding instead when no client secret is stored', async () => {
    const cfg = configuredConfig();
    cfg.spotify.client_secret_state = 'absent';
    configStore.set(cfg);
    spotifyDisconnected();
    const { container } = await mountSettings();

    const row = container.querySelector('.connection-row') as HTMLElement;
    const action = row.querySelector('.btn-secondary') as HTMLButtonElement;
    // A reconnect would open a browser flow that cannot finish: the secret it
    // starts from is not there.
    expect(action?.textContent?.trim()).toBe(t('settings.runOnboarding'));

    await fireEvent.click(action);
    await waitFor(() => expect(get(currentView)).toBe('onboarding'));
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'reconnect_spotify_session')).toBe(false);
  });
});

/**
 * #890 — dirty state used to be recovered by serialising the whole document
 * twice inside a `$derived` (`localConfig` vs `$configStore`), which every
 * write to the deep proxy invalidated: one keystroke in any field paid two full
 * `JSON.stringify` passes over the rules and the lexicon on the UI thread.
 *
 * Fails pre-fix: the input event re-serialises the config.
 */
describe('Settings dirty flag (#890)', () => {
  it('marks a text edit dirty without serialising the config', async () => {
    // The old detection was `JSON.stringify(cfg, replacer)`; nothing else in
    // this pane passes a replacer function.
    const stringifySpy = vi.spyOn(JSON, 'stringify');
    const serialisations = () =>
      stringifySpy.mock.calls.filter((call) => typeof call[1] === 'function').length;
    try {
      const { container } = await mountSettings();
      const before = serialisations();
      expect(container.querySelector('.dirty-banner')).toBeNull();

      await fireEvent.input(formatInput(container), { target: { value: '🎧 {track}' } });
      await tick();

      expect(container.querySelector('.dirty-banner')).not.toBeNull();
      expect(serialisations()).toBe(before);
    } finally {
      stringifySpy.mockRestore();
    }
  });

  it('clears the flag once the draft is saved, and keeps it when the save fails', async () => {
    const { container } = await mountSettings();
    await fireEvent.input(formatInput(container), { target: { value: '🎧 {track}' } });
    await tick();
    expect(container.querySelector('.dirty-banner')).not.toBeNull();

    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);
    await waitFor(() => expect(container.querySelector('.dirty-banner')).toBeNull());

    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'save_config') throw new Error('disk full');
      return base(cmd, args);
    });

    await fireEvent.input(formatInput(container), { target: { value: '🎧 {track} — {artist}' } });
    await tick();
    await fireEvent.click(container.querySelector('.actions .btn-full') as HTMLElement);
    await waitFor(() => expect(container.querySelector('.settings')?.textContent).toContain('disk full'));
    // A save that never happened must not clear the flag: Back still asks.
    expect(container.querySelector('.dirty-banner')).not.toBeNull();
  });

  it('does not flag the draft for a control that applies itself', async () => {
    const { container } = await mountSettings();
    const density = container.querySelector('#compact-density') as HTMLInputElement;

    await fireEvent.click(density);
    await tick();

    // Spacing is applied and stored by its own store — there is nothing for
    // Save to commit, so the banner must not claim otherwise.
    expect(container.querySelector('.dirty-banner')).toBeNull();
  });
});

/**
 * #966 — the banner was a status line: its actions appeared only when a
 * navigation was blocked, so the only Save button sat at the end of the
 * twelve-card form and nothing anywhere restored the last saved values.
 *
 * Fails pre-fix: the banner holds no control while the form is merely dirty,
 * and no revert action exists.
 */
describe('Settings unsaved-changes banner actions (#966)', () => {
  it('saves from the banner without scrolling to the end of the form', async () => {
    const { container } = await mountSettings();
    await fireEvent.input(formatInput(container), { target: { value: '🎧 {track}' } });
    await tick();

    const banner = container.querySelector('.dirty-banner') as HTMLElement;
    const save = banner.querySelector('.btn-secondary') as HTMLButtonElement;
    expect(save?.textContent?.trim()).toBe(t('settings.saveChanges'));

    await fireEvent.click(save);
    await waitFor(() => expect(get(configStore).teams.status_format).toBe('🎧 {track}'));
    expect(container.querySelector('.dirty-banner')).toBeNull();
  });

  it('reverts the draft to the last saved values', async () => {
    const { container } = await mountSettings();
    const input = formatInput(container);
    const stored = get(configStore).teams.status_format;

    await fireEvent.input(input, { target: { value: '🎧 changed' } });
    await tick();
    expect(input.value).toBe('🎧 changed');

    const banner = container.querySelector('.dirty-banner') as HTMLElement;
    const revert = [...banner.querySelectorAll('button')].find(
      (button) => button.textContent?.trim() === t('settings.revertChanges')
    ) as HTMLButtonElement;
    await fireEvent.click(revert);
    await tick();

    expect(input.value).toBe(stored);
    expect(container.querySelector('.dirty-banner')).toBeNull();
    // Reverting is not a save: nothing is written back.
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'save_config')).toBe(false);
  });
});

/**
 * #973 (cross-slice, absorbed here): `get_spotify_granted_scopes` answers
 * `null` when the stored token's scopes could not be decoded — no token, or a
 * payload the backend could not read. That is an unknown set, not an empty one,
 * and rendering it as "the playback scope is missing" tells the user to
 * reconnect over something the app cannot know.
 */
describe('Settings playback-scope banner and an undecodable token (#973)', () => {
  function scopesAnswer(answer: string[] | null) {
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) =>
      cmd === 'get_spotify_granted_scopes' ? answer : base(cmd, args)
    );
  }

  it('stays quiet when the scopes could not be decoded', async () => {
    scopesAnswer(null);
    const { container } = await mountSettings();
    expect(container.textContent).not.toContain(t('settings.playbackScopeBanner'));
  });

  it('shows the banner for a decoded token that lacks the scope', async () => {
    scopesAnswer(['playlist-read-private']);
    const { container } = await mountSettings();
    expect(container.textContent).toContain(t('settings.playbackScopeBanner'));
  });

  it('stays quiet when the scope is granted', async () => {
    scopesAnswer(['user-modify-playback-state']);
    const { container } = await mountSettings();
    expect(container.textContent).not.toContain(t('settings.playbackScopeBanner'));
  });
});
