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
      '[role="group"][aria-label="' + t('rules.trackRulesLabel') + '"]'
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
